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

        impl $callbacks {
            pub const fn mapping(self) -> Mapping {
                Mapping::Special(SpecialAction::Callback(self))
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
