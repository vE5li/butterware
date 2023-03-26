use crate::flash::BondSlot;
use crate::interface::Keyboard;
use crate::power::PowerState;
#[cfg(feature = "lighting")]
use crate::led::{Animation, LedIndex};
use crate::Side;

pub mod german;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Modifiers: u8 {
        const NONE = 0;
        const LCTRL = 0x01;
        const LSHIFT = 0x02;
        const LALT = 0x04;
        const LMETA = 0x08;
        const RCTRL = 0x10;
        const RSHIFT = 0x20;
        const RALT = 0x40;
        const RMETA = 0x80;
    }
}

pub struct Key(u8, Modifiers);
pub struct Layer(pub usize);

impl Key {
    pub const fn from_keycode(keycode: u8) -> Self {
        Self(keycode, Modifiers::NONE)
    }

    pub const fn shift(mut self) -> Self {
        self.1 = self.1.union(Modifiers::LSHIFT);
        self
    }

    pub const fn alt_gr(mut self) -> Self {
        self.1 = self.1.union(Modifiers::RALT);
        self
    }

    pub const fn get_value(&self) -> u8 {
        self.0
    }

    pub const fn get_modifiers(&self) -> Modifiers {
        self.1
    }
}

pub enum SpecialAction {
    RemoveBond {
        side: Side,
        bond_slot: BondSlot,
    },
    ResetPersistentData {
        side: Side,
    },
    SetPower {
        side: Side,
        state: PowerState,
    },
    #[cfg(feature = "lighting")]
    SetAnimation {
        side: Side,
        index: LedIndex,
        animation: Animation,
    },
    Callback(<crate::Used as Keyboard>::Callbacks),
}

#[const_trait]
pub trait IntoTapAction {
    fn into_tap_action(self) -> TapAction;
}

impl const IntoTapAction for Key {
    fn into_tap_action(self) -> TapAction {
        TapAction::Keycode(self.0, self.1)
    }
}

impl const IntoTapAction for SpecialAction {
    fn into_tap_action(self) -> TapAction {
        TapAction::Special(self)
    }
}

#[const_trait]
pub trait IntoHoldAction {
    fn into_hold_action(self) -> HoldAction;
}

impl const IntoHoldAction for Layer {
    fn into_hold_action(self) -> HoldAction {
        HoldAction::Layer(self.0)
    }
}

impl const IntoHoldAction for Modifiers {
    fn into_hold_action(self) -> HoldAction {
        HoldAction::Modifier(self)
    }
}

pub enum TapAction {
    Keycode(u8, Modifiers),
    Special(SpecialAction),
}

pub enum HoldAction {
    Layer(usize),
    Modifier(Modifiers),
}

pub enum Mapping {
    Tap(TapAction),
    Hold(HoldAction),
    HoldTap(HoldAction, TapAction),
}

#[const_trait]
pub trait IntoMapping {
    fn into_mapping(self) -> Mapping;
}

impl const IntoMapping for Mapping {
    fn into_mapping(self) -> Mapping {
        self
    }
}

impl const IntoMapping for TapAction {
    fn into_mapping(self) -> Mapping {
        Mapping::Tap(self)
    }
}

impl const IntoMapping for HoldAction {
    fn into_mapping(self) -> Mapping {
        Mapping::Hold(self)
    }
}

impl const IntoMapping for Key {
    fn into_mapping(self) -> Mapping {
        Mapping::Tap(self.into_tap_action())
    }
}

impl const IntoMapping for SpecialAction {
    fn into_mapping(self) -> Mapping {
        Mapping::Tap(self.into_tap_action())
    }
}

impl const IntoMapping for Layer {
    fn into_mapping(self) -> Mapping {
        Mapping::Hold(self.into_hold_action())
    }
}

impl const IntoMapping for Modifiers {
    fn into_mapping(self) -> Mapping {
        Mapping::Hold(self.into_hold_action())
    }
}

pub const fn hold_tap(hold: impl ~const IntoHoldAction, tap: impl ~const IntoTapAction) -> Mapping {
    Mapping::HoldTap(hold.into_hold_action(), tap.into_tap_action())
}

pub const MOD_LCTRL: Modifiers = Modifiers::LCTRL;
pub const MOD_LSHIFT: Modifiers = Modifiers::LSHIFT;
pub const MOD_LALT: Modifiers = Modifiers::LALT;
pub const MOD_LMETA: Modifiers = Modifiers::LMETA;
pub const MOD_RCTRL: Modifiers = Modifiers::RCTRL;
pub const MOD_RSHIFT: Modifiers = Modifiers::RSHIFT;
pub const MOD_RALT: Modifiers = Modifiers::RALT;
pub const MOD_RMETA: Modifiers = Modifiers::RMETA;

