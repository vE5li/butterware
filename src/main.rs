#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr)]
#![feature(concat_idents)]
#![feature(iter_next_chunk)]
#![allow(incomplete_features)]

use core::convert::Infallible;
use core::ops::ControlFlow;

// global logger
use embassy_executor::Spawner;
use embassy_nrf as _; // time driver
use embassy_nrf::config::{HfclkSource, LfclkSource};
use embassy_nrf::interrupt;
use embassy_time::driver::now;
use embassy_time::{Duration, Timer};
use embedded_storage_async::nor_flash::{NorFlash, ReadNorFlash};
use futures::future::{select, Either};
use futures::pin_mut;
use hardware::{DebouncedKey, ScanPins};
use keys::Mapping;
use nrf_softdevice::ble::{central, gatt_server, peripheral, set_address, Address, Connection};
use nrf_softdevice::{random_bytes, raw, Flash, Softdevice};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod ble;
mod hardware;
#[allow(unused)]
mod keys;
#[macro_use]
mod interface;
#[path = "../keyboards/mod.rs"]
mod keyboards;

use ble::Server;
use keyboards::Used;

use crate::ble::{AdvertisingData, Bonder, KEYBOARD_ICON};
use crate::hardware::ActiveLayer;
use crate::interface::{Keyboard, KeyboardExtension, Scannable};

#[cfg(all(feature = "left", feature = "right"))]
compile_error!("Only one side can be built for at a time. Try disabling either the left or right feature.");

#[cfg(not(any(feature = "left", feature = "right")))]
compile_error!("No side to compile for was selected. Try enabling the left or right feature.");

mod flash {
    use core::mem::MaybeUninit;

    use elain::Align;

    const SETTINGS_PAGES: usize = 1;

    #[repr(C)]
    pub struct SettingsFlash {
        _align: Align<{ embassy_nrf::nvmc::PAGE_SIZE }>,
        _data: [u8; embassy_nrf::nvmc::PAGE_SIZE * SETTINGS_PAGES],
    }

    #[link_section = ".flash_storage"]
    pub static SETTINGS_FLASH: MaybeUninit<SettingsFlash> = MaybeUninit::uninit();
}

#[embassy_executor::task]
async fn flash_task(flash: Flash) {
    let address = &flash::SETTINGS_FLASH as *const _ as u32;

    defmt::error!("working on flash at address 0x{:x}", &flash::SETTINGS_FLASH as *const _);

    pin_mut!(flash);

    let mut buffer = [0u8; 256];
    defmt::warn!("starting read");
    defmt::unwrap!(flash.read(address, &mut buffer).await);
    defmt::warn!("done with read. value: {:x}", buffer);

    defmt::warn!("start erase page");
    defmt::unwrap!(flash.erase(address, address + embassy_nrf::nvmc::PAGE_SIZE as u32).await);
    defmt::warn!("done erase page");

    defmt::warn!("starting write");
    defmt::unwrap!(flash.write(address, &mut [0x15; 256]).await);
    defmt::warn!("done with write");

    defmt::warn!("starting read");
    defmt::unwrap!(flash.read(address, &mut buffer).await);
    defmt::warn!("done with read. value: {:x}", buffer);
}

// TODO: rename to BitOperations or similar
trait TestBit {
    fn test_bit(self, offset: usize) -> bool;

    fn clear_bit(&mut self, offset: usize);

    fn set_bit(&mut self, offset: usize);
}

impl TestBit for u64 {
    fn test_bit(self, offset: usize) -> bool {
        (self >> offset) & 0b1 != 0
    }

    fn clear_bit(&mut self, offset: usize) {
        *self &= !(1 << offset);
    }

    fn set_bit(&mut self, offset: usize) {
        *self |= 1 << offset;
    }
}

// TODO: maybe use embassy::nrf::rng instead?
async fn generate_random_u32(softdevice: &Softdevice) -> u32 {
    loop {
        let mut count = 0u8;
        unsafe { raw::sd_rand_application_bytes_available_get(&mut count as *mut u8) };

        if count >= 4 {
            let mut buffer = [0; 4];
            let result = random_bytes(softdevice, &mut buffer);
            return u32::from_le_bytes(buffer);
        }

        Timer::after(Duration::from_millis(5)).await;
    }
}

