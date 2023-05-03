use core::convert::Infallible;

use futures::future::{select, Either};
use futures::pin_mut;
use nrf_softdevice::ble::{central, Address, Connection};
use nrf_softdevice::Softdevice;

use super::HalfDisconnected;
use crate::ble::{FlashServer, FlashServerEvent, FlashServiceEvent, KeyStateServiceClient};
use crate::hardware::{ScanPins, SlaveState};
use crate::interface::Keyboard;
use crate::led::LedSender;

pub async fn do_slave<'a, K>(
    softdevice: &Softdevice,
    flash_server: &FlashServer,
    pins: &mut ScanPins<'a, { K::COLUMNS }, { K::ROWS }>,
    address: &Address,
    #[cfg(feature = "lighting")] led_sender: &LedSender,
) -> Result<Infallible, HalfDisconnected>
where
    K: Keyboard,
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

    #[cfg(feature = "lighting")]
    let animation = unsafe { crate::flash::FLASH_SETTINGS.assume_init_ref() }.settings.animation;

    #[cfg(feature = "lighting")]
    led_sender.send(animation).await;

    slave_connection(
        &mut keyboard_state,
        pins,
        master_connection,
        flash_server,
    )
    .await
}

async fn slave_connection<'a, K>(
    state: &mut SlaveState<K>,
    pins: &mut ScanPins<'a, { K::COLUMNS }, { K::ROWS }>,
    master_connection: Connection,
    flash_server: &FlashServer,
) -> Result<Infallible, HalfDisconnected>
where
    K: Keyboard,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    let key_state_client: KeyStateServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&master_connection).await);
    let flash_sender = crate::flash::flash_sender();

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
