mod master;
mod slave;

use embassy_time::{Duration, Timer};

pub use self::master::MasterState;
pub use self::slave::SlaveState;
use super::{DebouncedKey, MatrixPins};
use crate::interface::Scannable;

pub trait KeyState {
    fn key(&mut self, column: usize, row: usize) -> &mut DebouncedKey;

    /// Update the key state and check if the external state needs to be
    /// updated.
    fn update_needs_synchronize(&mut self, new_state: u64) -> bool;
}

pub async fn do_scan(
    state: &mut impl KeyState,
    matrix_pins: &mut MatrixPins<'_, { <crate::Used as Scannable>::COLUMNS }, { <crate::Used as Scannable>::ROWS }>,
) -> u64 {
    loop {
        let mut key_state = 0;
        let mut offset = 0;

        for (column_index, column) in matrix_pins.columns.iter_mut().enumerate() {
            column.set_high();

            for (row_index, row) in matrix_pins.rows.iter().enumerate() {
                let raw_state = row.is_high();
                state.key(column_index, row_index).update(raw_state);

                key_state |= (state.key(column_index, row_index).is_down() as u64) << offset;
                offset += 1;
            }

            column.set_low();
        }

        if state.update_needs_synchronize(key_state) {
            return key_state;
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}
