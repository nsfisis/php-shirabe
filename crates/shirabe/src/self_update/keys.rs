//! ref: composer/src/Composer/SelfUpdate/Keys.php

use bstr::BString;
use shirabe_php_shim::{file_get_contents, hash, implode, php_regex, preg_replace, strtoupper};
use std::path::Path;

pub struct Keys;

impl Keys {
    pub fn fingerprint(path: impl AsRef<Path>) -> anyhow::Result<BString> {
        let content = file_get_contents(path)?;
        let cleaned = preg_replace(php_regex!(r"{\s}"), "", content);
        let hash = strtoupper(hash("sha256", cleaned)).to_owned();

        Ok(implode(
            " ",
            &[
                &hash[0..8],
                &hash[8..16],
                &hash[16..24],
                &hash[24..32],
                &b""[..],
                &hash[32..40],
                &hash[40..48],
                &hash[48..56],
                &hash[56..64],
            ],
        ))
    }
}
