//! ref: composer/vendor/symfony/filesystem/Exception/IOException.php

use shirabe_php_shim::RuntimeException;

#[derive(Debug)]
pub struct IOException {
    inner: RuntimeException,
    pub path: Option<String>,
}

impl IOException {
    pub fn new(
        message: String,
        code: i64,
        previous: Option<std::sync::Arc<anyhow::Error>>,
        path: Option<String>,
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
