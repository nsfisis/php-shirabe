//! ref: composer/src/Composer/Installer/BinaryPresenceInterface.php sha1:b843d440aef093884c881ca365e5d802a0a57361

use crate::package::PackageInterfaceHandle;

pub trait BinaryPresenceInterface {
    fn ensure_binaries_presence(&self, package: PackageInterfaceHandle) -> anyhow::Result<()>;
}