#[nrf_softdevice::gatt_client(uuid = "5a7ef8bc-de9e-11ed-b5ea-0242ac120002")]
struct MasterServiceClient {
    #[characteristic(uuid = "66762370-de9e-11ed-b5ea-0242ac120002", read, write)]
    other_random_number: u32,
    #[characteristic(uuid = "734e5e64-de9e-11ed-b5ea-0242ac120002", read)]
    is_master: bool,
}

#[nrf_softdevice::gatt_service(uuid = "5a7ef8bc-de9e-11ed-b5ea-0242ac120002")]
struct MasterService {
    #[characteristic(uuid = "66762370-de9e-11ed-b5ea-0242ac120002", read, write)]
    other_random_number: u32,
    #[characteristic(uuid = "734e5e64-de9e-11ed-b5ea-0242ac120002", read)]
    is_master: bool,
}

#[nrf_softdevice::gatt_server]
struct MasterServer {
    master_service: MasterService,
}

#[nrf_softdevice::gatt_client(uuid = "c78c4d70-e02d-11ed-b5ea-0242ac120002")]
struct KeyStateServiceClient {
    #[characteristic(uuid = "d8004dfa-e02d-11ed-b5ea-0242ac120002", write)]
    key_state: u64,
}

#[nrf_softdevice::gatt_service(uuid = "c78c4d70-e02d-11ed-b5ea-0242ac120002")]
struct KeyStateService {
    #[characteristic(uuid = "d8004dfa-e02d-11ed-b5ea-0242ac120002", write)]
    key_state: u64,
}

#[nrf_softdevice::gatt_server]
struct KeyStateServer {
    key_state_service: KeyStateService,
}

async fn advertise_determine_master(softdevice: &Softdevice, server: &MasterServer, adv_data: &[u8], scan_data: &[u8]) -> bool {
    let config = peripheral::Config::default();
    let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };

    defmt::debug!("start advertising");

    let connection = defmt::unwrap!(peripheral::advertise_connectable(softdevice, adv, &config).await);

    defmt::debug!("connected to other half");

    let random_number = generate_random_u32(softdevice).await;

    defmt::debug!("random number is {}", random_number);

    let mut is_master = false;
    let _ = gatt_server::run(&connection, server, |event| match event {
        MasterServerEvent::MasterService(event) => match event {
            MasterServiceEvent::OtherRandomNumberWrite(other_random_number) => {
                // Determine which side is the master based on our random numbers.
                is_master = random_number > other_random_number;

                defmt::debug!("other random number is {}", other_random_number);

                // Update is_master so that the other side can read it.
                defmt::unwrap!(server.master_service.is_master_set(&is_master));
            }
        },
    })
    .await;

    is_master
}

async fn connect_determine_master(softdevice: &Softdevice, address: &Address) -> bool {
    let addresses = [address];
    let mut config = central::ConnectConfig::default();
    config.scan_config.whitelist = Some(&addresses);

    defmt::debug!("start scanning");

    let connection = defmt::unwrap!(central::connect(softdevice, &config).await);
    let client: MasterServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&connection).await);

    defmt::debug!("connected to other half");

    // FIX:
    //let random_number = generate_random_u32(softdevice).await;
    let random_number = !0;

    defmt::debug!("random number is {}", random_number);
    defmt::debug!("writing random number to the master service");

    defmt::unwrap!(client.other_random_number_write(&random_number).await);

    defmt::debug!("reading is_master from the master service");

    !defmt::unwrap!(client.is_master_read().await)
}

