use embassy_nrf::gpio::Pin;
use embassy_nrf::peripherals::{SPI2, SPI3, TWISPI1};
use embassy_nrf::Peripherals;

use crate::flash::{get_settings, store_board_flash, FlashToken};
use crate::hardware::PeripheralConfig;
use crate::interface::{Keyboard, KeyboardExtension, Scannable};
use crate::keys::german::*;
use crate::keys::*;
#[cfg(feature = "lighting")]
use crate::led::{set_animation, Animation, Led, Speed, Ws2812bDriver};
use crate::power::{set_power_state, PowerState};
use crate::side::Side;
use crate::split::trigger_event;

#[derive(Clone, Copy, defmt::Format)]
pub struct PersistentData {
    keys_animation: usize,
    wings_animation: usize,
    status_animation: usize,
    #[cfg(feature = "lighting")]
    lighting_state: PowerState,
}

pub struct Butterboard {
    persistent_data: PersistentData,
}

register_layers!(Butterboard, Layers, [BASE, NUMBERS, SYMBOLS, SPECIAL]);

#[cfg(feature = "lighting")]
register_callbacks!(Butterboard, Callbacks, [
    NextKeysAnimation,
    NextWingsAnimation,
    NextStatusAnimation,
    ToggleLighting,
    SyncAnimations,
]);

#[cfg(feature = "lighting")]
register_leds!(Butterboard, Leds, [
    Keys: Ws2812bDriver<19, SPI3>,
    Wings: Ws2812bDriver<57, TWISPI1>,
    Status: Ws2812bDriver<18, SPI2>,
]);

#[cfg(feature = "lighting")]
register_events!(Butterboard, Events, [SyncAnimations]);

#[rustfmt::skip]
macro_rules! new_layer {
    (
        $K0:expr,  $K1:expr,  $K2:expr,  $K3:expr,  $K4:expr,  $K5:expr,  $K6:expr,  $K7:expr,  $K8:expr,  $K9:expr,
        $K10:expr, $K11:expr, $K12:expr, $K13:expr, $K14:expr, $K15:expr, $K16:expr, $K17:expr, $K18:expr, $K19:expr,
        $K20:expr, $K21:expr, $K22:expr, $K23:expr, $K24:expr, $K25:expr, $K26:expr, $K27:expr, $K28:expr, $K29:expr,
        $K30:expr, $K31:expr, $K32:expr, $K33:expr, $K34:expr, $K35:expr, $K36:expr, $K37:expr, $K38:expr, $K39:expr,
    ) => {
        [
            $K9.into_mapping(), $K19.into_mapping(), $K29.into_mapping(), $K39.into_mapping(),
            $K8.into_mapping(), $K18.into_mapping(), $K28.into_mapping(), $K38.into_mapping(),
            $K7.into_mapping(), $K17.into_mapping(), $K27.into_mapping(), $K37.into_mapping(),
            $K6.into_mapping(), $K16.into_mapping(), $K26.into_mapping(), $K36.into_mapping(),
            $K5.into_mapping(), $K15.into_mapping(), $K25.into_mapping(), $K35.into_mapping(),
            $K4.into_mapping(), $K14.into_mapping(), $K24.into_mapping(), $K34.into_mapping(),
            $K3.into_mapping(), $K13.into_mapping(), $K23.into_mapping(), $K33.into_mapping(),
            $K2.into_mapping(), $K12.into_mapping(), $K22.into_mapping(), $K32.into_mapping(),
            $K1.into_mapping(), $K11.into_mapping(), $K21.into_mapping(), $K31.into_mapping(),
            $K0.into_mapping(), $K10.into_mapping(), $K20.into_mapping(), $K30.into_mapping(),
        ]
    };
}

