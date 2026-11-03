//! ref: composer/src/Composer/Installer/InstallerEvents.php

use bstr::BStr;
use shirabe_php_shim::php_str;

pub struct InstallerEvents;

impl InstallerEvents {
    pub const PRE_OPERATIONS_EXEC: &'static BStr = php_str(b"pre-operations-exec");
}