async fn do_slave<'a>(
    softdevice: &Softdevice,
    pins: &mut ScanPins<'a, { Used::COLUMNS }, { Used::ROWS }>,
    address: &Address,
) -> Result<Infallible, HalfDisconnected> {
    let addresses = [address];
    let mut config = central::ConnectConfig::default();
    config.scan_config.whitelist = Some(&addresses);
    config.conn_params.min_conn_interval = 6;
    config.conn_params.max_conn_interval = 6;

    let mut keyboard_state = SlaveState::<Used>::new();

    defmt::debug!("stating slave");

    let master_connection = defmt::unwrap!(central::connect(softdevice, &config).await);
    let client: KeyStateServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&master_connection).await);

    defmt::info!("connected to other half");

    use_slave(&mut keyboard_state, pins, client).await
}

async fn do_master<'a>(
    softdevice: &Softdevice,
    server: &Server<'a>,
    key_state_server: &KeyStateServer,
    bonder: &'static Bonder,
    adv_data: &[u8],
    scan_data: &[u8],
    pins: &mut ScanPins<'a, { Used::COLUMNS }, { Used::ROWS }>,
) -> Result<Infallible, HalfDisconnected> {
    defmt::debug!("stating master");

    // Connect to the other half
    let config = peripheral::Config::default();
    let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
    let slave_connection = defmt::unwrap!(peripheral::advertise_connectable(softdevice, adv, &config).await);

    defmt::info!("connected to other half");

    // Set unified address.
    set_address(softdevice, &Used::ADDRESS);

    let mut keyboard_state = MasterState::<Used>::new();

    loop {
        // Advertise
        let config = peripheral::Config::default();
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
        let host_connection = defmt::unwrap!(peripheral::advertise_pairable(softdevice, adv, &config, bonder).await);

        defmt::warn!("connected");

        // Run until the host disconnects.
        use_master(
            &mut keyboard_state,
            pins,
            server,
            key_state_server,
            &slave_connection,
            &host_connection,
        )
        .await?;
    }
}

trait KeyState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    fn key(&mut self, column: usize, row: usize) -> &mut hardware::DebouncedKey<K>;

    /// Update the key state and check if the external state needs to be
    /// updated.
    fn update_needs_synchronize(&mut self, new_state: u64) -> bool;
}

pub struct MasterState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    active_layers: heapless::Vec<ActiveLayer, { K::MAXIMUM_ACTIVE_LAYERS }>,
    keys: [[hardware::DebouncedKey<K>; K::ROWS]; K::COLUMNS],
    previous_key_state: u64,
    previous_raw_state: u64,
    slave_raw_state: u64,
    state_mask: u64,
    lock_mask: u64,
}

impl<K> KeyState<K> for MasterState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    fn key(&mut self, column: usize, row: usize) -> &mut hardware::DebouncedKey<K> {
        &mut self.keys[column][row]
    }

    fn update_needs_synchronize(&mut self, new_state: u64) -> bool {
        let changed = self.previous_raw_state != new_state;
        self.previous_raw_state = new_state;
        changed
    }
}

