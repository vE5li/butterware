#[allow(unused_macros)]
macro_rules! register_layers {
    ($board:ident, $layers:ident, [$($names:ident),* $(,)?]) => {
        pub struct $layers;

        impl $layers {
            #[allow(unused)]
            $(pub const $names: Layer = Layer(${index()});)*
            pub const LAYER_LOOKUP: &'static [&'static [Mapping; <$board as crate::interface::KeyboardExtension>::KEYS_TOTAL]] = &[$(&$board::$names),*];
        }
    };
}

#[allow(unused_macros)]
macro_rules! register_callbacks {
    ($board:ident, $callbacks:ident, [$($names:ident),* $(,)?]) => {
        #[derive(Clone, Copy, Debug, defmt::Format, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $callbacks {
            $($names),*
        }

        impl const crate::keys::IntoTapAction for $callbacks {
            fn into_tap_action(self) -> TapAction {
                TapAction::Special(SpecialAction::Callback(self))
            }
        }

        impl const crate::keys::IntoMapping for $callbacks {
            fn into_mapping(self) -> Mapping {
                Mapping::Tap(self.into_tap_action())
            }
        }
    };
}

#[allow(unused_macros)]
macro_rules! register_events {
    ($board:ident, $events:ident, [$($names:ident),* $(,)?]) => {
        #[repr(C)]
        #[derive(Clone, Copy, Debug, defmt::Format, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $events {
            $($names),*
        }

        impl nrf_softdevice::ble::FixedGattValue for $events {
            const SIZE: usize = core::mem::size_of::<$events>();

            fn from_gatt(data: &[u8]) -> Self {
                let mut buffer = [0; Self::SIZE];
                buffer.copy_from_slice(data);
                unsafe { core::mem::transmute::<&[u8; Self::SIZE], &$events>(&buffer).clone() }
            }

            fn to_gatt(&self) -> &[u8] {
                unsafe { core::mem::transmute::<&$events, &[u8; Self::SIZE]>(self) }
            }
        }

    };
}

#[allow(unused_macros)]
macro_rules! register_leds {
    ($board:ident, $leds:ident, [$($names:ident: $types:ty,)* $(,)?]) => {

        #[repr(C)]
        #[derive(Clone, Copy, Debug, defmt::Format, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $leds {
            $($names),*
        }

        #[allow(non_snake_case)]
        pub struct GeneratedLedStorage {
            $($names: $types),*
        }

        impl crate::led::LedProvider for $leds {
            type Collection = GeneratedLedStorage;
        }

        impl crate::led::LedCollection for GeneratedLedStorage {
            type Index = $leds;

            fn set_animation(&mut self, index: Self::Index, animtaion: Animation) {
                match index {
                    $(Self::Index::$names => crate::led::LedDriver::set_animation(&mut self.$names, animtaion)),*
                }
            }

            async fn update(&mut self, elapsed_time: f32) {
                $(crate::led::LedDriver::update(&mut self.$names, elapsed_time).await;)*
            }
        }

        macro_rules! initialize_leds {
            ($$($$field:ident: $$value:expr,)*) => {
                GeneratedLedStorage {
                    $$($$field: $$value),*
                }
            }
        }
    };
}
