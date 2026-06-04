//! ref: composer/src/Composer/Installer/BinaryPresenceInterface.php

use crate::package::PackageInterfaceHandle;

pub trait BinaryPresenceInterface {
    fn ensure_binaries_presence(&mut self, package: PackageInterfaceHandle);
}