impl<K> MasterState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    const DEFAULT_KEY: DebouncedKey<K> = DebouncedKey::new();
    const DEFAULT_ROW: [DebouncedKey<K>; K::ROWS] = [Self::DEFAULT_KEY; K::ROWS];

    pub const fn new() -> Self {
        Self {
            active_layers: heapless::Vec::new(),
            keys: [Self::DEFAULT_ROW; K::COLUMNS],
            previous_key_state: 0,
            previous_raw_state: 0,
            slave_raw_state: 0,
            state_mask: !0,
            lock_mask: 0,
        }
    }

    pub fn current_layer_index(&self) -> usize {
        self.active_layers.last().map(|layer| layer.layer_index).unwrap_or(0)
    }

    pub fn apply(&mut self, mut key_state: u64) -> Option<(usize, u64, u64)> {
        let mut injected_keys = 0;

        // TODO: make key_state immutable and copy to modify instead.
        let saved_state = key_state;

        // We do this before popping the layers to avoid clearing the mask instantly.
        self.lock_mask &= key_state;

        // Try to pop layers
        while let Some(active_layer) = self.active_layers.last() {
            let key_index = active_layer.key_index;

            match key_state.test_bit(key_index) {
                true => break,
                false => {
                    // Check if we want to execute the tap action for this layer (if
                    // present).
                    if matches!(active_layer.tap_timer, Some(time) if now() - time < Used::TAP_TIME) {
                        injected_keys.set_bit(key_index);
                    }

                    self.active_layers.pop();

                    // We lock all keys except the layer keys. This avoids
                    // cases where we leave a layer while holding a key and we
                    // send the key again but from the lower layer.
                    self.lock_mask = self.state_mask & saved_state;

                    // Add layer key to the mask again (re-enable the key).
                    self.state_mask.set_bit(key_index);

                    // For now we unset all non-layer keys so we don't get any key
                    // presses form the current layer.
                    key_state &= !self.state_mask;
                }
            }
        }

        // Ignore all keys that are held as part of a layer.
        key_state &= self.state_mask;

        // Ignore all locked keys.
        key_state &= !self.lock_mask;

        if key_state | injected_keys != self.previous_key_state {
            // FIX: unclear what happens if we press multiple layer keys on the same
            // event

            let active_layer = Used::LAYER_LOOKUP[self.current_layer_index()];

            for key_index in 0..Used::KEY_COUNT * 2 {
                // Get layer index and optional tap key.
                let (layer_index, tap_timer) = match active_layer[Used::MATRIX[key_index]] {
                    Mapping::Key(..) => continue,
                    Mapping::Layer(layer_index) => (layer_index, None),
                    Mapping::TapLayer(layer_index, _) => (layer_index, Some(now())),
                };

                // Make sure that the same layer is not pushed twice in a row
                if key_state.test_bit(key_index) {
                    // If we already have an active layer, we set it's timer to `None` to prevent
                    // the tap action from executing if both layer
                    // keys are released quickly.
                    if let Some(active_layer) = self.active_layers.last_mut() {
                        active_layer.tap_timer = None;
                    }

                    let new_active_layer = ActiveLayer {
                        layer_index,
                        key_index,
                        tap_timer,
                    };

                    self.active_layers.push(new_active_layer).expect("Active layer limit reached");

                    // Remove the key from the state mask (disable the key). This
                    // helps cut down on expensive updates and also ensures that we
                    // don't get any modifier keys in send_input_report.
                    self.state_mask.clear_bit(key_index);

                    // We lock all keys except the layer keys. This avoids
                    // cases where we enter a layer while holding a key and we
                    // send the key again but from the new layer.
                    self.lock_mask = self.state_mask & saved_state;

                    // For now we just set the entire key_state to 0
                    key_state = 0;
                }
            }

            // If the key state is not zero, that there is at least one non-layer
            // button pressed, since layer keys are masked out.
            if key_state != 0 {
                // If a regular key is pressed and there is an active layer, we set it's timer
                // to `None` to prevent the tap action from
                // executing if the layer key is released quickly.
                if let Some(active_layer) = self.active_layers.last_mut() {
                    active_layer.tap_timer = None;
                }
            }

            // Since we might have altered the key state we check again if it changed
            // to avoid sending the same input report multiple times.
            if key_state | injected_keys != self.previous_key_state {
                self.previous_key_state = key_state;

                return Some((self.current_layer_index(), key_state, injected_keys));
            }
        }

        None
    }
}

pub struct SlaveState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    pub keys: [[hardware::DebouncedKey<K>; K::ROWS]; K::COLUMNS],
    pub previous_key_state: u64,
}

impl<K> KeyState<K> for SlaveState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    fn key(&mut self, column: usize, row: usize) -> &mut hardware::DebouncedKey<K> {
        &mut self.keys[column][row]
    }

    fn update_needs_synchronize(&mut self, new_state: u64) -> bool {
        let changed = self.previous_key_state != new_state;
        self.previous_key_state = new_state;
        changed
    }
}

impl<K> SlaveState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    const DEFAULT_KEY: DebouncedKey<K> = DebouncedKey::new();
    const DEFAULT_ROW: [DebouncedKey<K>; K::ROWS] = [Self::DEFAULT_KEY; K::ROWS];

    pub const fn new() -> Self {
        Self {
            keys: [Self::DEFAULT_ROW; K::COLUMNS],
            previous_key_state: 0,
        }
    }
}

