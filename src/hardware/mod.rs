use embassy_nrf::gpio::{AnyPin, Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::peripherals::{SPI3, SPI2, TWISPI1};
use embassy_nrf::spim::{Config, Spim};

use self::debounce::DebouncedKey;
use crate::interface::{Keyboard, UnwrapInfelliable};
use crate::TestBit;

mod debounce;

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

impl<const C: usize, const R: usize> ScanPinConfig<C, R> {
    pub fn to_pins(self) -> ScanPins<'static, C, R> {
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

        let spi = self.spi_config.map(|config| {
            Spim::new_txonly(
                config.interface,
                config.interrupt,
                config.clock_pin,
                config.mosi_pin,
                config_foo,
            )
        });

        // FIX: fix colliding names
        let mut config_foo = Config::default();
        config_foo.frequency = embassy_nrf::spim::Frequency::M8;

        let spi_2 = self.spi_2_config.map(|config| {
            Spim::new_txonly(
                config.interface,
                config.interrupt,
                config.clock_pin,
                config.mosi_pin,
                config_foo,
            )
        });

        // FIX: fix colliding names
        let mut config_foo = Config::default();
        config_foo.frequency = embassy_nrf::spim::Frequency::M8;

        let spi_1 = self.spi_1_config.map(|config| {
            Spim::new_txonly(
                config.interface,
                config.interrupt,
                config.clock_pin,
                config.mosi_pin,
                config_foo,
            )
        });

        ScanPins {
            columns,
            rows,
            power_pin,
            spi,
            spi_2,
            spi_1,
        }
    }
}

pub struct ScanPins<'a, const C: usize, const R: usize> {
    pub columns: [Output<'a, AnyPin>; C],
    pub rows: [Input<'a, AnyPin>; R],
    pub power_pin: Option<Output<'a, AnyPin>>,
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

pub struct KeyboardState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    pub active_layers: heapless::Vec<ActiveLayer, { K::MAXIMUM_ACTIVE_LAYERS }>,
    pub keys: [[DebouncedKey<K>; K::ROWS]; K::COLUMNS],
    pub previous_key_state: u64,
    pub state_mask: u64,
}

impl<K> KeyboardState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    const DEFAULT_KEY: DebouncedKey<K> = DebouncedKey::new();
    const DEFAULT_ROW: [DebouncedKey<K>; K::ROWS] = [Self::DEFAULT_KEY; K::ROWS];

    pub const fn new() -> Self {
        Self {
            active_layers: heapless::Vec::new(),
            keys: [Self::DEFAULT_ROW; K::COLUMNS],
            previous_key_state: 0,
            state_mask: !0,
        }
    }

    pub fn current_layer_index(&self) -> usize {
        self.active_layers.last().map(|layer| layer.layer_index).unwrap_or(0)
    }

    pub fn lock_keys(&mut self) {
        for column in 0..K::COLUMNS {
            for row in 0..K::ROWS {
                let key_index = column * K::ROWS + row;

                // Only disable keys that are not part of a hold layer.
                if self.state_mask.test_bit(key_index) {
                    self.keys[column][row].lock();
                }
            }
        }
    }
}
