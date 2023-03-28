use embassy_nrf::Peripherals;

use crate::hardware::ScanPinConfig;
use crate::keys::Mapping;

pub trait Scannable {
    const COLUMNS: usize;
    const ROWS: usize;
    const NAME_LENGTH: usize;
}

pub trait Keyboard: Scannable
where
    [(); Self::NAME_LENGTH]:,
    [(); Self::COLUMNS * Self::ROWS]:,
{
    const DEVICE_NAME: &'static [u8; Self::NAME_LENGTH];

    const MATRIX: [usize; Self::COLUMNS * Self::ROWS];

    const LAYER_LOOKUP: &'static [&'static [Mapping; Self::COLUMNS * Self::ROWS]];

    // 32768 Ticks per second on the nice!nano. 100 Ticks is around 3 milliseconds
    const DEBOUNCE_TICKS: u64 = 100;

    fn new() -> Self;

    fn init_peripherals(&mut self, peripherals: Peripherals) -> ScanPinConfig<{ Self::COLUMNS }, { Self::ROWS }>;
}
