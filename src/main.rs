#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr)]
#![feature(concat_idents)]
#![feature(iter_next_chunk)]
#![feature(async_fn_in_trait)]
#![feature(associated_type_defaults)]
#![feature(never_type)]
#![feature(adt_const_params)]
#![allow(incomplete_features)]

use embassy_executor::Spawner;
use embassy_nrf as _; // time driver
use embassy_nrf::config::{HfclkSource, LfclkSource};
use embassy_nrf::interrupt;
use nrf_softdevice::ble::{set_address, Address};
use nrf_softdevice::raw::ble_common_cfg_vs_uuid_t;
use nrf_softdevice::{raw, Flash, Softdevice};
use procedural::{alias_keyboard, import_keyboards};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

mod battery;
mod ble;
mod flash;
mod hardware;
#[allow(unused)]
mod keys;
#[cfg(feature = "lighting")]
mod led;
mod split;
#[macro_use]
mod interface;

// Import every *.rs file in the specified directory (relative to the src
// folder) into a module named keyboards.
import_keyboards!("../keyboards");

// Get the keyboard selected by the user from environment variables and alias it
// as `Used`.
alias_keyboard!(as Used);

use ble::Server;

use crate::ble::{AdvertisingData, Bonder, KEYBOARD_ICON};
use crate::interface::Keyboard;
#[cfg(feature = "lighting")]
use crate::led::set_animation;

#[cfg(all(feature = "left", feature = "right"))]
compile_error!("Only one side can be built for at a time. Try disabling either the left or right feature");

#[cfg(not(any(feature = "left", feature = "right")))]
compile_error!("No side to compile for was selected. Try enabling the left or right feature");

#[derive(Clone, Copy, Debug, defmt::Format, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    This,
    Other,
    Left,
    Right,
    Both,
}

impl Side {
    const fn includes_this(&self) -> bool {
        match self {
            Side::This => true,
            Side::Other => false,
            Side::Left => cfg!(feature = "left"),
            Side::Right => cfg!(feature = "right"),
            Side::Both => true,
        }
    }

    const fn includes_other(&self) -> bool {
        match self {
            Side::This => false,
            Side::Other => true,
            Side::Left => !cfg!(feature = "left"),
            Side::Right => !cfg!(feature = "right"),
            Side::Both => true,
        }
    }
}

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    // Peripherals
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = interrupt::Priority::P2;
    config.time_interrupt_priority = interrupt::Priority::P2;
    config.hfclk_source = HfclkSource::ExternalXtal;
    config.lfclk_source = LfclkSource::ExternalXtal;
    let peripherals = embassy_nrf::init(config);

    // Softdevice config
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
            periph_role_count: 4,
            central_role_count: 4,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: Used::DEVICE_NAME.as_ptr() as *mut _,
            current_len: Used::DEVICE_NAME.len() as u16,
            max_len: Used::DEVICE_NAME.len() as u16,
            write_perm: unsafe { core::mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(raw::BLE_GATTS_VLOC_STACK as u8),
        }),
        common_vs_uuid: Some(ble_common_cfg_vs_uuid_t { vs_uuid_count: 12 }),
        ..Default::default()
    };

    let softdevice = Softdevice::enable(&config);

    // Initialize the settings stored in flash.
    let mut flash = Flash::take(softdevice);
    let flash_token = flash::initialize_flash(&mut flash).await;

    // Register BLE services.
    let server = defmt::unwrap!(Server::new(softdevice));
    let communication_server = defmt::unwrap!(ble::CommunicationServer::new(softdevice));
    #[cfg(feature = "left")]
    let master_server = defmt::unwrap!(ble::MasterServer::new(softdevice));

    // Instanciate the keyboard.
    let mut keyboard = Used::new(flash_token);
    keyboard.pre_initialize().await;
    let (mut pins, leds) = keyboard.initialize_peripherals(peripherals).await.to_pins();
    keyboard.post_initialize().await;

    // Softdevice task
    defmt::unwrap!(spawner.spawn(softdevice_task(softdevice)));

    // Flash task
    defmt::unwrap!(spawner.spawn(flash::flash_task(flash, flash_token)));

    // Battery task
    defmt::unwrap!(spawner.spawn(battery::battery_task(&server)));

    // Led task
    #[cfg(feature = "lighting")]
    defmt::unwrap!(spawner.spawn(led::lighting_task(leds)));

    // Bluetooth setup
    const SCAN_DATA: &[u8] = &[0x03, 0x03, 0x09, 0x18];
    const ADVERTISING_DATA: AdvertisingData = AdvertisingData::new()
        .add_flags(raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8)
        .add_services(&[0x09, 0x18])
        .add_name(Used::DEVICE_NAME)
        .add_appearance(KEYBOARD_ICON);

    static BONDER: StaticCell<Bonder> = StaticCell::new();
    let bonder = BONDER.init(Bonder::new(flash_token));

    loop {
        // Set a well-defined address that the other half can connect to.
        #[cfg(feature = "left")]
        set_address(softdevice, &Used::LEFT_ADDRESS);
        #[cfg(feature = "right")]
        set_address(softdevice, &Used::RIGHT_ADDRESS);

        #[cfg(feature = "lighting")]
        set_animation(Side::This, Used::STATUS_LEDS, Used::SEARCH_ANIMATION).await;

        // Both sides will connect, initially with the left side as the server and the
        // right as peripheral. Once they are connected, they will generate a random
        // number to determine which will be the master and drop the
        // connection again.
        #[cfg(feature = "left")]
        let is_master = split::advertise_determine_master(softdevice, &master_server, ADVERTISING_DATA.get_slice(), SCAN_DATA).await;
        #[cfg(feature = "right")]
        let is_master = split::connect_determine_master(softdevice, &Used::LEFT_ADDRESS).await;

        #[cfg(feature = "lighting")]
        match is_master {
            true => set_animation(Side::This, Used::STATUS_LEDS, Used::MASTER_ANIMATION).await,
            false => set_animation(Side::This, Used::STATUS_LEDS, Used::SLAVE_ANIMATION).await,
        }

        defmt::debug!("Is master: {}", is_master);

        let _ = match is_master {
            true => {
                split::do_master(
                    softdevice,
                    &mut keyboard,
                    &server,
                    &communication_server,
                    bonder,
                    ADVERTISING_DATA.get_slice(),
                    SCAN_DATA,
                    &mut pins,
                )
                .await
            }
            false => {
                #[cfg(feature = "left")]
                const MASTER_ADDRESS: Address = Used::RIGHT_ADDRESS;
                #[cfg(feature = "right")]
                const MASTER_ADDRESS: Address = Used::LEFT_ADDRESS;

                split::do_slave(softdevice, &mut keyboard, &communication_server, &mut pins, &MASTER_ADDRESS).await
            }
        };

        defmt::error!("Halves disconnected");

        // TODO: reimplement this
        //#[cfg(all(feature = "lighting", not(feature = "auto-reset")))]
        //lighting_sender.send((Used::STATUS_LEDS, Animation::Disconnected)).await;

        #[cfg(not(feature = "auto-reset"))]
        futures::future::pending::<()>().await;
    }
}
