use core::convert::Infallible;
use core::ops::ControlFlow;

use futures::future::{select, Either};
use futures::pin_mut;
use nrf_softdevice::ble::gatt_server::NotifyValueError;
use nrf_softdevice::ble::{gatt_server, peripheral, set_address, Connection};
use nrf_softdevice::Softdevice;

use super::HalfDisconnected;
use crate::battery::battery_level_receiver;
use crate::ble::{
    Bonder, CommunicationServer, CommunicationServerEvent, EventServiceClient, EventServiceEvent, FlashServiceClient, FlashServiceEvent,
    KeyStateServiceEvent, LightingServiceClient, LightingServiceEvent, Server,
};
use crate::flash::flash_sender;
use crate::hardware::{BitOperations, MasterState, ScanPins};
use crate::interface::{Keyboard, KeyboardExtension, Scannable};
#[cfg(feature = "lighting")]
use crate::led::lighting_sender;

pub async fn do_master(
    softdevice: &Softdevice,
    keyboard: &mut crate::Used,
    server: &Server,
    communication_server: &CommunicationServer,
    bonder: &'static Bonder,
    adv_data: &[u8],
    scan_data: &[u8],
    pins: &mut ScanPins<'_, { <crate::Used as Scannable>::COLUMNS }, { <crate::Used as Scannable>::ROWS }>,
) -> Result<Infallible, HalfDisconnected> {
    defmt::debug!("Stating master");

    keyboard.pre_sides_connected(true).await;

    // Connect to the other half
    let config = peripheral::Config::default();
    let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
    let slave_connection = defmt::unwrap!(peripheral::advertise_connectable(softdevice, adv, &config).await);

    // Get the flash client of the other side.
    let flash_client: FlashServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&slave_connection).await);
    let other_flash_operations = crate::flash::other_flash_receiver();

    // Get the lighting client of the other side.
    let lighting_client: LightingServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&slave_connection).await);
    let other_lighting_operations = crate::led::other_lighting_receiver();

    // Get the event client of the other side.
    let event_client: EventServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&slave_connection).await);
    let other_events = crate::split::event::other_event_receiver();

    defmt::info!("Connected to other half");

    keyboard.post_sides_connected(true).await;

    // Set unified address.
    set_address(softdevice, &<crate::Used as Keyboard>::ADDRESS);

    let inner_future = async {
        let mut keyboard_state = MasterState::new();

        loop {
            // Advertise
            let config = peripheral::Config::default();
            let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
            let advertise_future = peripheral::advertise_pairable(softdevice, adv, &config, bonder);
            pin_mut!(advertise_future);

            let host_connection = loop {
                let inner_future = master_scan(keyboard, &mut keyboard_state, pins, communication_server, &slave_connection);

                pin_mut!(inner_future);

                match select(advertise_future, inner_future).await {
                    Either::Left((advertise_result, _)) => {
                        break defmt::unwrap!(advertise_result);
                    }
                    Either::Right((result, passed_advertise_future)) => {
                        // We just want to make sure that the slave did not disconnect, so we discard
                        // all other information.
                        if result.is_err() {
                            return HalfDisconnected;
                        }

                        advertise_future = passed_advertise_future;
                        continue;
                    }
                }
            };

            defmt::warn!("Connected to host");

            let host_future = gatt_server::run(&host_connection, server, |_| {});
            let state_future = update_master_state(
                keyboard,
                &mut keyboard_state,
                pins,
                server,
                communication_server,
                &slave_connection,
                &host_connection,
            );

            pin_mut!(host_future);
            pin_mut!(state_future);

            match select(host_future, state_future).await {
                // Keyboard disconnected from host, so just continue.
                Either::Left(..) => {}
                // Only returns if the halves disconnected, so we break.
                Either::Right(..) => break HalfDisconnected,
            }
        }
    };

    let client_future = super::common::run_clients(
        &flash_client,
        &other_flash_operations,
        &lighting_client,
        &other_lighting_operations,
        &event_client,
        &other_events,
    );

    pin_mut!(inner_future);
    pin_mut!(client_future);

    // FIX: match ?
    let _ = select(inner_future, client_future).await;

    Err(HalfDisconnected)
}

