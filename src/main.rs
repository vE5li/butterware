#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr)]
#![feature(concat_idents)]
#![feature(iter_next_chunk)]
#![allow(incomplete_features)]

use defmt_rtt as _; // global logger
use embassy_executor::Spawner;
use embassy_nrf as _; // time driver
use embassy_nrf::config::{HfclkSource, LfclkSource};
use embassy_nrf::interrupt;
use embassy_time::driver::now;
use embassy_time::{Duration, Timer};
use futures::future::{select, Either};
use futures::pin_mut;
use keys::Mapping;
use nrf_softdevice::ble::{gatt_server, peripheral};
use nrf_softdevice::{raw, Softdevice};
use panic_probe as _;
use static_cell::StaticCell;

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
use crate::hardware::{ActiveLayer, KeyboardState};
use crate::interface::{Keyboard, Scannable};

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

    let sd = Softdevice::enable(&config);
    let mut server = defmt::unwrap!(Server::new(sd));
    server.set_softdevice(sd);
    defmt::unwrap!(spawner.spawn(softdevice_task(sd)));

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

    let mut keyboard_state = KeyboardState::<Used>::new();

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

    loop {
        // Advertise
        let config = peripheral::Config::default();
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data: ADVERTISING_DATA.get_slice(),
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

                            key_state |= (keyboard_state.keys[column_index][row_index].is_down() as u64) << offset;
                            offset += 1;
                        }

                        column.set_low();
                    }

                    let mut inject_mask = 0;

                    // Try to pop layers
                    while let Some(active_layer) = keyboard_state.active_layers.last() {
                        let key_index = active_layer.key_index;

                        match key_state.test_bit(key_index) {
                            true => break,
                            false => {
                                // Check if we want to execute the tap action for this layer (if
                                // present).
                                if matches!(active_layer.tap_timer, Some(time) if now() - time < Used::TAP_TIME) {
                                    inject_mask.set_bit(key_index);
                                }

                                keyboard_state.active_layers.pop();

                                // We lock all keys except the layer keys. This avoids
                                // cases where we leave a layer while holding a key and we
                                // send the key again but from the lower layer.
                                keyboard_state.lock_keys();

                                // Add layer key to the mask again (re-enable the key).
                                keyboard_state.state_mask.set_bit(key_index);

                                // For now we unset all non-layer keys so we don't get any key
                                // presses form the current layer.
                                key_state &= !keyboard_state.state_mask;
                            }
                        }
                    }

                    // Ignore all keys that are held as part of a layer.
                    key_state &= keyboard_state.state_mask;

                    if key_state | inject_mask != keyboard_state.previous_key_state {
                        // FIX: unclear what happens if we press multiple layer keys on the same
                        // event

                        let active_layer = Used::LAYER_LOOKUP[keyboard_state.current_layer_index()];

                        for key_index in 0..Used::COLUMNS * Used::ROWS {
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
                                if let Some(active_layer) = keyboard_state.active_layers.last_mut() {
                                    active_layer.tap_timer = None;
                                }

                                let new_active_layer = ActiveLayer {
                                    layer_index,
                                    key_index,
                                    tap_timer,
                                };

                                keyboard_state
                                    .active_layers
                                    .push(new_active_layer)
                                    .expect("Active layer limit reached");

                                // Remove the key from the state mask (disable the key). This
                                // helps cut down on expensive updates and also ensures that we
                                // don't get any modifier keys in send_input_report.
                                keyboard_state.state_mask.clear_bit(key_index);

                                // We lock all keys except the layer keys. This avoids
                                // cases where we enter a layer while holding a key and we
                                // send the key again but from the new layer.
                                keyboard_state.lock_keys();

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
                            if let Some(active_layer) = keyboard_state.active_layers.last_mut() {
                                active_layer.tap_timer = None;
                            }
                        }

                        // Inject key press from tap actions. Only a single bit should be set.
                        key_state |= inject_mask;

                        // Since we might have altered the key state we check again if it changed
                        // to avoid sending the same input report multiple times.
                        if key_state != keyboard_state.previous_key_state {
                            // We save the state after potentially injecting an additional key press, since
                            // that will cause the next scan to update again, releasing the key on the host.
                            keyboard_state.previous_key_state = key_state;

                            server.send_input_report::<Used>(&connection, keyboard_state.current_layer_index(), key_state);
                        }
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
