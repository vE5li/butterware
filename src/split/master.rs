use core::convert::Infallible;
use core::ops::ControlFlow;

use embassy_time::{Duration, Timer};
use futures::future::{select, Either};
use futures::pin_mut;
use nrf_softdevice::ble::{gatt_server, peripheral, set_address, Connection};
use nrf_softdevice::Softdevice;

use super::HalfDisconnected;
use crate::ble::{Bonder, FlashServiceClient, KeyStateServer, KeyStateServerEvent, KeyStateServiceEvent, Server};
use crate::hardware::{MasterState, ScanPins};
use crate::interface::{Keyboard, KeyboardExtension};
#[cfg(feature = "lighting")]
use crate::led::AnimationType;
use crate::led::LedSender;

pub async fn do_master<'a, K>(
    softdevice: &Softdevice,
    server: &Server<'a>,
    key_state_server: &KeyStateServer,
    bonder: &'static Bonder,
    adv_data: &[u8],
    scan_data: &[u8],
    pins: &mut ScanPins<'a, { K::COLUMNS }, { K::ROWS }>,
    #[cfg(feature = "lighting")] led_sender: &LedSender,
) -> Result<Infallible, HalfDisconnected>
where
    K: Keyboard,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    defmt::debug!("stating master");

    // Connect to the other half
    let config = peripheral::Config::default();
    let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
    let slave_connection = defmt::unwrap!(peripheral::advertise_connectable(softdevice, adv, &config).await);

    defmt::info!("connected to other half");

    //let animation = unsafe { flash::FLASH_SETTINGS.assume_init_ref()
    // }.settings.animation;
    #[cfg(feature = "lighting")]
    let animation = AnimationType::Rainbow;

    #[cfg(feature = "lighting")]
    led_sender.send(animation).await;

    // Set unified address.
    set_address(softdevice, &K::ADDRESS);

    let mut keyboard_state = MasterState::<K>::new();

    loop {
        // Advertise
        let config = peripheral::Config::default();
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected { adv_data, scan_data };
        let advertise_future = peripheral::advertise_pairable(softdevice, adv, &config, bonder);
        pin_mut!(advertise_future);

        let host_connection = loop {
            let connection_future = Timer::after(Duration::from_secs(1));
            pin_mut!(connection_future);

            match select(advertise_future, connection_future).await {
                Either::Left((advertise_result, _)) => {
                    break defmt::unwrap!(advertise_result);
                }
                Either::Right((_, passed_advertise_future)) => {
                    slave_connection.handle().ok_or(HalfDisconnected)?;
                    advertise_future = passed_advertise_future;
                    continue;
                }
            }
        };

        /*if let Some(spi) = &mut pins.spi_2 {
            strips.top_strip.set_uniform_color(Led::rgb(255, 128, 0));
            spi.write(&strips.top_strip.get_led_data()).await;
        }*/

        defmt::warn!("connected");

        // Run until the host disconnects.
        master_connection(
            &mut keyboard_state,
            pins,
            server,
            key_state_server,
            &slave_connection,
            &host_connection,
        )
        .await?;

        /*if let Some(spi) = &mut pins.spi_2 {
            strips.top_strip.set_uniform_color(Led::rgb(100, 100, 255));
            spi.write(&strips.top_strip.get_led_data()).await;
        }*/
    }
}

async fn master_connection<'a, K>(
    state: &mut MasterState<K>,
    pins: &mut ScanPins<'a, { K::COLUMNS }, { K::ROWS }>,
    server: &Server<'_>,
    key_state_server: &KeyStateServer,
    slave_connection: &Connection,
    host_connection: &Connection,
) -> Result<(), HalfDisconnected>
where
    K: Keyboard,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    let host_future = gatt_server::run(host_connection, server, |_| {});
    pin_mut!(host_future);

    let flash_operations = crate::flash::SLAVE_FLASH_OPERATIONS.receiver();
    let flash_client: FlashServiceClient = defmt::unwrap!(nrf_softdevice::ble::gatt_client::discover(&slave_connection).await);

    loop {
        let inner_future = async {
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
                            let combined_state = slave_raw_state | (key_state << K::KEY_COUNT);

                            #[cfg(feature = "right")]
                            let combined_state = (slave_raw_state << K::KEY_COUNT) | key_state;

                            (combined_state, slave_raw_state)
                        }
                        // Slave side state changed.
                        crate::future::Either3::Second(key_state) => {
                            let key_state = key_state.map_err(|_| HalfDisconnected)?;

                            #[cfg(feature = "left")]
                            let combined_state = (master_raw_state << K::KEY_COUNT) | key_state;

                            #[cfg(feature = "right")]
                            let combined_state = master_raw_state | (key_state << K::KEY_COUNT);

                            (combined_state, key_state)
                        }
                        crate::future::Either3::Third(..) => unreachable!(),
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

                // If there are any, send the input once with the injected keys.
                if injected_keys != 0 {
                    server.send_input_report::<K>(&host_connection, active_layer, key_state | injected_keys);
                }

                server.send_input_report::<K>(&host_connection, active_layer, key_state);

                host_future = passed_host_future;
            }
        }
    }
}