/**
 * Scan codes - last N slots in the HID report (usually 6).
 * 0x00 if no key pressed.
 *
 * If more than N keys are pressed, the HID reports
 * ERR_OVF in all slots to indicate this condition.
 */

pub const NONE: Key = Key::from_keycode(0x00); // No key pressed
pub const ERR_OVF: Key = Key::from_keycode(0x01); //  Keyboard Error Roll Over - used for all slots if too many keys are pressed ("Phantom key")
// 0x02 //  Keycodeboard POST Fail
// 0x03 //  Keycodeboard Error Undefined
pub const A: Key = Key::from_keycode(0x04); // Keyboard a and A
pub const B: Key = Key::from_keycode(0x05); // Keyboard b and B
pub const C: Key = Key::from_keycode(0x06); // Keyboard c and C
pub const D: Key = Key::from_keycode(0x07); // Keyboard d and D
pub const E: Key = Key::from_keycode(0x08); // Keyboard e and E
pub const F: Key = Key::from_keycode(0x09); // Keyboard f and F
pub const G: Key = Key::from_keycode(0x0a); // Keyboard g and G
pub const H: Key = Key::from_keycode(0x0b); // Keyboard h and H
pub const I: Key = Key::from_keycode(0x0c); // Keyboard i and I
pub const J: Key = Key::from_keycode(0x0d); // Keyboard j and J
pub const K: Key = Key::from_keycode(0x0e); // Keyboard k and K
pub const L: Key = Key::from_keycode(0x0f); // Keyboard l and L
pub const M: Key = Key::from_keycode(0x10); // Keyboard m and M
pub const N: Key = Key::from_keycode(0x11); // Keyboard n and N
pub const O: Key = Key::from_keycode(0x12); // Keyboard o and O
pub const P: Key = Key::from_keycode(0x13); // Keyboard p and P
pub const Q: Key = Key::from_keycode(0x14); // Keyboard q and Q
pub const R: Key = Key::from_keycode(0x15); // Keyboard r and R
pub const S: Key = Key::from_keycode(0x16); // Keyboard s and S
pub const T: Key = Key::from_keycode(0x17); // Keyboard t and T
pub const U: Key = Key::from_keycode(0x18); // Keyboard u and U
pub const V: Key = Key::from_keycode(0x19); // Keyboard v and V
pub const W: Key = Key::from_keycode(0x1a); // Keyboard w and W
pub const X: Key = Key::from_keycode(0x1b); // Keyboard x and X
pub const Y: Key = Key::from_keycode(0x1c); // Keyboard y and Y
pub const Z: Key = Key::from_keycode(0x1d); // Keyboard z and Z

pub const N1: Key = Key::from_keycode(0x1e); // Keyboard 1 and !
pub const N2: Key = Key::from_keycode(0x1f); // Keyboard 2 and @
pub const N3: Key = Key::from_keycode(0x20); // Keyboard 3 and #
pub const N4: Key = Key::from_keycode(0x21); // Keyboard 4 and $
pub const N5: Key = Key::from_keycode(0x22); // Keyboard 5 and %
pub const N6: Key = Key::from_keycode(0x23); // Keyboard 6 and ^
pub const N7: Key = Key::from_keycode(0x24); // Keyboard 7 and &
pub const N8: Key = Key::from_keycode(0x25); // Keyboard 8 and *
pub const N9: Key = Key::from_keycode(0x26); // Keyboard 9 and (
pub const N0: Key = Key::from_keycode(0x27); // Keyboard 0 and )

pub const ENTER: Key = Key::from_keycode(0x28); // Keyboard Return (ENTER)
pub const ESC: Key = Key::from_keycode(0x29); // Keyboard ESCAPE
pub const BACKSPACE: Key = Key::from_keycode(0x2a); // Keyboard DELETE (Backspace)
pub const TAB: Key = Key::from_keycode(0x2b); // Keyboard Tab
pub const SPACE: Key = Key::from_keycode(0x2c); // Keyboard Spacebar
pub const MINUS: Key = Key::from_keycode(0x2d); // Keyboard - and _
pub const EQUAL: Key = Key::from_keycode(0x2e); // Keyboard = and +
pub const LEFTBRACE: Key = Key::from_keycode(0x2f); // Keyboard [ and {
pub const RIGHTBRACE: Key = Key::from_keycode(0x30); // Keyboard ] and }
pub const BACKSLASH: Key = Key::from_keycode(0x31); // Keyboard \ and |
pub const HASHTILDE: Key = Key::from_keycode(0x32); // Keyboard Non-US # and ~
pub const SEMICOLON: Key = Key::from_keycode(0x33); // Keyboard ; and :
pub const APOSTROPHE: Key = Key::from_keycode(0x34); // Keyboard ' and "
pub const GRAVE: Key = Key::from_keycode(0x35); // Keyboard ` and ~
pub const COMMA: Key = Key::from_keycode(0x36); // Keyboard , and <
pub const DOT: Key = Key::from_keycode(0x37); // Keyboard . and >
pub const SLASH: Key = Key::from_keycode(0x38); // Keyboard / and ?
pub const CAPSLOCK: Key = Key::from_keycode(0x39); // Keyboard Caps Lock

