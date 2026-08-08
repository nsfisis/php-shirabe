use crate::PhpMixed;
use indexmap::IndexMap;

pub const OPENSSL_ALGO_SHA384: i64 = 9;
pub const OPENSSL_VERSION_NUMBER: i64 = 0;
pub const OPENSSL_VERSION_TEXT: &str = "";

pub fn openssl_x509_parse(
    _certificate: &str,
    _short_names: bool,
) -> Option<IndexMap<String, PhpMixed>> {
    todo!()
}
