use embassy_nrf::Peripherals;
use nrf_softdevice::ble::{Address, AddressType};

use crate::flash::FlashToken;
use crate::hardware::ScanPinConfig;
use crate::keys::Mapping;
#[cfg(feature = "lighting")]
use crate::led::{Animation, Led, Speed};

pub trait Scannable {
    const COLUMNS: usize;

    const ROWS: usize;

    /// Needs to be at least 1.
    const MAXIMUM_ACTIVE_LAYERS: usize = 6;
}

pub trait Keyboard: Scannable
where
    [(); Self::MAXIMUM_ACTIVE_LAYERS]:,
    [(); Self::COLUMNS * Self::ROWS * 2]:,
{
    /// Name presented to the connecting device.
    const DEVICE_NAME: &'static [u8];

    /// Key mappings.
    const LAYER_LOOKUP: &'static [&'static [Mapping; Self::COLUMNS * Self::ROWS * 2]];

    /// Bluetooth address of the left side before establishing a master.
    const LEFT_ADDRESS: Address = Address::new(AddressType::Public, [6, 2, 3, 4, 5, 9]);

    /// Bluetooth address of the right side before establishing a master.
    const RIGHT_ADDRESS: Address = Address::new(AddressType::Public, [7, 2, 3, 4, 5, 9]);

    /// Bluetooth address that the master will present itself with to other
    /// devices.
    const ADDRESS: Address = Address::new(AddressType::Public, [8, 2, 3, 4, 5, 9]);

    /// 32768 Ticks per second on the nice!nano. 100 Ticks is around 3
    /// milliseconds.
    const DEBOUNCE_TICKS: u64 = 100;

    /// 32768 Ticks per second on the nice!nano. 5000 Ticks is around 150
    /// milliseconds.
    const TAP_TIME: u64 = 5000;

    /// Number of pages in the flash to statically allocate for storing
    /// persistent data. Unless explicitly stated, this does not need to be
    /// increased.
    const SETTINGS_PAGES: usize = 1;

    /// Maximum number of bonds that can be stored permanently.
    const MAXIMUM_BONDS: usize = 10;

    /// Animation played when the halves are trying to find each other.
    #[cfg(feature = "lighting")]
    const SEARCH_ANIMATION: Animation = Animation::Pulsate {
        color: Led::rgb(1.0, 0.0, 0.0),
        speed: Speed(4.0),
        offset: 0.0,
    };

    /// Animation played on connecting only for the master side.
    #[cfg(feature = "lighting")]
    const MASTER_ANIMATION: Animation = Animation::Static {
        color: Led::rgb(1.0, 1.0, 1.0),
    };

    /// Animation played on connecting only for the slave side.
    #[cfg(feature = "lighting")]
    const SLAVE_ANIMATION: Animation = Animation::Static {
        color: Led::rgb(0.0, 0.0, 0.0),
    };

    /// Persistent data that is stored in the flash.
    type BoardFlash: Clone + defmt::Format = ();

    type Callbacks: Clone = !;

    /// Instantiate a new instance of the keyboard. This is only run once on
    /// boot.
    fn new(flash_token: FlashToken) -> Self;

    /// Function that gets called before initializing the peripherals. This is
    /// only run once on boot.
    async fn pre_initialize(&mut self) {}

    /// Initialize the peripherals. This is only run once on boot.
    async fn initialize_peripherals(&mut self, peripherals: Peripherals) -> ScanPinConfig<{ Self::COLUMNS }, { Self::ROWS }>;

    /// Function that gets called after initializing the peripherals. This is
    /// only run once on boot.
    async fn post_initialize(&mut self) {}

    /// Key press callback handler.
    async fn callback(&mut self, callback: Self::Callbacks) {
        let _ = callback;
        defmt::warn!("Callback handler not defined");
    }
}

pub trait KeyboardExtension {
    const KEYS_PER_SIDE: usize;
    const KEYS_TOTAL: usize;
}

impl<T: Keyboard> KeyboardExtension for T
where
    [(); T::MAXIMUM_ACTIVE_LAYERS]:,
    [(); T::COLUMNS * T::ROWS * 2]:,
{
    const KEYS_PER_SIDE: usize = Self::COLUMNS * Self::ROWS;
    const KEYS_TOTAL: usize = Self::COLUMNS * Self::ROWS * 2;
}
