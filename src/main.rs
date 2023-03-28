#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr)]
#![feature(concat_idents)]
#![feature(iter_next_chunk)]
#![feature(const_mut_refs)]

use defmt_rtt as _; // global logger
use embassy_executor::Spawner;
use embassy_nrf as _; // time driver
use embassy_nrf::config::{HfclkSource, LfclkSource};
use embassy_nrf::gpio::{AnyPin, Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::peripherals::SPI3;
use embassy_nrf::spim::{Config, Spim};
use embassy_nrf::{interrupt, Peripherals};
use embassy_time::{Duration, Timer};
use futures::future::{select, Either};
use futures::pin_mut;
use nrf_softdevice::ble::{gatt_server, peripheral};
use nrf_softdevice::{raw, Softdevice};
use panic_probe as _;
use static_cell::StaticCell;

mod ble;
use ble::Server;

#[allow(unused)]
mod keys;
use keys::*;

macro_rules! register_layers {
    ($board:ident, $layers:ident, [$($names:ident),*]) => {
        struct $layers;

        impl $layers {
            $(pub const $names: Layer = Layer(${index()});)*
            pub const LAYER_LOOKUP: &'static [&'static [Mapping; <$board as Scannable>::COLUMNS * <$board as Scannable>::ROWS]] = &[$(&$board::$names),*];
        }
    };
}

#[path = "../keyboards/mod.rs"]
mod keyboards;
use keyboards::Used;

use crate::ble::Bonder;

/*macro_rules! layout {
    ($($names:ident,)*) => {{
        &[$(core::concat_idents!(KEY_, $names)),*]
    }};
}*/

/*macro_rules! matrix {
    ($($lookup:expr,)*) => {
        const _MATRIX: &[usize] = {
            &[$($lookup),*]
        };
    };
}*/

trait UnwrapInfelliable {
    type Output;

    fn unwrap_infelliable(self) -> Self::Output;
}

impl<T, E> UnwrapInfelliable for Result<T, E> {
    type Output = T;

    fn unwrap_infelliable(self) -> Self::Output {
        match self {
            Ok(value) => value,
            Err(..) => unreachable!(),
        }
    }
}

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

pub trait Scannable {
    const COLUMNS: usize;
    const ROWS: usize;
    const NAME_LENGTH: usize;
}

pub trait Keyboard: Scannable
where
    [(); Self::NAME_LENGTH]:,
    [(); Self::COLUMNS * Self::ROWS]:,
{
    const DEVICE_NAME: &'static [u8; Self::NAME_LENGTH];

    const MATRIX: [usize; Self::COLUMNS * Self::ROWS];

    const LAYER_LOOKUP: &'static [&'static [Mapping; Self::COLUMNS * Self::ROWS]];

    // 32768 Ticks per second on the nice!nano. 100 Ticks is around 3 milliseconds
    const DEBOUNCE_TICKS: u64 = 100;

    fn new() -> Self;

    fn init_peripherals(&mut self, peripherals: Peripherals) -> ScanPinConfig<{ Self::COLUMNS }, { Self::ROWS }>;
}

pub struct SpiConfig {
    interface: SPI3,
    interrupt: embassy_nrf::interrupt::SPIM3,
    clock_pin: AnyPin,
    mosi_pin: AnyPin,
    config: Config,
}

pub struct ScanPinConfig<const C: usize, const R: usize> {
    columns: [AnyPin; C],
    rows: [AnyPin; R],
    power_pin: Option<AnyPin>,
    spi_config: Option<SpiConfig>,
}

impl<const C: usize, const R: usize> ScanPinConfig<C, R> {
    pub fn to_pins(self) -> ScanPins<'static, C, R> {
        let columns = self
            .columns
            .into_iter()
            .map(|pin| Output::new(pin, Level::Low, OutputDrive::Standard))
            .next_chunk()
            .unwrap_infelliable();

        let rows = self
            .rows
            .into_iter()
            .map(|pin| Input::new(pin, Pull::Down))
            .next_chunk()
            .unwrap_infelliable();

        let power_pin = self.power_pin.map(|pin| Output::new(pin, Level::High, OutputDrive::Standard));

        let spi = self.spi_config.map(|config| {
            Spim::new_txonly(
                config.interface,
                config.interrupt,
                config.clock_pin,
                config.mosi_pin,
                config.config,
            )
        });

        ScanPins {
            columns,
            rows,
            power_pin,
            spi,
        }
    }
}

