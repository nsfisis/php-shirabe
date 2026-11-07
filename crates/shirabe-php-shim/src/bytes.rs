//! Conversions between PHP strings, which are byte strings, and Rust's text and path types.

use bstr::{BStr, BString};
use std::ffi::OsStr;
use std::path::Path;

/// A PHP string constant: `const NAME: &'static BStr = php_str(b"value");`. `BStr::new` is not a
/// `const fn`.
pub const fn php_str(bytes: &[u8]) -> &BStr {
    // SAFETY: `BStr` is a `#[repr(transparent)]` wrapper around `[u8]`.
    unsafe { &*(bytes as *const [u8] as *const BStr) }
}

/// The text a Rust API produces through `Display`, such as the message of an `std::io::Error`, as
/// the PHP string that holds it from there on.
pub fn display_to_bytes(value: impl std::fmt::Display) -> BString {
    use std::io::Write as _;
    let mut out = Vec::new();
    write!(out, "{value}").expect("writing to a Vec cannot fail");
    BString::from(out)
}

/// `bytes` as UTF-8 text, or `None` if they are not valid UTF-8. Only for a Rust API that has no
/// byte form, and for the shim of a PHP function that itself requires UTF-8, which reproduces how
/// that function fails.
#[expect(
    clippy::disallowed_methods,
    reason = "the one conversion from bytes to text"
)]
#[expect(
    clippy::disallowed_types,
    reason = "the one conversion from bytes to text"
)]
pub fn utf8_text(bytes: &(impl AsRef<[u8]> + ?Sized)) -> Option<&str> {
    std::str::from_utf8(bytes.as_ref()).ok()
}

/// The OS string PHP passes to the system (a path, a command line, an environment variable) for
/// the byte string `bytes`.
pub fn os_str_from_bytes(bytes: &(impl AsRef<[u8]> + ?Sized)) -> &OsStr {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        OsStr::from_bytes(bytes.as_ref())
    }
    #[cfg(windows)]
    {
        // TODO(windows): PHP converts the bytes from UTF-8 to UTF-16 before calling the wide Win32
        // API.
        todo!()
    }
}

/// The path PHP's filesystem functions open for the byte string `bytes`.
pub fn path_from_bytes(bytes: &(impl AsRef<[u8]> + ?Sized)) -> &Path {
    Path::new(os_str_from_bytes(bytes))
}

/// The byte string PHP holds for `path`.
pub fn path_to_bytes(path: &(impl AsRef<Path> + ?Sized)) -> &[u8] {
    path.as_ref().as_os_str().as_encoded_bytes()
}

/// PHP's `(string)` conversion: `fmt` appends the bytes PHP produces for the value, as
/// `Display::fmt` writes text into a formatter. PHP's `.` operator and string interpolation
/// convert their operands this way.
pub trait PhpDisplay {
    fn fmt(&self, out: &mut BString);
}

/// `ToString` for [`PhpDisplay`]: the PHP string of a value, as `(string) $value` gives it.
pub trait ToPhpString {
    fn to_php_string(&self) -> BString;
}

impl<T: PhpDisplay + ?Sized> ToPhpString for T {
    fn to_php_string(&self) -> BString {
        let mut out = BString::default();
        self.fmt(&mut out);
        out
    }
}

impl PhpDisplay for [u8] {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(self);
    }
}

impl PhpDisplay for Vec<u8> {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(self);
    }
}

impl PhpDisplay for bstr::BStr {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(self);
    }
}

impl PhpDisplay for BString {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(self);
    }
}

#[expect(clippy::disallowed_types, reason = "a string literal")]
impl PhpDisplay for str {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(self.as_bytes());
    }
}

impl PhpDisplay for Path {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(path_to_bytes(self));
    }
}

impl PhpDisplay for std::path::PathBuf {
    fn fmt(&self, out: &mut BString) {
        out.extend_from_slice(path_to_bytes(self));
    }
}

macro_rules! impl_php_display_for_integers {
    ($($ty:ty),* $(,)?) => {
        $(
            impl PhpDisplay for $ty {
                fn fmt(&self, out: &mut BString) {
                    use std::io::Write as _;
                    write!(out, "{self}").expect("writing to a Vec cannot fail");
                }
            }
        )*
    };
}

// `u8` is left out: a Rust `u8` is usually a byte of a string, not a number PHP would print.
impl_php_display_for_integers!(i8, i16, i32, i64, isize, u16, u32, u64, usize);

impl<T: PhpDisplay + ?Sized> PhpDisplay for &T {
    fn fmt(&self, out: &mut BString) {
        (**self).fmt(out);
    }
}

#[doc(hidden)]
pub fn __bconcat_buffer() -> BString {
    BString::default()
}

/// PHP's string concatenation: `bconcat!("Package ", $name, " not found")` is
/// `"Package " . $name . " not found"`, and evaluates to a `BString` holding the bytes of every
/// operand as they are.
#[macro_export]
macro_rules! bconcat {
    ($($operand:expr),* $(,)?) => {{
        let mut out = $crate::__bconcat_buffer();
        $($crate::PhpDisplay::fmt(&$operand, &mut out);)*
        out
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bconcat_keeps_invalid_utf8() {
        let name = BString::from(b"\xffvendor".to_vec());
        let path = path_from_bytes(b"/tmp/\xfe");
        let out = bconcat!("a ", name, b" ".as_slice(), path, " ", 42i64);
        assert_eq!(out, BString::from(b"a \xffvendor /tmp/\xfe 42".to_vec()));
    }

    #[test]
    fn bconcat_writes_integers_in_decimal() {
        let out = bconcat!(-1i8, " ", 2u16, " ", 3u32, " ", 4usize);
        assert_eq!(out, BString::from("-1 2 3 4"));
    }

    #[test]
    fn php_str_holds_the_bytes() {
        const NAME: &BStr = php_str(b"a\xffb");
        assert_eq!(NAME, BStr::new(b"a\xffb"));
    }

    #[test]
    fn display_to_bytes_keeps_the_text() {
        assert_eq!(display_to_bytes(1.5f64), BString::from("1.5"));
    }

    #[test]
    fn path_round_trip() {
        let bytes = b"dir/\x80file";
        assert_eq!(path_to_bytes(path_from_bytes(bytes)), bytes);
    }
}