impl Butterboard {
    #[rustfmt::skip]
    const BASE: [Mapping; <Butterboard as KeyboardExtension>::KEYS_TOTAL] = new_layer![
        DE_Q, DE_W, DE_F, DE_P, DE_B, DE_J, DE_L, DE_U, DE_Y, DE_SS,
        DE_A, DE_R, DE_S, DE_T, DE_G, DE_M, DE_N, DE_E, DE_I, DE_O,
        DE_Z, DE_X, DE_C, DE_D, DE_V, DE_K, DE_H, DE_UDIA, DE_ODIA, DE_ADIA,
        NONE, hold_tap(MOD_LCTRL, ESC), hold_tap(Layers::SPECIAL, SPACE), MOD_LMETA, MOD_LALT, MOD_LCTRL, Layers::NUMBERS, hold_tap(Layers::SYMBOLS, BACKSPACE), hold_tap(MOD_LSHIFT, ENTER), NONE,
    ];
    #[rustfmt::skip]
    const NUMBERS: [Mapping; <Butterboard as KeyboardExtension>::KEYS_TOTAL] = new_layer![
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10,
        N1, N2, N3, N4, N5, N6, N7, N8, N9, N0,
        NONE, NONE, NONE, NONE, F11, F12, NONE, NONE, NONE, NONE,
        NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE,
    ];
    #[rustfmt::skip]
    const SPECIAL: [Mapping; <Butterboard as KeyboardExtension>::KEYS_TOTAL] = new_layer![
        Callbacks::SyncAnimations, Callbacks::ToggleLighting, NONE, NONE, HOME, END, INSERT, UP, NONE, PAGEUP,
        Callbacks::NextKeysAnimation, Callbacks::NextWingsAnimation, Callbacks::NextStatusAnimation, NONE, TAB, BACKSPACE, LEFT, DOWN, RIGHT, PAGEDOWN,
        NONE, NONE, NONE, NONE, NONE, DELETE, NONE, NONE, NONE, NONE,
        NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE, NONE,
    ];
    #[rustfmt::skip]
    const SYMBOLS: [Mapping; <Butterboard as KeyboardExtension>::KEYS_TOTAL] = new_layer![
        DE_EXLM, DE_DQUO, DE_QUES, DE_AT,   DE_DLR,  DE_AMPR, DE_EQL,  DE_SLSH, DE_QUOT, DE_ASTR,
        DE_COLN, DE_LABK, DE_LCBR, DE_LBRC, DE_LPRN, DE_RPRN, DE_RBRC, DE_RCBR, DE_RABK, DE_SCLN,
        DE_BSLS, DE_PERC, DE_PIPE, DE_HASH, DE_COMM, DE_DOT,  DE_MINS, DE_TILD, DE_UNDS, DE_PLUS,
        NONE, DE_GRV, DE_CIRC, DE_DEG, DE_EURO, NONE, NONE, NONE, NONE, NONE,
    ];
}

#[cfg(feature = "lighting")]
impl Butterboard {
    const ANIMATIONS: &[Animation] = &[
        Animation::Rainbow {
            hue: 0.0,
            speed: Speed(15.0),
        },
        Animation::Rainbow {
            hue: 0.0,
            speed: Speed(30.0),
        },
        Animation::Rainbow {
            hue: 0.0,
            speed: Speed(60.0),
        },
        Animation::Pulsate {
            color: Led::rgb(1.0, 0.0, 0.0),
            speed: Speed(4.0),
            offset: 0.0,
        },
        Animation::Pulsate {
            color: Led::rgb(0.0, 1.0, 0.0),
            speed: Speed(4.0),
            offset: 0.0,
        },
        Animation::Pulsate {
            color: Led::rgb(0.0, 0.0, 1.0),
            speed: Speed(4.0),
            offset: 0.0,
        },
    ];

    async fn next_keys_animation(&mut self) {
        // Go to next animation.
        self.persistent_data.keys_animation = (self.persistent_data.keys_animation + 1) % Self::ANIMATIONS.len();
        let animation = Self::ANIMATIONS[self.persistent_data.keys_animation];

        // Set the animation for both sides.
        set_animation(Side::Both, Leds::Keys, animation).await;

        // Store persistent data on both sides.
        store_board_flash(Side::Both, self.persistent_data).await;
    }

    async fn next_wings_animation(&mut self) {
        // Go to next animation.
        self.persistent_data.wings_animation = (self.persistent_data.wings_animation + 1) % Self::ANIMATIONS.len();
        let animation = Self::ANIMATIONS[self.persistent_data.wings_animation];

        // Set the animation for both sides.
        set_animation(Side::Both, Leds::Wings, animation).await;

        // Store persistent data on both sides.
        store_board_flash(Side::Both, self.persistent_data).await;
    }

    async fn next_status_animation(&mut self) {
        // Go to next animation.
        self.persistent_data.status_animation = (self.persistent_data.status_animation + 1) % Self::ANIMATIONS.len();
        let animation = Self::ANIMATIONS[self.persistent_data.status_animation];

        // Set the animation for both sides.
        set_animation(Side::Both, Leds::Status, animation).await;

        // Store persistent data on both sides.
        store_board_flash(Side::Both, self.persistent_data).await;
    }

    async fn toggle_lighting(&mut self) {
        self.persistent_data.lighting_state = !self.persistent_data.lighting_state;

        // TODO: also turn lighting off so we can halt the lighting task
        set_power_state(Side::Both, self.persistent_data.lighting_state).await;

        // Store persistent data on both sides.
        store_board_flash(Side::Both, self.persistent_data).await;
    }
}

