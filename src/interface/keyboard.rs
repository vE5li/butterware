use embassy_nrf::Peripherals;
use nrf_softdevice::ble::{Address, AddressType};

use crate::hardware::ScanPinConfig;
use crate::keys::Mapping;

pub trait Scannable {
    const COLUMNS: usize;

    const ROWS: usize;

    // Needs to be at least 1.
    const MAXIMUM_ACTIVE_LAYERS: usize = 6;
}

pub trait Keyboard: Scannable
where
    [(); Self::MAXIMUM_ACTIVE_LAYERS]:,
    [(); Self::COLUMNS * Self::ROWS * 2]:,
{
    const DEVICE_NAME: &'static [u8];

    const LAYER_LOOKUP: &'static [&'static [Mapping; Self::COLUMNS * Self::ROWS * 2]];

    const LEFT_ADDRESS: Address = Address::new(AddressType::Public, [6, 2, 3, 4, 5, 9]);
    const RIGHT_ADDRESS: Address = Address::new(AddressType::Public, [7, 2, 3, 4, 5, 9]);
    const ADDRESS: Address = Address::new(AddressType::Public, [8, 2, 3, 4, 5, 9]);

    // 32768 Ticks per second on the nice!nano. 100 Ticks is around 3 milliseconds.
    const DEBOUNCE_TICKS: u64 = 100;

    // 32768 Ticks per second on the nice!nano. 5000 Ticks is around 150
    // milliseconds.
    const TAP_TIME: u64 = 5000;

    fn new() -> Self;

    fn init_peripherals(&mut self, peripherals: Peripherals) -> ScanPinConfig<{ Self::COLUMNS }, { Self::ROWS }>;
}

pub trait KeyboardExtension {
    const KEY_COUNT: usize;
}

impl<T: Keyboard> KeyboardExtension for T
where
    [(); T::MAXIMUM_ACTIVE_LAYERS]:,
    [(); T::COLUMNS * T::ROWS * 2]:,
{
    const KEY_COUNT: usize = Self::COLUMNS * Self::ROWS;
}
