use embassy_nrf::Peripherals;
use nrf_softdevice::ble::{Address, AddressType};

use crate::flash::FlashToken;
use crate::hardware::ScanPinConfig;
use crate::keys::Mapping;
use crate::led::{Animation, Led, Speed};

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

    // Maximum number of bonds that can be stored permanently.
    const MAXIMUM_BONDS: usize = 10;

    // Lighting effects
    const SEARCH_ANIMATION: Animation = Animation::Pulsate {
        color: Led::rgb(1.0, 0.0, 0.0),
        speed: Speed(4.0),
        offset: 0.0,
    };
    const MASTER_ANIMATION: Animation = Animation::Static {
        color: Led::rgb(1.0, 1.0, 1.0),
    };
    const SLAVE_ANIMATION: Animation = Animation::Static {
        color: Led::rgb(0.0, 0.0, 0.0),
    };

    type BoardFlash = ();

    fn new(flash_token: FlashToken) -> Self;

    async fn pre_initialize(&mut self) {}

    async fn initialize_peripherals(&mut self, peripherals: Peripherals) -> ScanPinConfig<{ Self::COLUMNS }, { Self::ROWS }>;

    async fn post_initialize(&mut self) {}

    async fn callback(&mut self, id: u32) {
        let _ = id;
        defmt::warn!("Callback handler not defined");
    }
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
