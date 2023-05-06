use embassy_nrf::gpio::{AnyPin, Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::peripherals::{self, SPI2, SPI3, TWISPI1};
use embassy_nrf::spim::{self, Config, Spim};
use embassy_nrf::spis::MODE_1;

use crate::interface::UnwrapInfelliable;

mod debounce;
mod random;
mod state;

pub use self::debounce::DebouncedKey;
pub use self::random::generate_random_u32;
pub use self::state::{do_scan, KeyState, MasterState, SlaveState};

pub struct SpiConfig {
    pub interface: SPI3,
    pub interrupt: embassy_nrf::interrupt::SPIM3,
    pub clock_pin: AnyPin,
    pub mosi_pin: AnyPin,
}

pub struct Spi2Config {
    pub interface: SPI2,
    pub interrupt: embassy_nrf::interrupt::SPIM2_SPIS2_SPI2,
    pub clock_pin: AnyPin,
    pub mosi_pin: AnyPin,
}

pub struct Spi1Config {
    pub interface: TWISPI1,
    pub interrupt: embassy_nrf::interrupt::SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1,
    pub clock_pin: AnyPin,
    pub mosi_pin: AnyPin,
}

pub struct ScanPinConfig<const C: usize, const R: usize> {
    pub columns: [AnyPin; C],
    pub rows: [AnyPin; R],
    pub power_pin: Option<AnyPin>,
    pub spi_config: Option<SpiConfig>,
    pub spi_2_config: Option<Spi2Config>,
    pub spi_1_config: Option<Spi1Config>,
}

embassy_nrf::bind_interrupts!(struct Irqs {
    SPIM3 => spim::InterruptHandler<peripherals::SPI3>;
    SPIM2_SPIS2_SPI2 => spim::InterruptHandler<peripherals::SPI2>;
    SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1 => spim::InterruptHandler<peripherals::TWISPI1>;
});

impl<const C: usize, const R: usize> ScanPinConfig<C, R> {
    pub fn to_pins(self) -> (ScanPins<'static, C, R>, Spis<'static>) {
        let columns = self
            .columns
            .into_iter()
            .map(|pin| Output::new(pin, Level::Low, OutputDrive::Standard))
            .next_chunk()
            .unwrap_infelliable();

        let rows = self
            .rows
            .into_iter()
            .map(|pin| Input::new(pin, Pull::Down))
            .next_chunk()
            .unwrap_infelliable();

        let power_pin = self.power_pin.map(|pin| Output::new(pin, Level::High, OutputDrive::Standard));

        // FIX: fix colliding names
        let mut config_foo = Config::default();
        config_foo.frequency = embassy_nrf::spim::Frequency::M8;
        config_foo.mode = MODE_1;

        let spi = self
            .spi_config
            .map(|config| Spim::new_txonly(config.interface, Irqs, config.clock_pin, config.mosi_pin, config_foo));

        // FIX: fix colliding names
        let mut config_foo = Config::default();
        config_foo.frequency = embassy_nrf::spim::Frequency::M8;
        config_foo.mode = MODE_1;

        let spi_2 = self
            .spi_2_config
            .map(|config| Spim::new_txonly(config.interface, Irqs, config.clock_pin, config.mosi_pin, config_foo));

        // FIX: fix colliding names
        let mut config_foo = Config::default();
        config_foo.frequency = embassy_nrf::spim::Frequency::M8;
        config_foo.mode = MODE_1;

        let spi_1 = self
            .spi_1_config
            .map(|config| Spim::new_txonly(config.interface, Irqs, config.clock_pin, config.mosi_pin, config_foo));

        (ScanPins { columns, rows, power_pin }, Spis { spi, spi_2, spi_1 })
    }
}

pub struct ScanPins<'a, const C: usize, const R: usize> {
    pub columns: [Output<'a, AnyPin>; C],
    pub rows: [Input<'a, AnyPin>; R],
    pub power_pin: Option<Output<'a, AnyPin>>,
}

pub struct Spis<'a> {
    pub spi: Option<Spim<'a, SPI3>>,
    pub spi_2: Option<Spim<'a, SPI2>>,
    pub spi_1: Option<Spim<'a, TWISPI1>>,
}

#[derive(Debug)]
pub struct ActiveLayer {
    pub layer_index: usize,
    pub key_index: usize,
    pub tap_timer: Option<u64>,
}

// TODO: rename to BitOperations or similar
pub trait TestBit {
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
