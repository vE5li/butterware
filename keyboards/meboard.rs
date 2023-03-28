use embassy_nrf::gpio::Pin;
use embassy_nrf::spim::Config;
use embassy_nrf::{interrupt, Peripherals};

use crate::keys::*;
use crate::{Keyboard, ScanPinConfig, Scannable};

pub struct Meboard;

register_layers!(Meboard, MeboardLayers, [BASE, SPECIAL]);

impl Meboard {
    const SPE_SPC: Mapping = Mapping::layer_or_key(MeboardLayers::SPECIAL, SPACE);

    #[rustfmt::skip]
    const BASE: [Mapping; <Meboard as Scannable>::COLUMNS * <Meboard as Scannable>::ROWS] = [
        Q, W, F, P, B,
        A, R, S, T, G,
        Z, X, C, D, V,
        NONE, NONE, NONE, NONE, Self::SPE_SPC,
    ];

    #[rustfmt::skip]
    const SPECIAL: [Mapping; <Meboard as Scannable>::COLUMNS * <Meboard as Scannable>::ROWS] = [
        N1, N2, N3, N4, N5,
        A, R, S, T, G,
        Z, X, C, D, V,
        NONE, NONE, NONE, NONE, NONE,
    ];
}

impl Scannable for Meboard {
    const COLUMNS: usize = 5;
    const ROWS: usize = 4;
    const NAME_LENGTH: usize = 7;
}

impl Keyboard for Meboard {
    const DEVICE_NAME: &'static [u8; Self::NAME_LENGTH] = b"Meboard";

    #[rustfmt::skip]
    const MATRIX: [usize; Self::COLUMNS * Self::ROWS] = [
        4, 9, 14, 19,
        3, 8, 13, 18,
        2, 7, 12, 17,
        1, 6, 11, 16,
        0, 5, 10, 15
    ];

    const LAYER_LOOKUP: &'static [&'static [Mapping; Self::COLUMNS * Self::ROWS]] =
        MeboardLayers::LAYER_LOOKUP;

    fn new() -> Self {
        Self
    }

    fn init_peripherals(
        &mut self,
        peripherals: Peripherals,
    ) -> ScanPinConfig<{ Self::COLUMNS }, { Self::ROWS }> {
        use embassy_nrf::interrupt::InterruptExt;

        // Enable power on the 3V rail.
        let power_pin = peripherals.P0_13.degrade();

        // Set up SPI
        let interrupt = interrupt::take!(SPIM3);
        interrupt.set_priority(interrupt::Priority::P2);

        let mut config = Config::default();
        config.frequency = embassy_nrf::spim::Frequency::M8;

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
            spi_config: Some(crate::SpiConfig {
                interface: peripherals.SPI3,
                interrupt,
                clock_pin: peripherals.P0_08.degrade(),
                mosi_pin: peripherals.P0_06.degrade(),
                config,
            }),
        }
    }
}