pub struct ScanPins<'a, const C: usize, const R: usize> {
    columns: [Output<'a, AnyPin>; C],
    rows: [Input<'a, AnyPin>; R],
    power_pin: Option<Output<'a, AnyPin>>,
    spi: Option<Spim<'a, SPI3>>,
}

#[derive(Debug, Clone, Copy)]
pub struct DebouncedKey<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    last_state_change: u64,
    internal_state: bool,
    output_state: bool,
    phantom_data: core::marker::PhantomData<K>,
}

impl<K> DebouncedKey<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    pub const fn new() -> Self {
        Self {
            last_state_change: 0,
            internal_state: false,
            output_state: false,
            phantom_data: core::marker::PhantomData,
        }
    }

    pub fn update(&mut self, new_state: bool) {
        const INTEGER_STATE: [u64; 2] = [0x0, !0x0];
        const BOOL_STATE: [bool; 2] = [false, true];

        let now = embassy_time::driver::now();

        // Branchless set of last_state_change. If new_state != internal_state
        // last_state_change will be set to now, otherwise it remains unchanged.
        let state_changed = self.internal_state != new_state;
        self.last_state_change =
            (INTEGER_STATE[(!state_changed) as usize] & self.last_state_change) | (INTEGER_STATE[state_changed as usize] & now);

        self.internal_state = new_state;

        // Branchless set of output_state. If the number of ticks since the last state
        // change is greater that the debounce ticks we set output_state =
        // internal_state.
        let debounced = now - self.last_state_change >= K::DEBOUNCE_TICKS;
        self.output_state =
            (BOOL_STATE[(!debounced) as usize] && self.output_state) || (BOOL_STATE[debounced as usize] && self.internal_state);
    }
}

pub struct KeyboardState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    //active_layers: heapless::Vec<u8, 6>,
    keys: [[DebouncedKey<K>; K::ROWS]; K::COLUMNS],
    previous_key_state: u64,
}

impl<K> KeyboardState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    const DEFAULT_KEY: DebouncedKey<K> = DebouncedKey::new();
    const DEFAULT_ROW: [DebouncedKey<K>; K::ROWS] = [Self::DEFAULT_KEY; K::ROWS];

    pub const fn new() -> Self {
        Self {
            //active_layers: heapless::Vec::new(),
            keys: [Self::DEFAULT_ROW; K::COLUMNS],
            previous_key_state: 0,
        }
    }
}

const MAXIMUM_ADVERTISE_LENGTH: usize = 31;

struct AdvertisingData<const C: usize> {
    data: [u8; MAXIMUM_ADVERTISE_LENGTH],
}

impl AdvertisingData<0> {
    pub const fn new() -> Self {
        Self {
            data: [0; MAXIMUM_ADVERTISE_LENGTH],
        }
    }
}

impl<const C: usize> AdvertisingData<C> {
    fn add_internal<const N: usize>(mut self, element_type: u8, element_data: &[u8]) -> AdvertisingData<{ N }> {
        self.data[C] = (N - C - 1) as u8;
        self.data[C + 1] = element_type;

        for (index, byte) in element_data.iter().copied().enumerate() {
            if C + 2 + index > N {
                defmt::warn!("Advertising element is bigger than the const generic implies, not all data will be copied");
                break;
            }

            self.data[C + 2 + index] = byte;
        }

        AdvertisingData { data: self.data }
    }

    pub fn add_flags(self, flags: u8) -> AdvertisingData<{ C + 3 }> {
        self.add_internal::<{ C + 3 }>(0x1, &[flags])
    }

    pub fn add_services<const A: usize>(self, services: &[u8; A]) -> AdvertisingData<{ C + A + 2 }> {
        self.add_internal::<{ C + A + 2 }>(0x3, services)
    }

    pub fn add_name<const A: usize>(self, name: &[u8; A]) -> AdvertisingData<{ C + A + 2 }> {
        self.add_internal::<{ C + A + 2 }>(0x9, name)
    }

    // Safety: This function should only be called by the get_advertising_data macro
    pub unsafe fn get_slice(&self) -> &[u8] {
        &self.data[..C]
    }
}

trait CompileTimeSize {
    const SIZE: usize;
}

impl<const C: usize> CompileTimeSize for AdvertisingData<C> {
    const SIZE: usize = C;
}

