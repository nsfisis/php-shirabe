//! ref: composer/vendor/symfony/filesystem/Exception/IOException.php

use bstr::BString;
use shirabe_php_shim::RuntimeException;
use std::path::PathBuf;

#[derive(Debug)]
pub struct IOException {
    inner: RuntimeException,
    pub path: Option<PathBuf>,
}

impl IOException {
    #[track_caller]
    pub fn new(
        message: impl Into<BString>,
        code: i64,
        previous: Option<std::sync::Arc<anyhow::Error>>,
        path: Option<PathBuf>,
    ) -> Self {
        Self {
            inner: RuntimeException::with_code_and_previous(message, code, previous),
            path,
        }
    }
}

shirabe_php_shim::impl_php_exception!(
    IOException,
    inner,
    r"Symfony\Component\Filesystem\Exception\IOException"
);
