//! ref: composer/src/Composer/Repository/ConfigurableRepositoryInterface.php

use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait ConfigurableRepositoryInterface {
    fn get_repo_config(&self) -> IndexMap<BString, PhpMixed>;
}
