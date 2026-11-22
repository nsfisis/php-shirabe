//! ref: composer/vendor/symfony/string/Resources/functions.php sha1:dbdfca4b252edf17a70e090f30e84c49d330f3db

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

/// Mirror of Symfony's `s()` helper function.
pub fn s(string: impl AsRef<[u8]>) -> UnicodeString {
    UnicodeString::new(string)
}
