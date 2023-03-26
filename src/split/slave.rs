use core::convert::Infallible;

use futures::future::select;
use futures::{pin_mut, FutureExt};
use nrf_softdevice::ble::{central, Address, Connection};
use nrf_softdevice::Softdevice;

use super::event::event_sender;
use super::{event_receiver, HalfDisconnected};
use crate::ble::{
    CommunicationServer, CommunicationServerEvent, EventServiceClient, EventServiceEvent, FlashServiceClient, FlashServiceEvent,
    KeyStateServiceClient, KeyStateServiceEvent, PowerServiceClient, PowerServiceEvent,
};
#[cfg(feature = "lighting")]
use crate::ble::{LightingServiceClient, LightingServiceEvent};
use crate::flash::flash_sender;
use crate::hardware::{MatrixPins, SlaveState};
use crate::interface::{Keyboard, Scannable};
#[cfg(feature = "lighting")]
use crate::led::lighting_sender;
use crate::power::power_sender;

pub async fn do_slave(
    softdevice: &Softdevice,
    keyboard: &mut crate::Used,
    communication_server: &CommunicationServer,
    matrix_pins: &mut MatrixPins<'_, { <crate::Used as Scannable>::COLUMNS }, { <crate::Used as Scannable>::ROWS }>,
    address: &Address,
) -> Result<Infallible, HalfDisconnected> {
    defmt::debug!("Stating slave");

    keyboard.pre_sides_connected(false).await;

    // Connect to the other half
    let addresses = [address];
    let mut config = central::ConnectConfig::default();
    config.scan_config.whitelist = Some(&addresses);
    config.conn_params.min_conn_interval = 6;
    config.conn_params.max_conn_interval = 6;
    config.conn_params.conn_sup_timeout = 100; // 1 second timeout

    let master_connection = defmt::unwrap!(central::connect(softdevice, &config).await);

    // Get the flash client of the other side.
    let flash_client: FlashServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&master_connection).await);
    let other_flash_operations = crate::flash::other_flash_receiver();

    // Get the power client of the other side.
    let power_client: PowerServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&master_connection).await);
    let other_power_operations = crate::power::other_power_receiver();

    // Get the led client of the other side.
    #[cfg(feature = "lighting")]
    let lighting_client: LightingServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&master_connection).await);
    #[cfg(feature = "lighting")]
    let other_lighting_operations = crate::led::other_lighting_receiver();

    // Get the event client of the other side.
    let event_client: EventServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&master_connection).await);
    let other_events = crate::split::event::other_event_receiver();

    //
    let key_state_client: KeyStateServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&master_connection).await);

    defmt::info!("Connected to other half");

    keyboard.post_sides_connected(false).await;

    let mut keyboard_state = SlaveState::new();

    let connection_future = slave_connection(
        keyboard,
        &mut keyboard_state,
        matrix_pins,
        &master_connection,
        communication_server,
        &key_state_client,
    );
    let client_future = super::common::run_clients(
        &flash_client,
        &other_flash_operations,
        &power_client,
        &other_power_operations,
        #[cfg(feature = "lighting")]
        &lighting_client,
        #[cfg(feature = "lighting")]
        &other_lighting_operations,
        &event_client,
        &other_events,
    );

    pin_mut!(connection_future);
    pin_mut!(client_future);

    // FIX: match ?
    let _ = select(connection_future, client_future).await;

    Err(HalfDisconnected)
}

async fn slave_connection(
    keyboard: &mut crate::Used,
    state: &mut SlaveState,
    matrix_pins: &mut MatrixPins<'_, { <crate::Used as Scannable>::COLUMNS }, { <crate::Used as Scannable>::ROWS }>,
    master_connection: &Connection,
    communication_server: &CommunicationServer,
    key_state_client: &KeyStateServiceClient,
) -> Result<Infallible, HalfDisconnected> {
    let event_sender = event_sender();
    let flash_sender = flash_sender();
    let power_sender = power_sender();
    #[cfg(feature = "lighting")]
    let lighting_sender = lighting_sender();

    let event_receiver = event_receiver();

    loop {
        // Returns any time there is any change in the key state. This state is already
        // debounced.
        let scan_future = crate::hardware::do_scan(state, matrix_pins).fuse();
        let event_future = event_receiver.recv().fuse();
        let connection_future = nrf_softdevice::ble::gatt_server::run(&master_connection, communication_server, |event| match event {
            CommunicationServerEvent::KeyStateService(event) => match event {
                KeyStateServiceEvent::KeyStateWrite(..) => defmt::warn!("Unexpected write to the key state service"),
            },
            CommunicationServerEvent::FlashService(event) => match event {
                FlashServiceEvent::FlashOperationWrite(flash_operation) => {
                    defmt::debug!("Received flash operation {:?}", flash_operation);

                    if flash_sender.try_send(flash_operation).is_err() {
                        defmt::error!("Failed to send flash operation to the flash task");
                    }
                }
            },
            CommunicationServerEvent::PowerService(event) => match event {
                PowerServiceEvent::PowerOperationWrite(power_operation) => {
                    defmt::debug!("Received power operation {:?}", power_operation);

                    if power_sender.try_send(power_operation).is_err() {
                        defmt::error!("Failed to send power operation to the power task");
                    }
                }
            },
            CommunicationServerEvent::EventService(event) => match event {
                EventServiceEvent::EventWrite(event) => {
                    defmt::debug!("Received event {:?}", event);

                    if event_sender.try_send(event).is_err() {
                        defmt::error!("Failed to send event");
                    }
                }
            },
            #[cfg(feature = "lighting")]
            CommunicationServerEvent::LightingService(event) => match event {
                LightingServiceEvent::LightingOperationWrite(lighting_operation) => {
                    defmt::debug!("Received lighting operation {:?}", lighting_operation);

                    if lighting_sender.try_send(lighting_operation).is_err() {
                        defmt::error!("Failed to send lighting operation to the lighting task");
                    }
                }
            },
        })
        .fuse();

        pin_mut!(scan_future);
        pin_mut!(event_future);
        pin_mut!(connection_future);

        futures::select_biased! {
            raw_state = scan_future => {
                // Update the key state on the master.
                key_state_client.key_state_write(&raw_state).await.map_err(|_| HalfDisconnected)?;
            },
            event = event_future => keyboard.event(event).await,
            _ = connection_future => return Err(HalfDisconnected),
        }
    }
}