macro_rules! get_advertising_data {
    ($trait_object:expr) => {{
        // Assert that the advertising data does not exeed the maximum size.
        type Alias = impl CompileTimeSize;
        let _: &Alias = $trait_object;
        const _: () = assert!(
            Alias::SIZE <= MAXIMUM_ADVERTISE_LENGTH,
            "Advertising data is too big. Try shortening the keyboard name."
        );

        // Return advertising data.
        unsafe { $trait_object.get_slice() }
    }};
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
            current_len: 9,
            max_len: 9,
            write_perm: unsafe { core::mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(raw::BLE_GATTS_VLOC_STACK as u8),
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&config);
    let mut server = defmt::unwrap!(Server::new(sd));
    server.set_softdevice(sd);
    defmt::unwrap!(spawner.spawn(softdevice_task(sd)));

    let advertising_data = AdvertisingData::new();
    let advertising_data = advertising_data.add_flags(raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8);
    let advertising_data = advertising_data.add_services(&[0x09, 0x18]);
    let advertising_data = advertising_data.add_name(Used::DEVICE_NAME);
    let advertising_data = get_advertising_data!(&advertising_data);

    #[rustfmt::skip]
    let scan_data = &[
        0x03, 0x03, 0x09, 0x18,
    ];

    static BONDER: StaticCell<Bonder> = StaticCell::new();
    let bonder = BONDER.init(Bonder::default());

    /*if let Some(spi) = &mut pins.spi {
        spi.write(&[
            //green (0)
            0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011, 0b11110001, 0b11111000,
            // red (255)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
            // blue (255)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
        ])
        .await
        .unwrap();
    }*/

    let mut keyboard_state = KeyboardState::<Used>::new();

    loop {
        // Advertise
        let config = peripheral::Config::default();
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data: advertising_data,
            scan_data,
        };
        let connection = defmt::unwrap!(peripheral::advertise_pairable(sd, adv, &config, bonder).await);

        // Create future that will run as long as the connection is running
        let run_future = gatt_server::run(&connection, &server, |event| {
            defmt::debug!("Event: {:?}", event);
        });
        pin_mut!(run_future);

        loop {
            let timer_future = Timer::after(Duration::from_millis(1));
            pin_mut!(timer_future);

            match select(run_future, timer_future).await {
                Either::Left((result, _)) => {
                    if let Err(error) = result {
                        defmt::debug!("gatt_server run exited with error: {:?}", error);
                    }

                    break;
                }
                Either::Right((_, passed_run_future)) => {
                    // we want to write red 255, green 0, blue 255
                    // => 11111111 00000000 11111111
                    // => 110*8 100*8 110*8
                    // => 111111000*8 111000000*8 111111000*8

                    // TEMP
                    let mut key_state = 0;
                    let mut offset = 0;

                    for (column_index, column) in pins.columns.iter_mut().enumerate() {
                        column.set_high();

                        for (row_index, row) in pins.rows.iter().enumerate() {
                            let raw_state = row.is_high();
                            keyboard_state.keys[column_index][row_index].update(raw_state);

                            key_state |= (keyboard_state.keys[column_index][row_index].output_state as u64) << offset;
                            offset += 1;
                        }

                        column.set_low();
                    }

                    if key_state != keyboard_state.previous_key_state {
                        server.send_input_report::<Used>(&connection, key_state);
                        keyboard_state.previous_key_state = key_state;
                    }

                    /*if key_state != 0 {
                        spi.write(&[
                            //green (0)
                            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011,
                            0b10000001, 0b11000000, // red (255)
                            0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011,
                            0b11110001, 0b11111000, // blue (255)
                            0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011,
                            0b11110001, 0b11111000,
                        ])
                        .await
                        .unwrap();
                    } else {
                        spi.write(&[
                            //green (0)
                            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011,
                            0b10000001, 0b11000000, // red (255)
                            0b11100000, 0b01110000, 0b00111111, 0b00011111, 0b10001111, 0b11000111, 0b11100011,
                            0b11110001, 0b11111000, // blue (255)
                            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011,
                            0b10000001, 0b11000000,
                        ])
                        .await
                        .unwrap();
                    }*/

                    run_future = passed_run_future;
                }
            }
        }

        /*spi.write(&[
            //green (0)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
            // red (255)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
            // blue (255)
            0b11100000, 0b01110000, 0b00111000, 0b00011100, 0b00001110, 0b00000111, 0b00000011, 0b10000001, 0b11000000,
        ])
        .await
        .unwrap();*/
    }
}
