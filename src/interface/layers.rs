macro_rules! register_layers {
    ($board:ident, $layers:ident, [$($names:ident),*]) => {
        struct $layers;

        impl $layers {
            $(pub const $names: Layer = Layer(${index()});)*
            pub const LAYER_LOOKUP: &'static [&'static [Mapping; <$board as Scannable>::COLUMNS * <$board as Scannable>::ROWS]] = &[$(&$board::$names),*];
        }
    };
}
