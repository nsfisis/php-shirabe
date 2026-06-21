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

pub fn openssl_get_publickey(_certificate: &str) -> Option<PhpMixed> {
    todo!()
}

pub fn openssl_pkey_get_details(_key: PhpMixed) -> Option<IndexMap<String, PhpMixed>> {
    todo!()
}

pub fn openssl_verify(
    _data: &str,
    _signature: &[u8],
    _pub_key_id: PhpMixed,
    _algorithm: PhpMixed,
) -> i64 {
    todo!()
}

pub fn openssl_pkey_get_public(_public_key: &str) -> PhpMixed {
    todo!()
}

pub fn openssl_get_md_methods() -> Vec<String> {
    todo!()
}

pub fn openssl_free_key(_key: PhpMixed) {
    todo!()
}
