//! ref: composer/src/Composer/SelfUpdate/Keys.php

use shirabe_php_shim::{file_get_contents, hash, php_regex, preg_replace, strtoupper};
use std::path::Path;

pub struct Keys;

impl Keys {
    pub fn fingerprint(path: impl AsRef<Path>) -> anyhow::Result<String> {
        let content = file_get_contents(path)?;
        let cleaned = preg_replace(php_regex!(r"{\s}"), "", content);
        let hash = strtoupper(hash("sha256", cleaned))
            .display_lossy()
            .to_string();

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
