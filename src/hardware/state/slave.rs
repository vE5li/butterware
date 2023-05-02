use super::KeyState;
use crate::hardware::DebouncedKey;
use crate::interface::Keyboard;

// TODO: make fileds private?
pub struct SlaveState<K>
where
    K: Keyboard,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    pub keys: [[DebouncedKey<K>; K::ROWS]; K::COLUMNS],
    pub previous_key_state: u64,
}

impl<K> KeyState<K> for SlaveState<K>
where
    K: Keyboard,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    fn key(&mut self, column: usize, row: usize) -> &mut DebouncedKey<K> {
        &mut self.keys[column][row]
    }

    fn update_needs_synchronize(&mut self, new_state: u64) -> bool {
        let changed = self.previous_key_state != new_state;
        self.previous_key_state = new_state;
        changed
    }
}

impl<K> SlaveState<K>
where
    K: Keyboard,
    [(); K::MAXIMUM_ACTIVE_LAYERS]:,
    [(); K::COLUMNS * K::ROWS * 2]:,
{
    const DEFAULT_KEY: DebouncedKey<K> = DebouncedKey::new();
    const DEFAULT_ROW: [DebouncedKey<K>; K::ROWS] = [Self::DEFAULT_KEY; K::ROWS];

    pub const fn new() -> Self {
        Self {
            keys: [Self::DEFAULT_ROW; K::COLUMNS],
            previous_key_state: 0,
        }
    }
}
