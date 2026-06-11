pub mod byte_string;
pub mod code_point_string;
pub mod unicode_string;

pub use byte_string::*;
pub use code_point_string::*;
pub use unicode_string::*;

/// Mirror of Symfony's `u()` / `b()` helper functions.
pub fn b(_string: &str) -> ByteString {
    todo!()
}

pub fn s(_string: &str) -> UnicodeString {
    todo!()
}
