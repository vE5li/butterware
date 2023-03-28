use core::marker::PhantomData;

use crate::interface::Keyboard;

#[derive(Debug, Clone, Copy)]
pub struct DebouncedKey<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    last_state_change: u64,
    internal_state: bool,
    output_state: bool,
    phantom_data: PhantomData<K>,
}

impl<K> DebouncedKey<K>
where
    K: Keyboard,
    [(); K::NAME_LENGTH]:,
    [(); K::COLUMNS * K::ROWS]:,
{
    pub const fn new() -> Self {
        Self {
            last_state_change: 0,
            internal_state: false,
            output_state: false,
            phantom_data: PhantomData,
        }
    }

    pub fn update(&mut self, new_state: bool) {
        const INTEGER_STATE: [u64; 2] = [0x0, !0x0];
        const BOOL_STATE: [bool; 2] = [false, true];

        let now = embassy_time::driver::now();

        // Branchless set of last_state_change. If new_state != internal_state
        // last_state_change will be set to now, otherwise it remains unchanged.
        let state_changed = self.internal_state != new_state;
        self.last_state_change =
            (INTEGER_STATE[(!state_changed) as usize] & self.last_state_change) | (INTEGER_STATE[state_changed as usize] & now);

        self.internal_state = new_state;

        // Branchless set of output_state. If the number of ticks since the last state
        // change is greater that the debounce ticks we set output_state =
        // internal_state.
        let debounced = now - self.last_state_change >= K::DEBOUNCE_TICKS;
        self.output_state =
            (BOOL_STATE[(!debounced) as usize] && self.output_state) || (BOOL_STATE[debounced as usize] && self.internal_state);
    }

    pub fn is_down(&self) -> bool {
        self.output_state
    }
}
