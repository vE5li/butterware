use core::convert::Infallible;
use core::ops::ControlFlow;

use futures::future::{select, Either};
use futures::pin_mut;
use nrf_softdevice::ble::{gatt_server, peripheral, set_address, Connection};
use nrf_softdevice::Softdevice;

use super::HalfDisconnected;
use crate::ble::{Bonder, FlashServiceClient, KeyStateServer, KeyStateServerEvent, KeyStateServiceEvent, Server};
use crate::flash::{get_settings, FlashToken, SlaveFlashReceiver};
use crate::hardware::{MasterState, ScanPins, BitOperations};
use crate::interface::{Keyboard, KeyboardExtension, Scannable};
use crate::led::LedSender;

pub async fn do_master(
    softdevice: &Softdevice,
    keyboard: &mut crate::Used,
    server: &Server,
    key_state_server: &KeyStateServer,
    bonder: &'static Bonder,
    adv_data: &[u8],
    scan_data: &[u8],
    pins: &mut ScanPins<'_, { <crate::Used as Scannable>::COLUMNS }, { <crate::Used as Scannable>::ROWS }>,
    #[cfg(feature = "lighting")] flash_token: FlashToken,
    #[cfg(feature = "lighting")] led_sender: &LedSender,
) -> Result<Infallible, HalfDisconnected> {
    defmt::debug!("stating master");

    // Connect to the other half
    let config = peripheral::Config::default();
    let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
    let slave_connection = defmt::unwrap!(peripheral::advertise_connectable(softdevice, adv, &config).await);

    // Get the flash client of the other side.
    let flash_client: FlashServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&slave_connection).await);
    let flash_operations = crate::flash::slave_flash_receiver();

    defmt::info!("connected to other half");

    #[cfg(feature = "lighting")]
    let animation = get_settings(flash_token).animation;

    #[cfg(feature = "lighting")]
    led_sender.send(animation).await;

    // Set unified address.
    set_address(softdevice, &<crate::Used as Keyboard>::ADDRESS);

    let mut keyboard_state = MasterState::new();

    loop {
        // Advertise
        let config = peripheral::Config::default();
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
        let advertise_future = peripheral::advertise_pairable(softdevice, adv, &config, bonder);
        pin_mut!(advertise_future);

        let host_connection = loop {
            let inner_future = master_scan(
                keyboard,
                &mut keyboard_state,
                pins,
                key_state_server,
                &slave_connection,
                &flash_client,
                &flash_operations,
            );
            pin_mut!(inner_future);

            match select(advertise_future, inner_future).await {
                Either::Left((advertise_result, _)) => {
                    break defmt::unwrap!(advertise_result);
                }
                Either::Right((result, passed_advertise_future)) => {
                    // We just want to make sure that the slave did not disconnect, so we discard
                    // all other information.
                    let _ = result?;
                    advertise_future = passed_advertise_future;
                    continue;
                }
            }
        };

        defmt::warn!("connected");

        // Run until the host disconnects.
        master_connection(
            keyboard,
            &mut keyboard_state,
            pins,
            server,
            key_state_server,
            &slave_connection,
            &host_connection,
            &flash_client,
            &flash_operations,
        )
        .await?;
    }
}

async fn master_scan(
    keyboard: &mut crate::Used,
    state: &mut MasterState,
    pins: &mut ScanPins<'_, { <crate::Used as Scannable>::COLUMNS }, { <crate::Used as Scannable>::ROWS }>,
    key_state_server: &KeyStateServer,
    slave_connection: &Connection,
    flash_client: &FlashServiceClient,
    flash_operations: &SlaveFlashReceiver,
) -> Result<(usize, u64, u64), HalfDisconnected> {
    loop {
        let master_raw_state = state.master_raw_state;
        let slave_raw_state = state.slave_raw_state;

        let (key_state, slave_raw_state) = {
            // Create futures.
            let scan_future = crate::hardware::do_scan(state, pins);
            let slave_future = gatt_server::run_until(slave_connection, key_state_server, |event| match event {
                KeyStateServerEvent::KeyStateService(event) => match event {
                    KeyStateServiceEvent::KeyStateWrite(key_state) => ControlFlow::Break(key_state),
                },
            });
            let flash_future = async {
                loop {
                    let flash_operation = flash_operations.recv().await;

                    defmt::info!("Received flash operation for client");
                    defmt::info!("Setting flash operation for client to {:?}", flash_operation);

                    defmt::unwrap!(flash_client.flash_operation_write(&flash_operation).await);
                }
            };

            // Pin futures so we can call select on them.
            //pin_mut!(scan_future);
            //pin_mut!(slave_future);

            match crate::future::select3(scan_future, slave_future, flash_future).await {
                // Master side state changed.
                crate::future::Either3::First(key_state) => {
                    #[cfg(feature = "left")]
                    let combined_state = slave_raw_state | (key_state << <crate::Used as KeyboardExtension>::KEYS_PER_SIDE);

                    #[cfg(feature = "right")]
                    let combined_state = (slave_raw_state << <crate::Used as KeyboardExtension>::KEYS_PER_SIDE) | key_state;

                    (combined_state, slave_raw_state)
                }
                // Slave side state changed.
                crate::future::Either3::Second(key_state) => {
                    let key_state = key_state.map_err(|_| HalfDisconnected)?;

                    #[cfg(feature = "left")]
                    let combined_state = (master_raw_state << <crate::Used as KeyboardExtension>::KEYS_PER_SIDE) | key_state;

                    #[cfg(feature = "right")]
                    let combined_state = master_raw_state | (key_state << <crate::Used as KeyboardExtension>::KEYS_PER_SIDE);

                    (combined_state, key_state)
                }
                crate::future::Either3::Third(..) => unreachable!(),
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

async fn master_connection(
    keyboard: &mut crate::Used,
    state: &mut MasterState,
    pins: &mut ScanPins<'_, { <crate::Used as Scannable>::COLUMNS }, { <crate::Used as Scannable>::ROWS }>,
    server: &Server,
    key_state_server: &KeyStateServer,
    slave_connection: &Connection,
    host_connection: &Connection,
    flash_client: &FlashServiceClient,
    flash_operations: &SlaveFlashReceiver,
) -> Result<(), HalfDisconnected> {
    let host_future = gatt_server::run(host_connection, server, |_| {});
    pin_mut!(host_future);

    loop {
        let inner_future = master_scan(
            keyboard,
            state,
            pins,
            key_state_server,
            slave_connection,
            flash_client,
            flash_operations,
        );
        pin_mut!(inner_future);

        match select(host_future, inner_future).await {
            // Keyboard disconnected from host, so just return.
            Either::Left(..) => return Ok(()),
            // There is a change in the output state of the keyboard so we need to send a new input
            // report.
            Either::Right((result, passed_host_future)) => {
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

                host_future = passed_host_future;
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
