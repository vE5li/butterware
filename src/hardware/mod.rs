use embassy_cortex_m::interrupt::Interrupt;
use embassy_nrf::gpio::{AnyPin, Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::interrupt;

use crate::interface::{Keyboard, UnwrapInfelliable};
use crate::led::LedProvider;

mod debounce;
mod random;
mod state;

pub use self::debounce::DebouncedKey;
pub use self::random::generate_random_u32;
pub use self::state::{do_scan, KeyState, MasterState, SlaveState};

pub type UsedLeds = <<crate::Used as Keyboard>::Leds as LedProvider>::Collection;

pub struct ScanPinConfig<const C: usize, const R: usize> {
    pub columns: [AnyPin; C],
    pub rows: [AnyPin; R],
    pub power_pin: Option<AnyPin>,
    pub leds: <<crate::Used as Keyboard>::Leds as LedProvider>::Collection,
}

impl<const C: usize, const R: usize> ScanPinConfig<C, R> {
    pub fn to_pins(self) -> (ScanPins<'static, C, R>, UsedLeds) {
        use embassy_nrf::interrupt::InterruptExt;

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

        unsafe { interrupt::SPIM3::steal() }.set_priority(interrupt::Priority::P2);
        unsafe { interrupt::SPIM2_SPIS2_SPI2::steal() }.set_priority(interrupt::Priority::P2);
        unsafe { interrupt::SPIM1_SPIS1_TWIM1_TWIS1_SPI1_TWI1::steal() }.set_priority(interrupt::Priority::P2);

        (ScanPins { columns, rows, power_pin }, self.leds)
    }
}

pub struct ScanPins<'a, const C: usize, const R: usize> {
    pub columns: [Output<'a, AnyPin>; C],
    pub rows: [Input<'a, AnyPin>; R],
    pub power_pin: Option<Output<'a, AnyPin>>,
}

#[derive(Debug)]
pub struct ActiveLayer {
    pub layer_index: usize,
    pub key_index: usize,
    pub tap_timer: Option<u64>,
}

pub trait BitOperations {
    fn test_bit(self, offset: usize) -> bool;

    fn clear_bit(&mut self, offset: usize);

    fn set_bit(&mut self, offset: usize);
}

impl BitOperations for u64 {
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
