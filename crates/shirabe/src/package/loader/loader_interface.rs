//! ref: composer/src/Composer/Package/Loader/LoaderInterface.php

use crate::package::PackageInterfaceHandle;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait LoaderInterface: std::fmt::Debug {
    fn load(
        &self,
        config: IndexMap<String, PhpMixed>,
        class: Option<String>,
    ) -> anyhow::Result<PackageInterfaceHandle>;
}