pub const F1: Key = Key::from_keycode(0x3a); // Keyboard F1
pub const F2: Key = Key::from_keycode(0x3b); // Keyboard F2
pub const F3: Key = Key::from_keycode(0x3c); // Keyboard F3
pub const F4: Key = Key::from_keycode(0x3d); // Keyboard F4
pub const F5: Key = Key::from_keycode(0x3e); // Keyboard F5
pub const F6: Key = Key::from_keycode(0x3f); // Keyboard F6
pub const F7: Key = Key::from_keycode(0x40); // Keyboard F7
pub const F8: Key = Key::from_keycode(0x41); // Keyboard F8
pub const F9: Key = Key::from_keycode(0x42); // Keyboard F9
pub const F10: Key = Key::from_keycode(0x43); // Keyboard F10
pub const F11: Key = Key::from_keycode(0x44); // Keyboard F11
pub const F12: Key = Key::from_keycode(0x45); // Keyboard F12

pub const SYSRQ: Key = Key::from_keycode(0x46); // Keyboard Print Screen
pub const SCROLLLOCK: Key = Key::from_keycode(0x47); // Keyboard Scroll Lock
pub const PAUSE: Key = Key::from_keycode(0x48); // Keyboard Pause
pub const INSERT: Key = Key::from_keycode(0x49); // Keyboard Insert
pub const HOME: Key = Key::from_keycode(0x4a); // Keyboard Home
pub const PAGEUP: Key = Key::from_keycode(0x4b); // Keyboard Page Up
pub const DELETE: Key = Key::from_keycode(0x4c); // Keyboard Delete Forward
pub const END: Key = Key::from_keycode(0x4d); // Keyboard End
pub const PAGEDOWN: Key = Key::from_keycode(0x4e); // Keyboard Page Down
pub const RIGHT: Key = Key::from_keycode(0x4f); // Keyboard Right Arrow
pub const LEFT: Key = Key::from_keycode(0x50); // Keyboard Left Arrow
pub const DOWN: Key = Key::from_keycode(0x51); // Keyboard Down Arrow
pub const UP: Key = Key::from_keycode(0x52); // Keyboard Up Arrow

pub const NUMLOCK: Key = Key::from_keycode(0x53); // Keyboard Num Lock and Clear
pub const KPSLASH: Key = Key::from_keycode(0x54); // Keypad /
pub const KPASTERISK: Key = Key::from_keycode(0x55); // Keypad *
pub const KPMINUS: Key = Key::from_keycode(0x56); // Keypad -
pub const KPPLUS: Key = Key::from_keycode(0x57); // Keypad +
pub const KPENTER: Key = Key::from_keycode(0x58); // Keypad ENTER
pub const KP1: Key = Key::from_keycode(0x59); // Keypad 1 and End
pub const KP2: Key = Key::from_keycode(0x5a); // Keypad 2 and Down Arrow
pub const KP3: Key = Key::from_keycode(0x5b); // Keypad 3 and PageDn
pub const KP4: Key = Key::from_keycode(0x5c); // Keypad 4 and Left Arrow
pub const KP5: Key = Key::from_keycode(0x5d); // Keypad 5
pub const KP6: Key = Key::from_keycode(0x5e); // Keypad 6 and Right Arrow
pub const KP7: Key = Key::from_keycode(0x5f); // Keypad 7 and Home
pub const KP8: Key = Key::from_keycode(0x60); // Keypad 8 and Up Arrow
pub const KP9: Key = Key::from_keycode(0x61); // Keypad 9 and Page Up
pub const KP0: Key = Key::from_keycode(0x62); // Keypad 0 and Insert
pub const KPDOT: Key = Key::from_keycode(0x63); // Keypad . and Delete
pub const NONUS_BACKSLASH: Key = Key::from_keycode(0x64);

