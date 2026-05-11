//! ref: composer/src/Composer/Package/Loader/LoaderInterface.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use crate::package::base_package::BasePackage;

pub trait LoaderInterface {
    fn load(&self, config: IndexMap<String, PhpMixed>, class: Option<String>) -> Box<BasePackage>;
}
