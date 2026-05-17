//! ref: composer/src/Composer/Package/Loader/LoaderInterface.php

use crate::package::base_package::BasePackage;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait LoaderInterface {
    fn load(
        &self,
        config: IndexMap<String, PhpMixed>,
        class: Option<String>,
    ) -> anyhow::Result<Box<dyn BasePackage>>;
}
