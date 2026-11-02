#![deny(clippy::disallowed_types, clippy::disallowed_macros)]

pub mod byte_string;
pub mod code_point_string;
pub mod exception;
pub mod unicode_string;

pub use byte_string::*;
pub use code_point_string::*;
pub use unicode_string::*;

/// Mirror of Symfony's `b()` helper function.
pub fn b(string: impl AsRef<[u8]>) -> ByteString {
    ByteString::new(string)
}

/// Mirror of Symfony's `u()` helper function.
pub fn s(string: impl AsRef<[u8]>) -> UnicodeString {
    UnicodeString::new(string)
}
