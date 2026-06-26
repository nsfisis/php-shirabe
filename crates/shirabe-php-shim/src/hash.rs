use crate::bin2hex;
use sha1::Digest as _;

pub fn hash(algo: &str, data: &str) -> String {
    crate::bin2hex(&calculate_hash(algo, data.as_bytes()))
}

pub fn hash_raw(algo: &str, data: &str) -> Vec<u8> {
    calculate_hash(algo, data.as_bytes())
}

pub fn hash_file(algo: &str, filename: impl AsRef<std::path::Path>) -> Option<String> {
    let data = std::fs::read(filename).ok()?;
    Some(bin2hex(&calculate_hash(algo, &data)))
}

fn calculate_hash(algo: &str, data: &[u8]) -> Vec<u8> {
    match algo {
        "md5" => md5::compute(data).0.to_vec(),
        "sha1" => sha1::Sha1::digest(data).to_vec(),
        "sha256" => sha2::Sha256::digest(data).to_vec(),
        "xxh3" => twox_hash::XxHash3_64::oneshot(data).to_be_bytes().to_vec(),
        _ => panic!("unsupported hash algorithm: {}", algo),
    }
}
