#[allow(dead_code)]
#[derive(Clone, Copy, Debug, defmt::Format, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    This,
    Other,
    Left,
    Right,
    Both,
}

impl Side {
    pub const fn includes_this(&self) -> bool {
        match self {
            Side::This => true,
            Side::Other => false,
            Side::Left => cfg!(feature = "left"),
            Side::Right => cfg!(feature = "right"),
            Side::Both => true,
        }
    }

    pub const fn includes_other(&self) -> bool {
        match self {
            Side::This => false,
            Side::Other => true,
            Side::Left => cfg!(feature = "right"),
            Side::Right => cfg!(feature = "left"),
            Side::Both => true,
        }
    }
}
