use embassy_cortex_m::interrupt::Interrupt;
use embassy_nrf::gpio::Pin;
use embassy_nrf::{interrupt, Peripherals};

use crate::flash::{get_settings, FlashToken, FlashTransaction};
use crate::hardware::{ScanPinConfig, SpiConfig};
use crate::interface::{Keyboard, Scannable};
use crate::keys::*;
use crate::led::{Animation, Led, Speed};

pub struct Butterboard {
    current_animation: usize,
}

register_layers!(Butterboard, ButterboardLayers, [BASE, SPECIAL, TEST]);

#[rustfmt::skip]
macro_rules! new_layer {
    (
        $K0:expr,  $K1:expr,  $K2:expr,  $K3:expr,  $K4:expr,  $K5:expr,  $K6:expr,  $K7:expr,  $K8:expr,  $K9:expr,
        $K10:expr, $K11:expr, $K12:expr, $K13:expr, $K14:expr, $K15:expr, $K16:expr, $K17:expr, $K18:expr, $K19:expr,
        $K20:expr, $K21:expr, $K22:expr, $K23:expr, $K24:expr, $K25:expr, $K26:expr, $K27:expr, $K28:expr, $K29:expr,
        $K30:expr, $K31:expr, $K32:expr, $K33:expr, $K34:expr, $K35:expr, $K36:expr, $K37:expr, $K38:expr, $K39:expr,
    ) => {
        [
            $K9, $K19, $K29, $K39,
            $K8, $K18, $K28, $K38,
            $K7, $K17, $K27, $K37,
            $K6, $K16, $K26, $K36,
            $K5, $K15, $K25, $K35,
            $K4, $K14, $K24, $K34,
            $K3, $K13, $K23, $K33,
            $K2, $K12, $K22, $K32,
            $K1, $K11, $K21, $K31,
            $K0, $K10, $K20, $K30,
        ]
    };
}

impl Butterboard {
    const ANIMATIONS: &[Animation] = &[
        Animation::Rainbow {
            hue: 0.0,
            speed: Speed(15.0),
        },
        Animation::Rainbow {
            hue: 0.0,
            speed: Speed(30.0),
        },
        Animation::Rainbow {
            hue: 0.0,
            speed: Speed(60.0),
        },
        Animation::Pulsate {
            color: Led::rgb(1.0, 0.0, 0.0),
            speed: Speed(4.0),
            offset: 0.0,
        },
        Animation::Pulsate {
            color: Led::rgb(0.0, 1.0, 0.0),
            speed: Speed(4.0),
            offset: 0.0,
        },
        Animation::Pulsate {
            color: Led::rgb(0.0, 0.0, 1.0),
            speed: Speed(4.0),
            offset: 0.0,
        },
    ];
    #[rustfmt::skip]
    const BASE: [Mapping; <Butterboard as Scannable>::COLUMNS * <Butterboard as Scannable>::ROWS * 2] = new_layer![
        Mapping::Special(SpecialAction::SwitchAnimation { animation: Self::ANIMATIONS[0] }), Mapping::Special(SpecialAction::SwitchAnimation { animation: Self::ANIMATIONS[1] }), Mapping::Special(SpecialAction::SwitchAnimation { animation: Self::ANIMATIONS[2] }), Mapping::Special(SpecialAction::SwitchAnimation { animation: Self::ANIMATIONS[3] }), Mapping::Special(SpecialAction::SwitchAnimation { animation: Self::ANIMATIONS[4] }), J, L, U, Y, Y,
        A, R, S, T, G, M, N, E, I, O,
        Z, X, C, D, Mapping::tap_layer(ButterboardLayers::TEST, V), K, H, H, H, Mapping::tap_layer(ButterboardLayers::TEST, H),
        NONE, NONE, NONE, NONE, Self::SPE_SPC, NONE, NONE, NONE, NONE, NONE,
    ];
    #[rustfmt::skip]
    const SPECIAL: [Mapping; <Butterboard as Scannable>::COLUMNS * <Butterboard as Scannable>::ROWS * 2] = new_layer![
        N1, N2, N3, N4, N5, N6, N7, N8, N9, N0,
        A, R, S, T, G, M, N, E, I, O,
        Z, X, C, D, V, K, H, H, H, H,
        NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE,
    ];
    const SPE_SPC: Mapping = Mapping::tap_layer(ButterboardLayers::SPECIAL, SPACE);
    #[rustfmt::skip]
    const TEST: [Mapping; <Butterboard as Scannable>::COLUMNS * <Butterboard as Scannable>::ROWS * 2] = new_layer![
        Q, W, F, P, B, J, L, U, Y, Y,
        A, R, S, T, G, M, N, E, I, O,
        Mapping::tap_layer(ButterboardLayers::SPECIAL, Z), X, C, D, V, K, H, H, H, H,
        NONE, NONE, NONE, NONE, Self::SPE_SPC, NONE, NONE, NONE, NONE, NONE,
    ];

