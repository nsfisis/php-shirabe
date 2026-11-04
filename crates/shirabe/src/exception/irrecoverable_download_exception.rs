//! ref: composer/src/Composer/Exception/IrrecoverableDownloadException.php sha1:633400c9a9dfb5973a890954e46c098339c2d86a

use bstr::BString;
use shirabe_php_shim::RuntimeException;

#[derive(Debug)]
pub struct IrrecoverableDownloadException(pub RuntimeException);

impl IrrecoverableDownloadException {
    #[track_caller]
    pub fn new(message: impl Into<BString>) -> Self {
        Self(RuntimeException::new(message))
    }
}

shirabe_php_shim::impl_php_exception!(
    IrrecoverableDownloadException,
    0,
    r"Composer\Exception\IrrecoverableDownloadException"
);
