use embassy_time::driver::now;

use super::KeyState;
use crate::flash::{remove_bond, reset_persistent_data};
use crate::hardware::{ActiveLayer, ActiveModifier, BitOperations, DebouncedKey};
use crate::interface::{Keyboard, KeyboardExtension, Scannable};
use crate::keys::{Mapping, Modifiers};
#[cfg(feature = "lighting")]
use crate::led::set_animation;
use crate::power::set_power_state;

// TODO: make fileds private?
pub struct MasterState {
    pub active_layers: heapless::Vec<ActiveLayer, { <crate::Used as Scannable>::MAXIMUM_ACTIVE_LAYERS }>,
    pub active_modifiers: heapless::Vec<ActiveModifier, 8>,
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
            active_modifiers: heapless::Vec::new(),
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

    pub async fn apply(
        &mut self,
        keyboard: &mut crate::Used,
        mut key_state: u64,
    ) -> Option<(heapless::Vec<ActiveModifier, 8>, usize, u64, u64)> {
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

        // Try to pop modifiers
        // TEMP
        let mut send_again = false;
        for index in (0..self.active_modifiers.len()).rev() {
            let key_index = self.active_modifiers[index].key_index;

            if !key_state.test_bit(key_index) {
                // Check if we want to execute the tap action for this key (if
                // present).
                if matches!(self.active_modifiers[index].tap_timer, Some(time) if now() - time < <crate::Used as Keyboard>::TAP_TIME) {
                    injected_keys.set_bit(key_index);
                }

                self.state_mask.set_bit(key_index);

                // TODO: is this needed?
                key_state &= !self.state_mask;

                self.active_modifiers.remove(index);
                send_again = true;
            }
        }

        // Ignore all keys that are held as part of a layer.
        key_state &= self.state_mask;

        // Ignore all locked keys.
        key_state &= !self.lock_mask;

        enum StackAction {
            Layer { index: usize, time: Option<u64> },
            Modifier { value: Modifiers, time: Option<u64> },
        }

        if key_state | injected_keys != self.previous_key_state || send_again {
            // FIX: unclear what happens if we press multiple layer keys on the same
            // event

            let active_layer = <crate::Used as Keyboard>::LAYER_LOOKUP[self.current_layer_index()];

            for key_index in 0..<crate::Used as KeyboardExtension>::KEYS_TOTAL {
                // Get layer index and optional tap key.
                let stack_action = match &active_layer[key_index] {
                    Mapping::Tap(tap_action) => match tap_action {
                        crate::keys::TapAction::Keycode(..) => continue,
                        crate::keys::TapAction::Special(special_action) => {
                            if key_state.test_bit(key_index) {
                                match special_action {
                                    crate::keys::SpecialAction::RemoveBond { side, bond_slot } => {
                                        remove_bond(*side, *bond_slot).await;
                                    }
                                    crate::keys::SpecialAction::ResetPersistentData { side } => {
                                        reset_persistent_data(*side).await;
                                    }
                                    #[cfg(feature = "lighting")]
                                    crate::keys::SpecialAction::SetAnimation { side, index, animation } => {
                                        set_animation(*side, index.clone(), animation.clone()).await;
                                    }
                                    crate::keys::SpecialAction::SetPower { side, state } => {
                                        set_power_state(*side, *state).await;
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
                    },
                    Mapping::Hold(hold_action) => match hold_action {
                        crate::keys::HoldAction::Layer(layer_index) => StackAction::Layer {
                            index: *layer_index,
                            time: None,
                        },
                        crate::keys::HoldAction::Modifier(modifier) => StackAction::Modifier {
                            value: *modifier,
                            time: None,
                        },
                    },
                    Mapping::HoldTap(hold_action, _) => match hold_action {
                        crate::keys::HoldAction::Layer(layer_index) => StackAction::Layer {
                            index: *layer_index,
                            time: Some(now()),
                        },
                        crate::keys::HoldAction::Modifier(modifier) => StackAction::Modifier {
                            value: *modifier,
                            time: Some(now()),
                        },
                    },
                };

                match stack_action {
                    StackAction::Layer { index, time } => {
                        // Make sure that the same layer is not pushed twice in a row
                        if key_state.test_bit(key_index) {
                            // If we already have an active layer, we set it's timer to `None` to prevent
                            // the tap action from executing if both layer
                            // keys are released quickly.
                            if let Some(active_layer) = self.active_layers.last_mut() {
                                active_layer.tap_timer = None;
                            }

                            if let Some(active_modifier) = self.active_modifiers.last_mut() {
                                active_modifier.tap_timer = None;
                            }

                            let new_active_layer = ActiveLayer {
                                layer_index: index,
                                key_index,
                                tap_timer: time,
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
                    StackAction::Modifier { value, time } => {
                        // Make sure that the same layer is not pushed twice in a row
                        // TODO: what is this actually
                        if key_state.test_bit(key_index) {
                            // If we already have an active layer, we set it's timer to `None` to prevent
                            // the tap action from executing if both layer
                            // keys are released quickly.
                            if let Some(active_layer) = self.active_layers.last_mut() {
                                active_layer.tap_timer = None;
                            }

                            if let Some(active_modifier) = self.active_modifiers.last_mut() {
                                active_modifier.tap_timer = None;
                            }

                            let new_active_modifier = ActiveModifier {
                                value,
                                key_index,
                                tap_timer: time,
                            };

                            self.active_modifiers.push(new_active_modifier).unwrap();

                            self.state_mask.clear_bit(key_index);

                            // For now we just set the entire key_state to 0
                            key_state = 0;
                        }
                    }
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

                if let Some(active_modifier) = self.active_modifiers.last_mut() {
                    active_modifier.tap_timer = None;
                }
            }

            // Since we might have altered the key state we check again if it changed
            // to avoid sending the same input report multiple times.
            if key_state | injected_keys != self.previous_key_state || send_again {
                self.previous_key_state = key_state;

                return Some((
                    self.active_modifiers.clone(),
                    self.current_layer_index(),
                    key_state,
                    injected_keys,
                ));
            }
        }

        None
    }
}
