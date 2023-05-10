use embassy_time::driver::now;

use super::KeyState;
use crate::flash::FlashTransaction;
use crate::hardware::{ActiveLayer, BitOperations, DebouncedKey};
use crate::interface::{Keyboard, KeyboardExtension, Scannable};
use crate::keys::Mapping;
use crate::led::set_animation;

// TODO: make fileds private?
pub struct MasterState {
    pub active_layers: heapless::Vec<ActiveLayer, { <crate::Used as Scannable>::MAXIMUM_ACTIVE_LAYERS }>,
    pub keys: [[DebouncedKey; <crate::Used as Scannable>::ROWS]; <crate::Used as Scannable>::COLUMNS],
    pub previous_key_state: u64,
    pub master_raw_state: u64,
    pub slave_raw_state: u64,
    pub state_mask: u64,
    pub lock_mask: u64,
}

impl KeyState for MasterState {
    fn key(&mut self, column: usize, row: usize) -> &mut DebouncedKey {
        &mut self.keys[column][row]
    }

    fn update_needs_synchronize(&mut self, new_state: u64) -> bool {
        let changed = self.master_raw_state != new_state;
        self.master_raw_state = new_state;
        changed
    }
}

impl MasterState {
    const DEFAULT_KEY: DebouncedKey = DebouncedKey::new();
    const DEFAULT_ROW: [DebouncedKey; <crate::Used as Scannable>::ROWS] = [Self::DEFAULT_KEY; <crate::Used as Scannable>::ROWS];

    pub const fn new() -> Self {
        Self {
            active_layers: heapless::Vec::new(),
            keys: [Self::DEFAULT_ROW; <crate::Used as Scannable>::COLUMNS],
            previous_key_state: 0,
            master_raw_state: 0,
            slave_raw_state: 0,
            state_mask: !0,
            lock_mask: 0,
        }
    }

    pub fn current_layer_index(&self) -> usize {
        self.active_layers.last().map(|layer| layer.layer_index).unwrap_or(0)
    }

    pub async fn apply(&mut self, keyboard: &mut crate::Used, mut key_state: u64) -> Option<(usize, u64, u64)> {
        let mut injected_keys = 0;

        // TODO: make key_state immutable and copy to modify instead.
        let saved_state = key_state;

        // We do this before popping the layers to avoid clearing the mask instantly.
        self.lock_mask &= key_state;

        // Try to pop layers
        while let Some(active_layer) = self.active_layers.last() {
            let key_index = active_layer.key_index;

            match key_state.test_bit(key_index) {
                true => break,
                false => {
                    // Check if we want to execute the tap action for this layer (if
                    // present).
                    if matches!(active_layer.tap_timer, Some(time) if now() - time < <crate::Used as Keyboard>::TAP_TIME) {
                        injected_keys.set_bit(key_index);
                    }

                    self.active_layers.pop();

                    // We lock all keys except the layer keys. This avoids
                    // cases where we leave a layer while holding a key and we
                    // send the key again but from the lower layer.
                    self.lock_mask = self.state_mask & saved_state;

                    // Add layer key to the mask again (re-enable the key).
                    self.state_mask.set_bit(key_index);

                    // For now we unset all non-layer keys so we don't get any key
                    // presses form the current layer.
                    key_state &= !self.state_mask;
                }
            }
        }

        // Ignore all keys that are held as part of a layer.
        key_state &= self.state_mask;

        // Ignore all locked keys.
        key_state &= !self.lock_mask;

        if key_state | injected_keys != self.previous_key_state {
            // FIX: unclear what happens if we press multiple layer keys on the same
            // event

            let active_layer = <crate::Used as Keyboard>::LAYER_LOOKUP[self.current_layer_index()];

            for key_index in 0..<crate::Used as KeyboardExtension>::KEYS_TOTAL {
                // Get layer index and optional tap key.
                let (layer_index, tap_timer) = match active_layer[key_index] {
                    Mapping::Key(..) => continue,
                    Mapping::Layer(layer_index) => (layer_index, None),
                    Mapping::TapLayer(layer_index, _) => (layer_index, Some(now())),
                    Mapping::Special(ref special_action) => {
                        if key_state.test_bit(key_index) {
                            match special_action {
                                crate::keys::SpecialAction::RemoveBond { side, bond_slot } => {
                                    FlashTransaction::new().remove_bond(*side, *bond_slot).apply().await;
                                }
                                crate::keys::SpecialAction::SwitchAnimation { side, index, animation } => {
                                    set_animation(*side, index.clone(), animation.clone()).await;
                                }
                                crate::keys::SpecialAction::Callback(callback) => {
                                    keyboard.callback(callback.clone()).await;
                                }
                            }

                            // Necessary so that the special key does not get sent.
                            key_state.clear_bit(key_index);
                        }

                        continue;
                    }
                };

                // Make sure that the same layer is not pushed twice in a row
                if key_state.test_bit(key_index) {
                    // If we already have an active layer, we set it's timer to `None` to prevent
                    // the tap action from executing if both layer
                    // keys are released quickly.
                    if let Some(active_layer) = self.active_layers.last_mut() {
                        active_layer.tap_timer = None;
                    }

                    let new_active_layer = ActiveLayer {
                        layer_index,
                        key_index,
                        tap_timer,
                    };

                    self.active_layers.push(new_active_layer).expect("Active layer limit reached");

                    // Remove the key from the state mask (disable the key). This
                    // helps cut down on expensive updates and also ensures that we
                    // don't get any modifier keys in send_input_report.
                    self.state_mask.clear_bit(key_index);

                    // We lock all keys except the layer keys. This avoids
                    // cases where we enter a layer while holding a key and we
                    // send the key again but from the new layer.
                    self.lock_mask = self.state_mask & saved_state;

                    // For now we just set the entire key_state to 0
                    key_state = 0;
                }
            }

            // If the key state is not zero, that there is at least one non-layer
            // button pressed, since layer keys are masked out.
            if key_state != 0 {
                // If a regular key is pressed and there is an active layer, we set it's timer
                // to `None` to prevent the tap action from
                // executing if the layer key is released quickly.
                if let Some(active_layer) = self.active_layers.last_mut() {
                    active_layer.tap_timer = None;
                }
            }

            // Since we might have altered the key state we check again if it changed
            // to avoid sending the same input report multiple times.
            if key_state | injected_keys != self.previous_key_state {
                self.previous_key_state = key_state;

                return Some((self.current_layer_index(), key_state, injected_keys));
            }
        }

        None
    }
}
