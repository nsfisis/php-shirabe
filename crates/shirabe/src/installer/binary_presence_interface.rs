//! ref: composer/src/Composer/Installer/BinaryPresenceInterface.php

use crate::package::package_interface::PackageInterface;

pub trait BinaryPresenceInterface {
    fn ensure_binaries_presence(&self, package: &dyn PackageInterface);
}
