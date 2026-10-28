//! ref: composer/src/Composer/SelfUpdate/Keys.php

use regex::Regex;
use shirabe_php_shim::{hash, legacy_lossy, strtoupper};

pub struct Keys;

impl Keys {
    pub fn fingerprint(path: &str) -> anyhow::Result<String> {
        let content = std::fs::read_to_string(path)?;
        let re = Regex::new(r"\s").unwrap();
        let cleaned = re.replace_all(&content, "");
        let hash = legacy_lossy(&strtoupper(hash("sha256", cleaned.as_bytes()))).into_owned();

        Ok([
            &hash[0..8],
            &hash[8..16],
            &hash[16..24],
            &hash[24..32],
            "",
            &hash[32..40],
            &hash[40..48],
            &hash[48..56],
            &hash[56..64],
        ]
        .join(" "))
    }
}