impl Scannable for Butterboard {
    const COLUMNS: usize = 5;
    const ROWS: usize = 4;
}

impl Keyboard for Butterboard {
    type BoardFlash = PersistentData;
    #[cfg(feature = "lighting")]
    type Callbacks = Callbacks;
    #[cfg(feature = "lighting")]
    type Events = Events;
    #[cfg(feature = "lighting")]
    type Leds = Leds;

    const DEVICE_NAME: &'static [u8] = b"Butterboard";
    const LAYER_LOOKUP: &'static [&'static [Mapping; Self::KEYS_TOTAL]] = Layers::LAYER_LOOKUP;
    #[cfg(feature = "lighting")]
    const STATUS_LEDS: Leds = Leds::Status;

    fn new(flash_token: FlashToken) -> Self {
        // Get the flash settings and extract the custom data stored for this board.
        let persistent_data = get_settings(flash_token).board_flash;

        Self { persistent_data }
    }

    #[cfg(feature = "lighting")]
    async fn callback(&mut self, callback: Callbacks) {
        match callback {
            Callbacks::NextKeysAnimation => self.next_keys_animation().await,
            Callbacks::NextWingsAnimation => self.next_wings_animation().await,
            Callbacks::NextStatusAnimation => self.next_status_animation().await,
            Callbacks::ToggleLighting => self.toggle_lighting().await,
            Callbacks::SyncAnimations => trigger_event(Side::Both, Events::SyncAnimations).await,
        }
    }

    #[cfg(feature = "lighting")]
    async fn event(&mut self, event: Events) {
        match event {
            Events::SyncAnimations => {
                #[cfg(feature = "lighting")]
                self.toggle_lighting().await;
            }
        }
    }

    async fn initialize_peripherals(&mut self, peripherals: Peripherals) -> PeripheralConfig<{ Self::COLUMNS }, { Self::ROWS }> {
        #[cfg(feature = "left")]
        let columns = [
            peripherals.P0_31.degrade(),
            peripherals.P0_29.degrade(),
            peripherals.P0_02.degrade(),
            peripherals.P1_15.degrade(),
            peripherals.P1_13.degrade(),
        ];

        #[cfg(feature = "right")]
        let columns = [
            peripherals.P1_04.degrade(),
            peripherals.P0_11.degrade(),
            peripherals.P1_00.degrade(),
            peripherals.P0_24.degrade(),
            peripherals.P0_22.degrade(),
        ];

        #[cfg(feature = "left")]
        let rows = [
            peripherals.P0_22.degrade(),
            peripherals.P0_24.degrade(),
            peripherals.P1_00.degrade(),
            peripherals.P0_11.degrade(),
        ];

        #[cfg(feature = "right")]
        let rows = [
            peripherals.P0_31.degrade(),
            peripherals.P0_29.degrade(),
            peripherals.P0_02.degrade(),
            peripherals.P1_15.degrade(),
        ];

        PeripheralConfig {
            columns,
            rows,
            #[cfg(feature = "lighting")]
            leds: initialize_leds! {
                Keys: Ws2812bDriver::new(peripherals.P0_06.degrade(), peripherals.P0_08.degrade(), peripherals.SPI3),
                Wings: Ws2812bDriver::new(peripherals.P0_20.degrade(), peripherals.P0_09.degrade(), peripherals.TWISPI1),
                Status: Ws2812bDriver::new(peripherals.P0_17.degrade(), peripherals.P0_10.degrade(), peripherals.SPI2),
            },
            power_pin: Some(peripherals.P0_13.degrade()),
        }
    }

    #[cfg(feature = "lighting")]
    async fn post_sides_connected(&mut self, _is_master: bool) {
        let keys_animation = Self::ANIMATIONS[self.persistent_data.keys_animation];
        let wings_animation = Self::ANIMATIONS[self.persistent_data.wings_animation];
        let status_animation = Self::ANIMATIONS[self.persistent_data.status_animation];

        // Restore animations for each side individually. This allows both side to run
        // different animations.
        set_animation(Side::This, Leds::Keys, keys_animation).await;
        set_animation(Side::This, Leds::Wings, wings_animation).await;
        set_animation(Side::This, Leds::Status, status_animation).await;

        // Restore power state.
        set_power_state(Side::This, self.persistent_data.lighting_state).await;
    }

    #[cfg(not(feature = "lighting"))]
    async fn post_sides_connected(&mut self, _is_master: bool) {
        set_power_state(Side::This, PowerState::Off).await;
    }

    #[cfg(feature = "lighting")]
    async fn sides_disconnected(&mut self) {
        set_power_state(Side::This, PowerState::On).await;
    }
}
