#[allow(unused_macros)]
macro_rules! register_layers {
    ($board:ident, $layers:ident, [$($names:ident),*]) => {
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
    ($board:ident, $callbacks:ident, [$($names:ident),*]) => {
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