async fn master_scan(
    keyboard: &mut crate::Used,
    state: &mut MasterState,
    pins: &mut ScanPins<'_, { <crate::Used as Scannable>::COLUMNS }, { <crate::Used as Scannable>::ROWS }>,
    communication_server: &CommunicationServer,
    slave_connection: &Connection,
) -> Result<(usize, u64, u64), HalfDisconnected> {
    let flash_sender = flash_sender();
    #[cfg(feature = "lighting")]
    let lighting_sender = lighting_sender();

    loop {
        let master_raw_state = state.master_raw_state;
        let slave_raw_state = state.slave_raw_state;

        let (key_state, slave_raw_state) = {
            // Create futures.
            let scan_future = crate::hardware::do_scan(state, pins);
            let slave_future = gatt_server::run_until(slave_connection, communication_server, |event| match event {
                CommunicationServerEvent::KeyStateService(event) => match event {
                    KeyStateServiceEvent::KeyStateWrite(key_state) => ControlFlow::Break(key_state),
                },
                CommunicationServerEvent::FlashService(event) => match event {
                    FlashServiceEvent::FlashOperationWrite(flash_operation) => {
                        defmt::debug!("Received flash operation {:?}", flash_operation);

                        if flash_sender.try_send(flash_operation).is_err() {
                            defmt::error!("Failed to send flash operation to the flash task");
                        }

                        ControlFlow::Continue(())
                    }
                },
                CommunicationServerEvent::EventService(event) => match event {
                    EventServiceEvent::EventWrite(event) => {
                        defmt::debug!("Received event {:?}", event);

                        //keyboard.event(event).await;
                        ControlFlow::Continue(())
                    }
                },
                #[cfg(feature = "lighting")]
                CommunicationServerEvent::LightingService(event) => match event {
                    LightingServiceEvent::LightingOperationWrite(lighting_operation) => {
                        defmt::debug!("Received lighting operation {:?}", lighting_operation);

                        if lighting_sender.try_send(lighting_operation).is_err() {
                            defmt::error!("Failed to send lighting operation to the lighting task");
                        }

                        ControlFlow::Continue(())
                    }
                },
            });

            pin_mut!(scan_future);
            pin_mut!(slave_future);

            match select(scan_future, slave_future).await {
                // Master side state changed.
                Either::Left((key_state, _)) => {
                    #[cfg(feature = "left")]
                    let combined_state = slave_raw_state | (key_state << <crate::Used as KeyboardExtension>::KEYS_PER_SIDE);

                    #[cfg(feature = "right")]
                    let combined_state = (slave_raw_state << <crate::Used as KeyboardExtension>::KEYS_PER_SIDE) | key_state;

                    (combined_state, slave_raw_state)
                }
                // Slave side state changed.
                Either::Right((key_state, _)) => {
                    let key_state = key_state.map_err(|_| HalfDisconnected)?;

                    #[cfg(feature = "left")]
                    let combined_state = (master_raw_state << <crate::Used as KeyboardExtension>::KEYS_PER_SIDE) | key_state;

                    #[cfg(feature = "right")]
                    let combined_state = master_raw_state | (key_state << <crate::Used as KeyboardExtension>::KEYS_PER_SIDE);

                    (combined_state, key_state)
                }
            }
        };

        // We do this update down here because we cannot mutably access the state inside
        // of the scope above.
        state.slave_raw_state = slave_raw_state;

        if let Some(output_state) = state.apply(keyboard, key_state).await {
            return Ok(output_state);
        }
    }
}

async fn update_master_state(
    keyboard: &mut crate::Used,
    state: &mut MasterState,
    pins: &mut ScanPins<'_, { <crate::Used as Scannable>::COLUMNS }, { <crate::Used as Scannable>::ROWS }>,
    server: &Server,
    communication_server: &CommunicationServer,
    slave_connection: &Connection,
    host_connection: &Connection,
) -> Result<(), HalfDisconnected> {
    let battery_level_receiver = battery_level_receiver();

    loop {
        let scan_future = master_scan(keyboard, state, pins, communication_server, slave_connection);
        let battery_level_future = battery_level_receiver.recv();

        pin_mut!(scan_future);
        pin_mut!(battery_level_future);

        match select(scan_future, battery_level_future).await {
            Either::Left((result, _)) => {
                let (active_layer, key_state, injected_keys) = result?;

                // If there are any, send the input once with the injected keys.
                if injected_keys != 0 {
                    //let input_report = InputReport::new(active_layer, key_state |
                    // injected_keys); defmt::unwrap!(server.hid_service.
                    // input_report_notify(&host_connection, &input_report));
                    send_input_report(server, &host_connection, active_layer, key_state | injected_keys);
                }

                //let input_report = InputReport::new(active_layer, key_state);
                //defmt::unwrap!(server.hid_service.input_report_notify(&host_connection,
                // &input_report));
                send_input_report(server, &host_connection, active_layer, key_state);
            }
            Either::Right((battery_level, _)) => {
                match server.battery_service.battery_level_notify(host_connection, &battery_level.0) {
                    Ok(..) => {}
                    Err(NotifyValueError::Disconnected) => return Err(HalfDisconnected),
                    Err(error) => defmt::warn!("Error when sending battery level: {:?}", error),
                };
            }
        }
    }
}

pub fn send_input_report(server: &Server, connection: &Connection, active_layer: usize, key_state: u64) {
    const SCAN_CODE_POSITION: usize = 2;
    const REPORT_SIZE: usize = 8;

    let mut input_report = [0; REPORT_SIZE];
    let mut offset = SCAN_CODE_POSITION;

    // temporary assert to avoid bugs while implementing.
    assert!(<crate::Used as KeyboardExtension>::KEYS_TOTAL <= 64);

    for index in 0..<crate::Used as KeyboardExtension>::KEYS_TOTAL {
        if key_state.test_bit(index) {
            if offset == REPORT_SIZE {
                input_report[SCAN_CODE_POSITION..REPORT_SIZE].fill(crate::keys::ERR_OVF.keycode());
                break;
            }

            let key = <crate::Used as Keyboard>::LAYER_LOOKUP[active_layer][index].keycode();
            input_report[offset] = key;
            offset += 1;
        }
    }

    defmt::info!("Sending input report with value {:?}", input_report);

    defmt::unwrap!(server.hid_service.input_report_notify(connection, &input_report));
}
