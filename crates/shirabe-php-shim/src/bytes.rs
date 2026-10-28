//! Conversions between PHP strings, which are byte strings, and Rust's text and path types.

use bstr::BString;
use std::borrow::Cow;
use std::ffi::OsStr;
use std::path::Path;

/// Decodes `bytes` as UTF-8, replacing invalid sequences with U+FFFD. PHP never does this: it is
/// the single conversion left at the boundary between code that holds PHP strings as bytes and code
/// that still holds them as `String`/`&str`.
// TODO(bytes): remove once every caller takes the bytes as they are.
pub fn legacy_lossy(bytes: &(impl AsRef<[u8]> + ?Sized)) -> Cow<'_, str> {
    String::from_utf8_lossy(bytes.as_ref())
}

/// The path PHP's filesystem functions open for the byte string `bytes`.
pub fn path_from_bytes(bytes: &(impl AsRef<[u8]> + ?Sized)) -> &Path {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Path::new(OsStr::from_bytes(bytes.as_ref()))
    }
    #[cfg(windows)]
    {
        // TODO(windows): PHP converts the bytes from UTF-8 to UTF-16 before calling the wide Win32
        // API.
        todo!()
    }
}

/// The byte string PHP holds for `path`.
pub fn path_to_bytes(path: &(impl AsRef<Path> + ?Sized)) -> &[u8] {
    path.as_ref().as_os_str().as_encoded_bytes()
}

/// A value that PHP's `.` operator (and string interpolation) appends as a string.
pub trait PhpConcat {
    fn php_concat_to(&self, out: &mut BString);
}

impl PhpConcat for [u8] {
    fn php_concat_to(&self, out: &mut BString) {
        out.extend_from_slice(self);
    }
}

impl PhpConcat for Vec<u8> {
    fn php_concat_to(&self, out: &mut BString) {
        out.extend_from_slice(self);
    }
}

impl PhpConcat for bstr::BStr {
    fn php_concat_to(&self, out: &mut BString) {
        out.extend_from_slice(self);
    }
}

impl PhpConcat for BString {
    fn php_concat_to(&self, out: &mut BString) {
        out.extend_from_slice(self);
    }
}

impl PhpConcat for str {
    fn php_concat_to(&self, out: &mut BString) {
        out.extend_from_slice(self.as_bytes());
    }
}

impl PhpConcat for String {
    fn php_concat_to(&self, out: &mut BString) {
        out.extend_from_slice(self.as_bytes());
    }
}

impl PhpConcat for Path {
    fn php_concat_to(&self, out: &mut BString) {
        out.extend_from_slice(path_to_bytes(self));
    }
}

impl PhpConcat for std::path::PathBuf {
    fn php_concat_to(&self, out: &mut BString) {
        out.extend_from_slice(path_to_bytes(self));
    }
}

impl PhpConcat for i64 {
    fn php_concat_to(&self, out: &mut BString) {
        out.extend_from_slice(self.to_string().as_bytes());
    }
}

impl<T: PhpConcat + ?Sized> PhpConcat for &T {
    fn php_concat_to(&self, out: &mut BString) {
        (**self).php_concat_to(out);
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
        $($crate::PhpConcat::php_concat_to(&$operand, &mut out);)*
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
    fn path_round_trip() {
        let bytes = b"dir/\x80file";
        assert_eq!(path_to_bytes(path_from_bytes(bytes)), bytes);
    }
}
