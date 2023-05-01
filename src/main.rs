#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr)]
#![feature(concat_idents)]
#![feature(iter_next_chunk)]
#![allow(incomplete_features)]

use embassy_executor::Spawner;
use embassy_nrf as _; // time driver
use embassy_nrf::config::{HfclkSource, LfclkSource};
use embassy_nrf::interrupt;
use nrf_softdevice::ble::{set_address, Address};
use nrf_softdevice::{raw, Flash, Softdevice};
use procedural::{alias_used_keyboard, import_keyboards};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod ble;
mod flash;
mod future;
mod hardware;
mod split;
#[allow(unused)]
mod keys;
mod led;
#[macro_use]
mod interface;

// Import every .rs file in the specified directory (relative to the src folder)
// into a module name keyboards.
import_keyboards!("../keyboards");

use ble::Server;

use crate::ble::{AdvertisingData, Bonder, KEYBOARD_ICON};
use crate::interface::Keyboard;
use crate::led::AnimationType;

#[cfg(all(feature = "left", feature = "right"))]
compile_error!("Only one side can be built for at a time. Try disabling either the left or right feature.");

#[cfg(not(any(feature = "left", feature = "right")))]
compile_error!("No side to compile for was selected. Try enabling the left or right feature.");

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    alias_used_keyboard!(as Used);

    // First we get the peripherals access crate.
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = interrupt::Priority::P2;
    config.time_interrupt_priority = interrupt::Priority::P2;
    config.hfclk_source = HfclkSource::ExternalXtal;
    config.lfclk_source = LfclkSource::ExternalXtal;
    let peripherals = embassy_nrf::init(config);

    let mut meboard = Used::new();
    let (mut pins, spis) = meboard.init_peripherals(peripherals).to_pins();

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
    let key_state_server = defmt::unwrap!(ble::KeyStateServer::new(softdevice));
    let flash_server = defmt::unwrap!(ble::FlashServer::new(softdevice));
    #[cfg(feature = "left")]
    let master_server = defmt::unwrap!(ble::MasterServer::new(softdevice));
    server.set_softdevice(softdevice);
    defmt::unwrap!(spawner.spawn(softdevice_task(softdevice)));

    // Flash task
    let flash = Flash::take(softdevice);
    defmt::unwrap!(spawner.spawn(flash::flash_task(flash)));

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
    let bonder = BONDER.init(Bonder::new());

    static LED_CHANNEL: embassy_sync::channel::Channel<embassy_sync::blocking_mutex::raw::ThreadModeRawMutex, AnimationType, 3> =
        embassy_sync::channel::Channel::new();

    // Led task
    defmt::unwrap!(spawner.spawn(led::led_task(spis, LED_CHANNEL.receiver())));

    loop {
        // Set a well-defined address that the other half can connect to.
        #[cfg(feature = "left")]
        set_address(softdevice, &Used::LEFT_ADDRESS);
        #[cfg(feature = "right")]
        set_address(softdevice, &Used::RIGHT_ADDRESS);

        let led_sender = LED_CHANNEL.sender();

        led_sender.send(AnimationType::Disconnected).await;

        // Both sides will connect, initially with the left side as the server and the
        // right as peripheral. Afterwards the will randomly determine which side is the
        // master and drop the connection again.
        #[cfg(feature = "left")]
        let is_master = split::advertise_determine_master(softdevice, &master_server, ADVERTISING_DATA.get_slice(), scan_data).await;
        #[cfg(feature = "right")]
        let is_master = split::connect_determine_master(softdevice, &Used::LEFT_ADDRESS).await;

        led_sender.send(AnimationType::IndicateMaster { is_master }).await;

        defmt::debug!("is master: {}", is_master);

        match is_master {
            true => {
                split::do_master::<Used>(
                    softdevice,
                    &server,
                    &key_state_server,
                    bonder,
                    ADVERTISING_DATA.get_slice(),
                    scan_data,
                    &mut pins,
                    led_sender,
                )
                .await
            }
            false => {
                #[cfg(feature = "left")]
                const MASTER_ADDRESS: Address = Used::RIGHT_ADDRESS;
                #[cfg(feature = "right")]
                const MASTER_ADDRESS: Address = Used::LEFT_ADDRESS;

                split::do_slave::<Used>(softdevice, &flash_server, &mut pins, led_sender, &MASTER_ADDRESS).await
            }
        };

        defmt::error!("halves disconnected");

        #[cfg(not(feature = "auto-reset"))]
        run_disconnected_animation().await;
    }
}
