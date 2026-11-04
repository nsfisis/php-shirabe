//! ref: composer/src/Composer/Installer/InstallerEvents.php sha1:411d9237d10ba7406d2ad7bf32d3d0700d39b0bf

use bstr::BStr;
use shirabe_php_shim::php_str;

pub struct InstallerEvents;

impl InstallerEvents {
    pub const PRE_OPERATIONS_EXEC: &'static BStr = php_str(b"pre-operations-exec");
}
