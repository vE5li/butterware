use embassy_nrf::gpio::Pin;
use embassy_nrf::{interrupt, Peripherals};

use crate::hardware::{ScanPinConfig, SpiConfig};
use crate::interface::{Keyboard, Scannable};
use crate::keys::*;

pub struct Meboard;

register_layers!(Meboard, MeboardLayers, [BASE, SPECIAL, TEST]);

impl Meboard {
    #[rustfmt::skip]
    const BASE: [Mapping; <Meboard as Scannable>::COLUMNS * <Meboard as Scannable>::ROWS] = [
        Q, W, F, P, B,
        A, R, S, T, G,
        Z, X, C, D, Mapping::tap_layer(MeboardLayers::TEST, V),
        NONE, NONE, NONE, NONE, Self::SPE_SPC,
    ];

    #[rustfmt::skip]
    const SPECIAL: [Mapping; <Meboard as Scannable>::COLUMNS * <Meboard as Scannable>::ROWS] = [
        N1, N2, N3, N4, N5,
        A, R, S, T, G,
        Z, X, C, D, V,
        NONE, NONE, NONE, NONE, NONE,
    ];

    #[rustfmt::skip]
    const TEST: [Mapping; <Meboard as Scannable>::COLUMNS * <Meboard as Scannable>::ROWS] = [
        Q, W, F, P, B,
        A, R, S, T, G,
        Mapping::tap_layer(MeboardLayers::SPECIAL, Z), X, C, D, V,
        NONE, NONE, NONE, NONE, Self::SPE_SPC,
    ];

    const SPE_SPC: Mapping = Mapping::tap_layer(MeboardLayers::SPECIAL, SPACE);
}

impl Scannable for Meboard {
    const COLUMNS: usize = 5;
    const NAME_LENGTH: usize = 7;
    const ROWS: usize = 4;
}

impl Keyboard for Meboard {
    const DEVICE_NAME: &'static [u8; Self::NAME_LENGTH] = b"Meboard";

    const LAYER_LOOKUP: &'static [&'static [Mapping; Self::COLUMNS * Self::ROWS]] = MeboardLayers::LAYER_LOOKUP;

    #[rustfmt::skip]
    const MATRIX: [usize; Self::COLUMNS * Self::ROWS] = [
        4, 9, 14, 19,
        3, 8, 13, 18,
        2, 7, 12, 17,
        1, 6, 11, 16,
        0, 5, 10, 15
    ];

    fn new() -> Self {
        Self
    }

    fn init_peripherals(&mut self, peripherals: Peripherals) -> ScanPinConfig<{ Self::COLUMNS }, { Self::ROWS }> {
        use embassy_nrf::interrupt::InterruptExt;

        // Enable power on the 3V rail.
        let power_pin = peripherals.P0_13.degrade();

        // Set up SPI
        let interrupt_3 = interrupt::take!(SPIM3);
        interrupt_3.set_priority(interrupt::Priority::P2);

        // Set up SPI
        let interrupt_2 = interrupt::take!(SPIM2_SPIS2_SPI2);
        interrupt_2.set_priority(interrupt::Priority::P2);

        // Set up SPI
        let interrupt_1 = interrupt::take!(SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1);
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