pub const COMPOSE: Key = Key::from_keycode(0x65); // Keyboard Application
pub const POWER: Key = Key::from_keycode(0x66); // Keyboard Power
pub const KPEQUAL: Key = Key::from_keycode(0x67); // Keypad =

pub const F13: Key = Key::from_keycode(0x68); // Keyboard F13
pub const F14: Key = Key::from_keycode(0x69); // Keyboard F14
pub const F15: Key = Key::from_keycode(0x6a); // Keyboard F15
pub const F16: Key = Key::from_keycode(0x6b); // Keyboard F16
pub const F17: Key = Key::from_keycode(0x6c); // Keyboard F17
pub const F18: Key = Key::from_keycode(0x6d); // Keyboard F18
pub const F19: Key = Key::from_keycode(0x6e); // Keyboard F19
pub const F20: Key = Key::from_keycode(0x6f); // Keyboard F20
pub const F21: Key = Key::from_keycode(0x70); // Keyboard F21
pub const F22: Key = Key::from_keycode(0x71); // Keyboard F22
pub const F23: Key = Key::from_keycode(0x72); // Keyboard F23
pub const F24: Key = Key::from_keycode(0x73); // Keyboard F24

pub const OPEN: Key = Key::from_keycode(0x74); // Keyboard Execute
pub const HELP: Key = Key::from_keycode(0x75); // Keyboard Help
pub const PROPS: Key = Key::from_keycode(0x76); // Keyboard Menu
pub const FRONT: Key = Key::from_keycode(0x77); // Keyboard Select
pub const STOP: Key = Key::from_keycode(0x78); // Keyboard Stop
pub const AGAIN: Key = Key::from_keycode(0x79); // Keyboard Again
pub const UNDO: Key = Key::from_keycode(0x7a); // Keyboard Undo
pub const CUT: Key = Key::from_keycode(0x7b); // Keyboard Cut
pub const COPY: Key = Key::from_keycode(0x7c); // Keyboard Copy
pub const PASTE: Key = Key::from_keycode(0x7d); // Keyboard Paste
pub const FIND: Key = Key::from_keycode(0x7e); // Keyboard Find
pub const MUTE: Key = Key::from_keycode(0x7f); // Keyboard Mute
pub const VOLUMEUP: Key = Key::from_keycode(0x80); // Keyboard Volume Up
pub const VOLUMEDOWN: Key = Key::from_keycode(0x81); // Keyboard Volume Down
// 0x82  Keycodeboard Locking Caps Lock
// 0x83  Keycodeboard Locking Num Lock
// 0x84  Keycodeboard Locking Scroll Lock
pub const KPCOMMA: Key = Key::from_keycode(0x85); // Keypad Comma
// 0x86  Keycodepad Equal Sign
pub const RO: Key = Key::from_keycode(0x87); // Keyboard International1
pub const KATAKANAHIRAGANA: Key = Key::from_keycode(0x88); // Keyboard International2
pub const YEN: Key = Key::from_keycode(0x89); // Keyboard International3
pub const HENKAN: Key = Key::from_keycode(0x8a); // Keyboard International4
pub const MUHENKAN: Key = Key::from_keycode(0x8b); // Keyboard International5
pub const KPJPCOMMA: Key = Key::from_keycode(0x8c); // Keyboard International6
// 0x8d  Keycodeboard International7
// 0x8e  Keycodeboard International8
// 0x8f  Keycodeboard International9
pub const HANGEUL: Key = Key::from_keycode(0x90); // Keyboard LANG1
pub const HANJA: Key = Key::from_keycode(0x91); // Keyboard LANG2
pub const KATAKANA: Key = Key::from_keycode(0x92); // Keyboard LANG3
pub const HIRAGANA: Key = Key::from_keycode(0x93); // Keyboard LANG4
pub const ZENKAKUHANKAKU: Key = Key::from_keycode(0x94); // Keyboard LANG5
// 0x95  Keycodeboard LANG6
// 0x96  Keycodeboard LANG7
// 0x97  Keycodeboard LANG8
// 0x98  Keycodeboard LANG9
// 0x99  Keycodeboard Alternate Erase
// 0x9a  Keycodeboard SysReq/Attention
// 0x9b  Keycodeboard Cancel
// 0x9c  Keycodeboard Clear
// 0x9d  Keycodeboard Prior
// 0x9e  Keycodeboard Return
// 0x9f  Keycodeboard Separator
// 0xa0  Keycodeboard Out
// 0xa1  Keycodeboard Oper
// 0xa2  Keycodeboard Clear/Again
// 0xa3  Keycodeboard CrSel/Props
// 0xa4  Keycodeboard ExSel