    #[allow(unused)]
    async fn next_animation(&mut self) {
        // Go to next animation.
        self.current_animation = (self.current_animation + 1) % Self::ANIMATIONS.len();

        let animation = Self::ANIMATIONS[self.current_animation];

        FlashTransaction::new()
            // Update the lighting on both sides and in the firmware flash.
            .switch_animation(animation)
            // Save custom data to our board flash.
            .store_board_flash(self.current_animation)
            // Apply operations
            .apply()
            .await;
    }
}

impl Scannable for Butterboard {
    const COLUMNS: usize = 5;
    const ROWS: usize = 4;
}

impl Keyboard for Butterboard {
    type BoardFlash = usize;

    const DEVICE_NAME: &'static [u8] = b"Butterboard";
    const LAYER_LOOKUP: &'static [&'static [Mapping; Self::COLUMNS * Self::ROWS * 2]] = ButterboardLayers::LAYER_LOOKUP;

    fn new(flash_token: FlashToken) -> Self {
        // Get the flash settings and extract the custom data stored for this board.
        let current_animation = get_settings(flash_token).board_flash;

        Self { current_animation }
    }

    /*async fn callback(&mut self, callback: Callback) {
        match callback {
            NextAnimation => self.next_animation().await,
        }
    }*/

    async fn initialize_peripherals(&mut self, peripherals: Peripherals) -> ScanPinConfig<{ Self::COLUMNS }, { Self::ROWS }> {
        use embassy_nrf::interrupt::InterruptExt;

        // Enable power on the 3V rail.
        let power_pin = peripherals.P0_13.degrade();

        // Set up SPI
        let interrupt_3 = unsafe { interrupt::SPIM3::steal() };
        interrupt_3.set_priority(interrupt::Priority::P2);

        // Set up SPI
        let interrupt_2 = unsafe { interrupt::SPIM2_SPIS2_SPI2::steal() };
        interrupt_2.set_priority(interrupt::Priority::P2);

        // Set up SPI
        let interrupt_1 = unsafe { interrupt::SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1::steal() };
        interrupt_1.set_priority(interrupt::Priority::P2);

        ScanPinConfig {
            columns: [
                peripherals.P0_31.degrade(),
                peripherals.P0_29.degrade(),
                peripherals.P0_02.degrade(),
                peripherals.P1_15.degrade(),
                peripherals.P1_13.degrade(),
            ],
            rows: [
                peripherals.P0_22.degrade(),
                peripherals.P0_24.degrade(),
                peripherals.P1_00.degrade(),
                peripherals.P0_11.degrade(),
            ],
            power_pin: Some(power_pin),
            spi_config: Some(SpiConfig {
                interface: peripherals.SPI3,
                interrupt: interrupt_3,
                clock_pin: peripherals.P0_08.degrade(),
                mosi_pin: peripherals.P0_06.degrade(),
            }),
            spi_2_config: Some(crate::hardware::Spi2Config {
                interface: peripherals.SPI2,
                interrupt: interrupt_2,
                clock_pin: peripherals.P0_09.degrade(),
                mosi_pin: peripherals.P0_17.degrade(),
            }),
            spi_1_config: Some(crate::hardware::Spi1Config {
                interface: peripherals.TWISPI1,
                interrupt: interrupt_1,
                clock_pin: peripherals.P0_10.degrade(),
                mosi_pin: peripherals.P0_20.degrade(),
            }),
        }
    }
}
