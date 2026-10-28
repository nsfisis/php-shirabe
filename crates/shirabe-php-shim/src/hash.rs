use crate::bin2hex;
use bstr::BString;
use sha1::Digest as _;

pub fn hash(algo: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> BString {
    bin2hex(calculate_hash(algo, data.as_ref()))
}

pub fn hash_raw(algo: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> Vec<u8> {
    calculate_hash(algo, data.as_ref())
}

pub fn hash_file(algo: impl AsRef<[u8]>, filename: impl AsRef<std::path::Path>) -> Option<BString> {
    let data = std::fs::read(filename).ok()?;
    Some(bin2hex(calculate_hash(algo, &data)))
}

pub(crate) fn calculate_hash(algo: impl AsRef<[u8]>, data: &[u8]) -> Vec<u8> {
    match algo.as_ref() {
        b"md5" => md5::compute(data).0.to_vec(),
        b"sha1" => sha1::Sha1::digest(data).to_vec(),
        b"sha256" => sha2::Sha256::digest(data).to_vec(),
        b"sha512" => sha2::Sha512::digest(data).to_vec(),
        b"xxh3" => twox_hash::XxHash3_64::oneshot(data).to_be_bytes().to_vec(),
        algo => panic!("unsupported hash algorithm: {}", bstr::BStr::new(algo)),
    }
}