// 0xb0  Keycodepad 00
// 0xb1  Keycodepad 000
// 0xb2  Thousands Separator
// 0xb3  Decimal Separator
// 0xb4  Currency Unit
// 0xb5  Currency Sub-unit
pub const KPLEFTPAREN: Key = Key::from_keycode(0xb6); // Keypad (
pub const KPRIGHTPAREN: Key = Key::from_keycode(0xb7); // Keypad )
// 0xb8  Keycodepad {
// 0xb9  Keycodepad }
// 0xba  Keycodepad Tab
// 0xbb  Keycodepad Backspace
// 0xbc  Keycodepad A
// 0xbd  Keycodepad B
// 0xbe  Keycodepad C
// 0xbf  Keycodepad D
// 0xc0  Keycodepad E
// 0xc1  Keycodepad F
// 0xc2  Keycodepad XOR
// 0xc3  Keycodepad ^
// 0xc4  Keycodepad %
// 0xc5  Keycodepad <
// 0xc6  Keycodepad >
// 0xc7  Keycodepad &
// 0xc8  Keycodepad &&
// 0xc9  Keycodepad |
// 0xca  Keycodepad ||
// 0xcb  Keycodepad :
// 0xcc  Keycodepad #
// 0xcd  Keycodepad Space
// 0xce  Keycodepad @
// 0xcf  Keycodepad !
// 0xd0  Keycodepad Memory Store
// 0xd1  Keycodepad Memory Recall
// 0xd2  Keycodepad Memory Clear
// 0xd3  Keycodepad Memory Add
// 0xd4  Keycodepad Memory Subtract
// 0xd5  Keycodepad Memory Multiply
// 0xd6  Keycodepad Memory Divide
// 0xd7  Keycodepad +/-
// 0xd8  Keycodepad Clear
// 0xd9  Keycodepad Clear Entry
// 0xda  Keycodepad Binary
// 0xdb  Keycodepad Octal
// 0xdc  Keycodepad Decimal
// 0xdd  Keycodepad Hexadecimal

pub const LEFTCTRL: Key = Key::from_keycode(0xe0); // Keyboard Left Control
pub const LEFTSHIFT: Key = Key::from_keycode(0xe1); // Keyboard Left Shift
pub const LEFTALT: Key = Key::from_keycode(0xe2); // Keyboard Left Alt
pub const LEFTMETA: Key = Key::from_keycode(0xe3); // Keyboard Left GUI
pub const RIGHTCTRL: Key = Key::from_keycode(0xe4); // Keyboard Right Control
pub const RIGHTSHIFT: Key = Key::from_keycode(0xe5); // Keyboard Right Shift
pub const RIGHTALT: Key = Key::from_keycode(0xe6); // Keyboard Right Alt
pub const RIGHTMETA: Key = Key::from_keycode(0xe7); // Keyboard Right GUI

pub const MEDIA_PLAYPAUSE: Key = Key::from_keycode(0xe8);
pub const MEDIA_STOPCD: Key = Key::from_keycode(0xe9);
pub const MEDIA_PREVIOUSSONG: Key = Key::from_keycode(0xea);
pub const MEDIA_NEXTSONG: Key = Key::from_keycode(0xeb);
pub const MEDIA_EJECTCD: Key = Key::from_keycode(0xec);
pub const MEDIA_VOLUMEUP: Key = Key::from_keycode(0xed);
pub const MEDIA_VOLUMEDOWN: Key = Key::from_keycode(0xee);
pub const MEDIA_MUTE: Key = Key::from_keycode(0xef);
pub const MEDIA_WWW: Key = Key::from_keycode(0xf0);
pub const MEDIA_BACK: Key = Key::from_keycode(0xf1);
pub const MEDIA_FORWARD: Key = Key::from_keycode(0xf2);
pub const MEDIA_STOP: Key = Key::from_keycode(0xf3);
pub const MEDIA_FIND: Key = Key::from_keycode(0xf4);
pub const MEDIA_SCROLLUP: Key = Key::from_keycode(0xf5);
pub const MEDIA_SCROLLDOWN: Key = Key::from_keycode(0xf6);
pub const MEDIA_EDIT: Key = Key::from_keycode(0xf7);
pub const MEDIA_SLEEP: Key = Key::from_keycode(0xf8);
pub const MEDIA_COFFEE: Key = Key::from_keycode(0xf9);
pub const MEDIA_REFRESH: Key = Key::from_keycode(0xfa);
pub const MEDIA_CALC: Key = Key::from_keycode(0xfb);
