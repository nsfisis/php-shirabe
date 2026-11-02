//! ref: composer/src/Composer/Installer/PackageEvents.php

use bstr::BStr;
use shirabe_php_shim::php_str;

pub struct PackageEvents;

impl PackageEvents {
    pub const PRE_PACKAGE_INSTALL: &'static BStr = php_str(b"pre-package-install");
    pub const POST_PACKAGE_INSTALL: &'static BStr = php_str(b"post-package-install");
    pub const PRE_PACKAGE_UPDATE: &'static BStr = php_str(b"pre-package-update");
    pub const POST_PACKAGE_UPDATE: &'static BStr = php_str(b"post-package-update");
    pub const PRE_PACKAGE_UNINSTALL: &'static BStr = php_str(b"pre-package-uninstall");
    pub const POST_PACKAGE_UNINSTALL: &'static BStr = php_str(b"post-package-uninstall");
}
