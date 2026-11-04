//! ref: composer/vendor/symfony/process/Exception/RuntimeException.php sha1:c166d875f9b7b1503969bb0e28b79842218ff8e9

use bstr::BString;

#[derive(Debug)]
pub struct RuntimeException {
    inner: shirabe_php_shim::RuntimeException,
}

impl RuntimeException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self {
            inner: shirabe_php_shim::RuntimeException::new(message),
        }
    }
}

shirabe_php_shim::impl_php_exception!(
    RuntimeException,
    inner,
    r"Symfony\Component\Process\Exception\RuntimeException"
);
