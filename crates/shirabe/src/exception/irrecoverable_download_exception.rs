//! ref: composer/src/Composer/Exception/IrrecoverableDownloadException.php

use shirabe_php_shim::RuntimeException;

pub struct IrrecoverableDownloadException(pub RuntimeException);
