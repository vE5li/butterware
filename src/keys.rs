use crate::flash::BondSlot;
use crate::interface::Keyboard;
use crate::led::{Animation, LedIndex};

pub struct Layer(pub usize);

pub enum SpecialAction {
    RemoveBond { bond_slot: BondSlot },
    SwitchAnimation { index: LedIndex, animation: Animation },
    Callback(<crate::Used as Keyboard>::Callbacks),
}

pub enum Mapping {
    Key(u8),
    Layer(usize),
    TapLayer(usize, u8),
    Special(SpecialAction),
}

impl Mapping {
    pub const fn layer(layer: Layer) -> Self {
        Self::Layer(layer.0)
    }

    pub const fn tap_layer(layer: Layer, mapping: Mapping) -> Self {
        // FIX: make sure that we cannot pass a tap_layer here
        Self::TapLayer(layer.0, mapping.keycode())
    }

    pub const fn keycode(&self) -> u8 {
        match self {
            Key(value) => *value,
            Mapping::Layer(..) => panic!("Mapping layer cannot be used as a regular key"),
            Mapping::TapLayer(_, value) => *value,
            Mapping::Special(..) => panic!("Special key cannot be used as a regular key"),
        }
    }
}

pub use self::Mapping::Key;

pub const MOD_LCTRL: Mapping = Key(0x01);
pub const MOD_LSHIFT: Mapping = Key(0x02);
pub const MOD_LALT: Mapping = Key(0x04);
pub const MOD_LMETA: Mapping = Key(0x08);
pub const MOD_RCTRL: Mapping = Key(0x10);
pub const MOD_RSHIFT: Mapping = Key(0x20);
pub const MOD_RALT: Mapping = Key(0x40);
pub const MOD_RMETA: Mapping = Key(0x80);

/**
 * Scan codes - last N slots in the HID report (usually 6).
 * 0x00 if no key pressed.
 *
 * If more than N keys are pressed, the HID reports
 * ERR_OVF in all slots to indicate this condition.
 */

pub const NONE: Mapping = Key(0x00); // No key pressed
pub const ERR_OVF: Mapping = Key(0x01); //  Keyboard Error Roll Over - used for all slots if too many keys are pressed ("Phantom key")
// 0x02 //  Keyboard POST Fail
// 0x03 //  Keyboard Error Undefined
pub const A: Mapping = Key(0x04); // Keyboard a and A
pub const B: Mapping = Key(0x05); // Keyboard b and B
pub const C: Mapping = Key(0x06); // Keyboard c and C
pub const D: Mapping = Key(0x07); // Keyboard d and D
pub const E: Mapping = Key(0x08); // Keyboard e and E
pub const F: Mapping = Key(0x09); // Keyboard f and F
pub const G: Mapping = Key(0x0a); // Keyboard g and G
pub const H: Mapping = Key(0x0b); // Keyboard h and H
pub const I: Mapping = Key(0x0c); // Keyboard i and I
pub const J: Mapping = Key(0x0d); // Keyboard j and J
pub const K: Mapping = Key(0x0e); // Keyboard k and K
pub const L: Mapping = Key(0x0f); // Keyboard l and L
pub const M: Mapping = Key(0x10); // Keyboard m and M
pub const N: Mapping = Key(0x11); // Keyboard n and N
pub const O: Mapping = Key(0x12); // Keyboard o and O
pub const P: Mapping = Key(0x13); // Keyboard p and P
pub const Q: Mapping = Key(0x14); // Keyboard q and Q
pub const R: Mapping = Key(0x15); // Keyboard r and R
pub const S: Mapping = Key(0x16); // Keyboard s and S
pub const T: Mapping = Key(0x17); // Keyboard t and T
pub const U: Mapping = Key(0x18); // Keyboard u and U
pub const V: Mapping = Key(0x19); // Keyboard v and V
pub const W: Mapping = Key(0x1a); // Keyboard w and W
pub const X: Mapping = Key(0x1b); // Keyboard x and X
pub const Y: Mapping = Key(0x1c); // Keyboard y and Y
pub const Z: Mapping = Key(0x1d); // Keyboard z and Z

