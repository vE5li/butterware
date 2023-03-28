use embassy_nrf::gpio::{AnyPin, Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::peripherals::SPI3;
use embassy_nrf::spim::{Config, Spim};

use self::debounce::DebouncedKey;
use crate::interface::{Keyboard, UnwrapInfelliable};

mod debounce;

pub struct SpiConfig {
    pub interface: SPI3,
    pub interrupt: embassy_nrf::interrupt::SPIM3,
    pub clock_pin: AnyPin,
    pub mosi_pin: AnyPin,
    pub config: Config,
}

pub struct ScanPinConfig<const C: usize, const R: usize> {
    pub columns: [AnyPin; C],
    pub rows: [AnyPin; R],
    pub power_pin: Option<AnyPin>,
    pub spi_config: Option<SpiConfig>,
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

        let spi = self.spi_config.map(|config| {
            Spim::new_txonly(
                config.interface,
                config.interrupt,
                config.clock_pin,
                config.mosi_pin,
                config.config,
            )
        });

        ScanPins {
            columns,
            rows,
            power_pin,
            spi,
        }
    }
}

pub struct ScanPins<'a, const C: usize, const R: usize> {
    pub columns: [Output<'a, AnyPin>; C],
    pub rows: [Input<'a, AnyPin>; R],
    pub power_pin: Option<Output<'a, AnyPin>>,
    pub spi: Option<Spim<'a, SPI3>>,
}

pub struct KeyboardState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    //pub active_layers: heapless::Vec<u8, 6>,
    pub keys: [[DebouncedKey<K>; K::ROWS]; K::COLUMNS],
    pub previous_key_state: u64,
}

impl<K> KeyboardState<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    const DEFAULT_KEY: DebouncedKey<K> = DebouncedKey::new();
    const DEFAULT_ROW: [DebouncedKey<K>; K::ROWS] = [Self::DEFAULT_KEY; K::ROWS];

    pub const fn new() -> Self {
        Self {
            //active_layers: heapless::Vec::new(),
            keys: [Self::DEFAULT_ROW; K::COLUMNS],
            previous_key_state: 0,
        }
    }
}
