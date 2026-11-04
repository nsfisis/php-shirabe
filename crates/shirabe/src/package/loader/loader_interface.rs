//! ref: composer/src/Composer/Package/Loader/LoaderInterface.php sha1:d15acd1b52befbc1fa6554adb0308060c31fc58e

use crate::package::PackageInterfaceHandle;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait LoaderInterface: std::fmt::Debug {
    fn load(
        &self,
        config: IndexMap<BString, PhpMixed>,
        class: Option<BString>,
    ) -> anyhow::Result<PackageInterfaceHandle>;

    fn as_any(&self) -> &dyn std::any::Any;
}
