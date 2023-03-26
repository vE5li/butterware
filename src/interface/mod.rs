mod keyboard;
#[macro_use]
mod macros;

pub use self::keyboard::{Keyboard, KeyboardExtension, Scannable};

pub trait UnwrapInfelliable {
    type Output;

    fn unwrap_infelliable(self) -> Self::Output;
}

impl<T, E> UnwrapInfelliable for Result<T, E> {
    type Output = T;

    fn unwrap_infelliable(self) -> Self::Output {
        match self {
            Ok(value) => value,
            Err(..) => unreachable!(),
        }
    }
}

impl<T> UnwrapInfelliable for Option<T> {
    type Output = T;

    fn unwrap_infelliable(self) -> Self::Output {
        match self {
            Some(value) => value,
            None => unreachable!(),
        }
    }
}
