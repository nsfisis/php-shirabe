//! ref: composer/vendor/symfony/filesystem/Exception/IOException.php sha1:26c2d0ed00f4063a561046d700938b161fe3d7ec

use bstr::BString;
use shirabe_php_shim::{RuntimeException, impl_php_exception};

#[derive(Debug)]
pub struct IOException {
    inner: RuntimeException,
    pub path: Option<std::path::PathBuf>,
}

impl IOException {
    #[track_caller]
    pub fn new(
        message: impl Into<BString>,
        code: i64,
        previous: Option<std::sync::Arc<anyhow::Error>>,
        path: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            inner: RuntimeException::with_code_and_previous(message, code, previous),
            path,
        }
    }
}

impl_php_exception!(
    IOException,
    inner,
    r"Symfony\Component\Filesystem\Exception\IOException"
);