async fn do_scan<'a, K>(state: &mut impl KeyState<K>, pins: &mut ScanPins<'a, { K::COLUMNS }, { K::ROWS }>) -> u64
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    loop {
        let mut key_state = 0;
        let mut offset = 0;

        for (column_index, column) in pins.columns.iter_mut().enumerate() {
            column.set_high();

            for (row_index, row) in pins.rows.iter().enumerate() {
                let raw_state = row.is_high();
                state.key(column_index, row_index).update(raw_state);

                key_state |= (state.key(column_index, row_index).is_down() as u64) << offset;
                offset += 1;
            }

            column.set_low();
        }

        if state.update_needs_synchronize(key_state) {
            return key_state;
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}

struct HalfDisconnected;

// TODO: rename function
async fn use_slave<'a, K>(
    state: &mut SlaveState<K>,
    pins: &mut ScanPins<'a, { K::COLUMNS }, { K::ROWS }>,
    client: KeyStateServiceClient,
) -> Result<Infallible, HalfDisconnected>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    loop {
        // Returns any time there is any change in the key state. This state is already
        // debounced.
        let raw_state = do_scan(state, pins).await;

        // Update the key state on the master.
        defmt::info!("started to write");
        defmt::unwrap!(client.key_state_write(&raw_state).await);
        defmt::info!("done writing");
    }
}

