pub mod byte_string;
pub mod code_point_string;
pub mod unicode_string;

pub use byte_string::*;
pub use code_point_string::*;
pub use unicode_string::*;

/// Mirror of Symfony's `b()` helper function.
pub fn b(string: &str) -> ByteString {
    ByteString::new(string)
}

/// Mirror of Symfony's `u()` helper function.
pub fn s(string: &str) -> UnicodeString {
    UnicodeString::new(string)
}
