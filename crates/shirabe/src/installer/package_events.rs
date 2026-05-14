//! ref: composer/src/Composer/Installer/PackageEvents.php

pub struct PackageEvents;

impl PackageEvents {
    pub const PRE_PACKAGE_INSTALL: &'static str = "pre-package-install";
    pub const POST_PACKAGE_INSTALL: &'static str = "post-package-install";
    pub const PRE_PACKAGE_UPDATE: &'static str = "pre-package-update";
    pub const POST_PACKAGE_UPDATE: &'static str = "post-package-update";
    pub const PRE_PACKAGE_UNINSTALL: &'static str = "pre-package-uninstall";
    pub const POST_PACKAGE_UNINSTALL: &'static str = "post-package-uninstall";
}