// TODO: rename function
async fn use_master<'a, K>(
    state: &mut MasterState<K>,
    pins: &mut ScanPins<'a, { K::COLUMNS }, { K::ROWS }>,
    server: &Server<'_>,
    key_state_server: &KeyStateServer,
    slave_connection: &Connection,
    host_connection: &Connection,
) -> Result<(), HalfDisconnected>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    let host_future = gatt_server::run(host_connection, server, |_| {});
    pin_mut!(host_future);

    loop {
        let inner_future = async {
            loop {
                let previous_raw_state = state.previous_raw_state;
                let slave_raw_state = state.slave_raw_state;

                let (key_state, slave_raw_state) = {
                    // Create futures.
                    let scan_future = do_scan(state, pins);
                    let slave_future = gatt_server::run_until(slave_connection, key_state_server, |event| match event {
                        KeyStateServerEvent::KeyStateService(event) => match event {
                            KeyStateServiceEvent::KeyStateWrite(key_state) => ControlFlow::Break(key_state),
                        },
                    });

                    // Pin futures so we can call select on them.
                    pin_mut!(scan_future);
                    pin_mut!(slave_future);

                    match select(scan_future, slave_future).await {
                        // Master side state changed.
                        Either::Left((key_state, _)) => {
                            #[cfg(feature = "left")]
                            let combined_state = slave_raw_state | (key_state << K::KEY_COUNT);

                            #[cfg(feature = "right")]
                            let combined_state = (slave_raw_state << K::KEY_COUNT) | key_state;

                            (combined_state, slave_raw_state)
                        }
                        // Slave side state changed.
                        Either::Right((key_state, _)) => {
                            let key_state = key_state.map_err(|_| HalfDisconnected)?;

                            #[cfg(feature = "left")]
                            let combined_state = (previous_raw_state << K::KEY_COUNT) | key_state;

                            #[cfg(feature = "right")]
                            let combined_state = previous_raw_state | (key_state << K::KEY_COUNT);

                            (combined_state, key_state)
                        }
                    }
                };

                // We do this update down here because we cannot mutably access the state inside
                // of the scope above.
                state.slave_raw_state = slave_raw_state;

                if let Some(output_state) = state.apply(key_state) {
                    return Ok(output_state);
                }
            }
        };
        pin_mut!(inner_future);

        match select(host_future, inner_future).await {
            // Keyboard disconnected from host, so just return.
            Either::Left(..) => return Ok(()),
            // There is a change in the output state of the keyboard so we need to send a new input
            // report.
            Either::Right((result, passed_host_future)) => {
                let (active_layer, key_state, injected_keys) = result?;

                server.send_input_report::<K>(&host_connection, active_layer, key_state | injected_keys);

                if injected_keys != 0 {
                    server.send_input_report::<K>(&host_connection, active_layer, key_state);
                }

                host_future = passed_host_future;
            }
        }
    }
}

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    // First we get the peripherals access crate.
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = interrupt::Priority::P2;
    config.time_interrupt_priority = interrupt::Priority::P2;
    config.hfclk_source = HfclkSource::ExternalXtal;
    config.lfclk_source = LfclkSource::ExternalXtal;
    let peripherals = embassy_nrf::init(config);

    let mut meboard = Used::new();
    let mut pins = meboard.init_peripherals(peripherals).to_pins();

    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 6,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 256 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t { attr_tab_size: 32768 }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 3,
            central_role_count: 3,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: Used::DEVICE_NAME as *const u8 as _,
            current_len: Used::DEVICE_NAME.len() as u16,
            max_len: Used::DEVICE_NAME.len() as u16,
            write_perm: unsafe { core::mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(raw::BLE_GATTS_VLOC_STACK as u8),
        }),
        ..Default::default()
    };

    let softdevice = Softdevice::enable(&config);
    let mut server = defmt::unwrap!(Server::new(softdevice));
    // TODO: move this into the other thing too
    let key_state_server = defmt::unwrap!(KeyStateServer::new(softdevice));
    #[cfg(feature = "left")]
    let master_server = defmt::unwrap!(MasterServer::new(softdevice));
    server.set_softdevice(softdevice);
    defmt::unwrap!(spawner.spawn(softdevice_task(softdevice)));

    let flash = Flash::take(softdevice);
    let channel = embassy_sync::channel::Channel::<embassy_sync::blocking_mutex::raw::NoopRawMutex, u8, 8>::new();

    let sender = channel.sender();
    let receiver = channel.receiver();

    defmt::unwrap!(spawner.spawn(flash_task(flash)));

    const ADVERTISING_DATA: AdvertisingData = AdvertisingData::new()
        .add_flags(raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8)
        .add_services(&[0x09, 0x18])
        .add_name(Used::DEVICE_NAME)
        .add_appearance(KEYBOARD_ICON);

    #[rustfmt::skip]
    let scan_data = &[
        0x03, 0x03, 0x09, 0x18,
    ];

    static BONDER: StaticCell<Bonder> = StaticCell::new();
    let bonder = BONDER.init(Bonder::default());

    loop {
        // Set a well-defined address that the other half can connect to.
        #[cfg(feature = "left")]
        set_address(softdevice, &Used::LEFT_ADDRESS);
        #[cfg(feature = "right")]
        set_address(softdevice, &Used::RIGHT_ADDRESS);

        // Both sides will connect, initially with the left side as the server and the
        // right as peripheral. Afterwards the will randomly determine which side is the
        // master and drop the connection again.
        #[cfg(feature = "left")]
        let is_master = advertise_determine_master(softdevice, &master_server, ADVERTISING_DATA.get_slice(), scan_data).await;
        #[cfg(feature = "right")]
        let is_master = connect_determine_master(softdevice, &Used::LEFT_ADDRESS).await;

        defmt::debug!("is master: {}", is_master);

        match is_master {
            true => {
                do_master(
                    softdevice,
                    &server,
                    &key_state_server,
                    bonder,
                    ADVERTISING_DATA.get_slice(),
                    scan_data,
                    &mut pins,
                )
                .await
            }
            false => {
                #[cfg(feature = "left")]
                const MASTER_ADDRESS: Address = Used::RIGHT_ADDRESS;
                #[cfg(feature = "right")]
                const MASTER_ADDRESS: Address = Used::LEFT_ADDRESS;

                do_slave(softdevice, &mut pins, &MASTER_ADDRESS).await
            }
        };

        defmt::error!("halves disconnected");

        #[cfg(not(feature = "auto-reset"))]
        run_disconnected_animation().await;
    }
}
