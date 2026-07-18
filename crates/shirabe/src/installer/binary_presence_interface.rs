//! ref: composer/src/Composer/Installer/BinaryPresenceInterface.php

use crate::package::PackageInterfaceHandle;

pub trait BinaryPresenceInterface {
    fn ensure_binaries_presence(&self, package: PackageInterfaceHandle);
}
