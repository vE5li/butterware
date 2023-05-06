use super::KeyState;
use crate::hardware::DebouncedKey;
use crate::interface::Scannable;

// TODO: make fileds private?
pub struct SlaveState {
    pub keys: [[DebouncedKey; <crate::Used as Scannable>::ROWS]; <crate::Used as Scannable>::COLUMNS],
    pub previous_key_state: u64,
}

impl KeyState for SlaveState {
    fn key(&mut self, column: usize, row: usize) -> &mut DebouncedKey {
        &mut self.keys[column][row]
    }

    fn update_needs_synchronize(&mut self, new_state: u64) -> bool {
        let changed = self.previous_key_state != new_state;
        self.previous_key_state = new_state;
        changed
    }
}

impl SlaveState {
    const DEFAULT_KEY: DebouncedKey = DebouncedKey::new();
    const DEFAULT_ROW: [DebouncedKey; <crate::Used as Scannable>::ROWS] = [Self::DEFAULT_KEY; <crate::Used as Scannable>::ROWS];

    pub const fn new() -> Self {
        Self {
            keys: [Self::DEFAULT_ROW; <crate::Used as Scannable>::COLUMNS],
            previous_key_state: 0,
        }
    }
}