pub const N1: Mapping = Key(0x1e); // Keyboard 1 and !
pub const N2: Mapping = Key(0x1f); // Keyboard 2 and @
pub const N3: Mapping = Key(0x20); // Keyboard 3 and #
pub const N4: Mapping = Key(0x21); // Keyboard 4 and $
pub const N5: Mapping = Key(0x22); // Keyboard 5 and %
pub const N6: Mapping = Key(0x23); // Keyboard 6 and ^
pub const N7: Mapping = Key(0x24); // Keyboard 7 and &
pub const N8: Mapping = Key(0x25); // Keyboard 8 and *
pub const N9: Mapping = Key(0x26); // Keyboard 9 and (
pub const N0: Mapping = Key(0x27); // Keyboard 0 and )

pub const ENTER: Mapping = Key(0x28); // Keyboard Return (ENTER)
pub const ESC: Mapping = Key(0x29); // Keyboard ESCAPE
pub const BACKSPACE: Mapping = Key(0x2a); // Keyboard DELETE (Backspace)
pub const TAB: Mapping = Key(0x2b); // Keyboard Tab
pub const SPACE: Mapping = Key(0x2c); // Keyboard Spacebar
pub const MINUS: Mapping = Key(0x2d); // Keyboard - and _
pub const EQUAL: Mapping = Key(0x2e); // Keyboard = and +
pub const LEFTBRACE: Mapping = Key(0x2f); // Keyboard [ and {
pub const RIGHTBRACE: Mapping = Key(0x30); // Keyboard ] and }
pub const BACKSLASH: Mapping = Key(0x31); // Keyboard \ and |
pub const HASHTILDE: Mapping = Key(0x32); // Keyboard Non-US # and ~
pub const SEMICOLON: Mapping = Key(0x33); // Keyboard ; and :
pub const APOSTROPHE: Mapping = Key(0x34); // Keyboard ' and "
pub const GRAVE: Mapping = Key(0x35); // Keyboard ` and ~
pub const COMMA: Mapping = Key(0x36); // Keyboard , and <
pub const DOT: Mapping = Key(0x37); // Keyboard . and >
pub const SLASH: Mapping = Key(0x38); // Keyboard / and ?
pub const CAPSLOCK: Mapping = Key(0x39); // Keyboard Caps Lock

pub const F1: Mapping = Key(0x3a); // Keyboard F1
pub const F2: Mapping = Key(0x3b); // Keyboard F2
pub const F3: Mapping = Key(0x3c); // Keyboard F3
pub const F4: Mapping = Key(0x3d); // Keyboard F4
pub const F5: Mapping = Key(0x3e); // Keyboard F5
pub const F6: Mapping = Key(0x3f); // Keyboard F6
pub const F7: Mapping = Key(0x40); // Keyboard F7
pub const F8: Mapping = Key(0x41); // Keyboard F8
pub const F9: Mapping = Key(0x42); // Keyboard F9
pub const F10: Mapping = Key(0x43); // Keyboard F10
pub const F11: Mapping = Key(0x44); // Keyboard F11
pub const F12: Mapping = Key(0x45); // Keyboard F12

pub const SYSRQ: Mapping = Key(0x46); // Keyboard Print Screen
pub const SCROLLLOCK: Mapping = Key(0x47); // Keyboard Scroll Lock
pub const PAUSE: Mapping = Key(0x48); // Keyboard Pause
pub const INSERT: Mapping = Key(0x49); // Keyboard Insert
pub const HOME: Mapping = Key(0x4a); // Keyboard Home
pub const PAGEUP: Mapping = Key(0x4b); // Keyboard Page Up
pub const DELETE: Mapping = Key(0x4c); // Keyboard Delete Forward
pub const END: Mapping = Key(0x4d); // Keyboard End
pub const PAGEDOWN: Mapping = Key(0x4e); // Keyboard Page Down
pub const RIGHT: Mapping = Key(0x4f); // Keyboard Right Arrow
pub const LEFT: Mapping = Key(0x50); // Keyboard Left Arrow
pub const DOWN: Mapping = Key(0x51); // Keyboard Down Arrow
pub const UP: Mapping = Key(0x52); // Keyboard Up Arrow

