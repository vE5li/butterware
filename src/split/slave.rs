use core::convert::Infallible;

use futures::{pin_mut, future::{select, Either}};
use nrf_softdevice::{Softdevice, ble::{Address, central, Connection}};

use crate::{ble::{FlashServer, KeyStateServiceClient, FlashServerEvent, FlashServiceEvent}, hardware::{ScanPins, SlaveState}, led::AnimationType, interface::Keyboard};

use super::HalfDisconnected;

pub async fn do_slave<'a, K>(
    softdevice: &Softdevice,
    flash_server: &FlashServer,
    pins: &mut ScanPins<'a, { K::COLUMNS }, { K::ROWS }>,
    led_sender: embassy_sync::channel::Sender<'static, embassy_sync::blocking_mutex::raw::ThreadModeRawMutex, AnimationType, 3>,
    address: &Address,
) -> Result<Infallible, HalfDisconnected>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    let addresses = [address];
    let mut config = central::ConnectConfig::default();
    config.scan_config.whitelist = Some(&addresses);
    config.conn_params.min_conn_interval = 6;
    config.conn_params.max_conn_interval = 6;
    config.conn_params.conn_sup_timeout = 100; // 1 second timeout

    let mut keyboard_state = SlaveState::<K>::new();

    defmt::debug!("stating slave");

    let master_connection = defmt::unwrap!(central::connect(softdevice, &config).await);

    defmt::info!("connected to other half");

    //let animation = unsafe { flash::FLASH_SETTINGS.assume_init_ref()
    // }.settings.animation;
    let animation = AnimationType::Rainbow;
    led_sender.send(animation).await;

    slave_connection(&mut keyboard_state, pins, led_sender, master_connection, flash_server).await
}

async fn slave_connection<'a, K>(
    state: &mut SlaveState<K>,
    pins: &mut ScanPins<'a, { K::COLUMNS }, { K::ROWS }>,
    led_sender: embassy_sync::channel::Sender<'static, embassy_sync::blocking_mutex::raw::ThreadModeRawMutex, AnimationType, 3>,
    master_connection: Connection,
    flash_server: &FlashServer,
) -> Result<Infallible, HalfDisconnected>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    let key_state_client: KeyStateServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&master_connection).await);
    let flash_sender = crate::flash::FLASH_OPERATIONS.sender();

    loop {
        // Returns any time there is any change in the key state. This state is already
        // debounced.
        let scan_future = crate::hardware::do_scan(state, pins);
        let flash_future = nrf_softdevice::ble::gatt_server::run(&master_connection, flash_server, |event| match event {
            FlashServerEvent::FlashService(event) => match event {
                FlashServiceEvent::FlashOperationWrite(flash_operation) => {
                    defmt::debug!("Received flash operation {:?}", flash_operation);

                    if flash_sender.try_send(flash_operation).is_err() {
                        defmt::error!("Failed to send flash operation to the flash task");
                    }
                }
            },
        });

        pin_mut!(scan_future);
        pin_mut!(flash_future);

        match select(scan_future, flash_future).await {
            Either::Left((raw_state, _)) => {
                // Update the key state on the master.
                key_state_client.key_state_write(&raw_state).await.map_err(|_| HalfDisconnected)?;
            }
            Either::Right(..) => return Err(HalfDisconnected),
        }
    }
}