pub const NUMLOCK: Mapping = Key(0x53); // Keyboard Num Lock and Clear
pub const KPSLASH: Mapping = Key(0x54); // Keypad /
pub const KPASTERISK: Mapping = Key(0x55); // Keypad *
pub const KPMINUS: Mapping = Key(0x56); // Keypad -
pub const KPPLUS: Mapping = Key(0x57); // Keypad +
pub const KPENTER: Mapping = Key(0x58); // Keypad ENTER
pub const KP1: Mapping = Key(0x59); // Keypad 1 and End
pub const KP2: Mapping = Key(0x5a); // Keypad 2 and Down Arrow
pub const KP3: Mapping = Key(0x5b); // Keypad 3 and PageDn
pub const KP4: Mapping = Key(0x5c); // Keypad 4 and Left Arrow
pub const KP5: Mapping = Key(0x5d); // Keypad 5
pub const KP6: Mapping = Key(0x5e); // Keypad 6 and Right Arrow
pub const KP7: Mapping = Key(0x5f); // Keypad 7 and Home
pub const KP8: Mapping = Key(0x60); // Keypad 8 and Up Arrow
pub const KP9: Mapping = Key(0x61); // Keypad 9 and Page Up
pub const KP0: Mapping = Key(0x62); // Keypad 0 and Insert
pub const KPDOT: Mapping = Key(0x63); // Keypad . and Delete

pub const COMPOSE: Mapping = Key(0x65); // Keyboard Application
pub const POWER: Mapping = Key(0x66); // Keyboard Power
pub const KPEQUAL: Mapping = Key(0x67); // Keypad =

pub const F13: Mapping = Key(0x68); // Keyboard F13
pub const F14: Mapping = Key(0x69); // Keyboard F14
pub const F15: Mapping = Key(0x6a); // Keyboard F15
pub const F16: Mapping = Key(0x6b); // Keyboard F16
pub const F17: Mapping = Key(0x6c); // Keyboard F17
pub const F18: Mapping = Key(0x6d); // Keyboard F18
pub const F19: Mapping = Key(0x6e); // Keyboard F19
pub const F20: Mapping = Key(0x6f); // Keyboard F20
pub const F21: Mapping = Key(0x70); // Keyboard F21
pub const F22: Mapping = Key(0x71); // Keyboard F22
pub const F23: Mapping = Key(0x72); // Keyboard F23
pub const F24: Mapping = Key(0x73); // Keyboard F24

pub const OPEN: Mapping = Key(0x74); // Keyboard Execute
pub const HELP: Mapping = Key(0x75); // Keyboard Help
pub const PROPS: Mapping = Key(0x76); // Keyboard Menu
pub const FRONT: Mapping = Key(0x77); // Keyboard Select
pub const STOP: Mapping = Key(0x78); // Keyboard Stop
pub const AGAIN: Mapping = Key(0x79); // Keyboard Again
pub const UNDO: Mapping = Key(0x7a); // Keyboard Undo
pub const CUT: Mapping = Key(0x7b); // Keyboard Cut
pub const COPY: Mapping = Key(0x7c); // Keyboard Copy
pub const PASTE: Mapping = Key(0x7d); // Keyboard Paste
pub const FIND: Mapping = Key(0x7e); // Keyboard Find
pub const MUTE: Mapping = Key(0x7f); // Keyboard Mute
pub const VOLUMEUP: Mapping = Key(0x80); // Keyboard Volume Up
pub const VOLUMEDOWN: Mapping = Key(0x81); // Keyboard Volume Down
// 0x82  Keyboard Locking Caps Lock
// 0x83  Keyboard Locking Num Lock
// 0x84  Keyboard Locking Scroll Lock
pub const KPCOMMA: Mapping = Key(0x85); // Keypad Comma
// 0x86  Keypad Equal Sign
pub const RO: Mapping = Key(0x87); // Keyboard International1
pub const KATAKANAHIRAGANA: Mapping = Key(0x88); // Keyboard International2
pub const YEN: Mapping = Key(0x89); // Keyboard International3
pub const HENKAN: Mapping = Key(0x8a); // Keyboard International4
pub const MUHENKAN: Mapping = Key(0x8b); // Keyboard International5
pub const KPJPCOMMA: Mapping = Key(0x8c); // Keyboard International6
// 0x8d  Keyboard International7
// 0x8e  Keyboard International8
// 0x8f  Keyboard International9
pub const HANGEUL: Mapping = Key(0x90); // Keyboard LANG1
pub const HANJA: Mapping = Key(0x91); // Keyboard LANG2
pub const KATAKANA: Mapping = Key(0x92); // Keyboard LANG3
pub const HIRAGANA: Mapping = Key(0x93); // Keyboard LANG4
pub const ZENKAKUHANKAKU: Mapping = Key(0x94); // Keyboard LANG5
// 0x95  Keyboard LANG6
// 0x96  Keyboard LANG7
// 0x97  Keyboard LANG8
// 0x98  Keyboard LANG9
// 0x99  Keyboard Alternate Erase
// 0x9a  Keyboard SysReq/Attention
// 0x9b  Keyboard Cancel
// 0x9c  Keyboard Clear
// 0x9d  Keyboard Prior
// 0x9e  Keyboard Return
// 0x9f  Keyboard Separator
// 0xa0  Keyboard Out
// 0xa1  Keyboard Oper
// 0xa2  Keyboard Clear/Again
// 0xa3  Keyboard CrSel/Props
// 0xa4  Keyboard ExSel

// 0xb0  Keypad 00
// 0xb1  Keypad 000
// 0xb2  Thousands Separator
// 0xb3  Decimal Separator
// 0xb4  Currency Unit
// 0xb5  Currency Sub-unit
pub const KPLEFTPAREN: Mapping = Key(0xb6); // Keypad (
pub const KPRIGHTPAREN: Mapping = Key(0xb7); // Keypad )
// 0xb8  Keypad {
// 0xb9  Keypad }
// 0xba  Keypad Tab
// 0xbb  Keypad Backspace
// 0xbc  Keypad A
// 0xbd  Keypad B
// 0xbe  Keypad C
// 0xbf  Keypad D
// 0xc0  Keypad E
// 0xc1  Keypad F
// 0xc2  Keypad XOR
// 0xc3  Keypad ^
// 0xc4  Keypad %
// 0xc5  Keypad <
// 0xc6  Keypad >
// 0xc7  Keypad &
// 0xc8  Keypad &&
// 0xc9  Keypad |
// 0xca  Keypad ||
// 0xcb  Keypad :
// 0xcc  Keypad #
// 0xcd  Keypad Space
// 0xce  Keypad @
// 0xcf  Keypad !
// 0xd0  Keypad Memory Store
// 0xd1  Keypad Memory Recall
// 0xd2  Keypad Memory Clear
// 0xd3  Keypad Memory Add
// 0xd4  Keypad Memory Subtract
// 0xd5  Keypad Memory Multiply
// 0xd6  Keypad Memory Divide
// 0xd7  Keypad +/-
// 0xd8  Keypad Clear
// 0xd9  Keypad Clear Entry
// 0xda  Keypad Binary
// 0xdb  Keypad Octal
// 0xdc  Keypad Decimal
// 0xdd  Keypad Hexadecimal

pub const LEFTCTRL: Mapping = Key(0xe0); // Keyboard Left Control
pub const LEFTSHIFT: Mapping = Key(0xe1); // Keyboard Left Shift
pub const LEFTALT: Mapping = Key(0xe2); // Keyboard Left Alt
pub const LEFTMETA: Mapping = Key(0xe3); // Keyboard Left GUI
pub const RIGHTCTRL: Mapping = Key(0xe4); // Keyboard Right Control
pub const RIGHTSHIFT: Mapping = Key(0xe5); // Keyboard Right Shift
pub const RIGHTALT: Mapping = Key(0xe6); // Keyboard Right Alt
pub const RIGHTMETA: Mapping = Key(0xe7); // Keyboard Right GUI

pub const MEDIA_PLAYPAUSE: Mapping = Key(0xe8);
pub const MEDIA_STOPCD: Mapping = Key(0xe9);
pub const MEDIA_PREVIOUSSONG: Mapping = Key(0xea);
pub const MEDIA_NEXTSONG: Mapping = Key(0xeb);
pub const MEDIA_EJECTCD: Mapping = Key(0xec);
pub const MEDIA_VOLUMEUP: Mapping = Key(0xed);
pub const MEDIA_VOLUMEDOWN: Mapping = Key(0xee);
pub const MEDIA_MUTE: Mapping = Key(0xef);
pub const MEDIA_WWW: Mapping = Key(0xf0);
pub const MEDIA_BACK: Mapping = Key(0xf1);
pub const MEDIA_FORWARD: Mapping = Key(0xf2);
pub const MEDIA_STOP: Mapping = Key(0xf3);
pub const MEDIA_FIND: Mapping = Key(0xf4);
pub const MEDIA_SCROLLUP: Mapping = Key(0xf5);
pub const MEDIA_SCROLLDOWN: Mapping = Key(0xf6);
pub const MEDIA_EDIT: Mapping = Key(0xf7);
pub const MEDIA_SLEEP: Mapping = Key(0xf8);
pub const MEDIA_COFFEE: Mapping = Key(0xf9);
pub const MEDIA_REFRESH: Mapping = Key(0xfa);
pub const MEDIA_CALC: Mapping = Key(0xfb);
