//! ref: composer/src/Composer/Repository/ConfigurableRepositoryInterface.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait ConfigurableRepositoryInterface {
    fn get_repo_config(&self) -> IndexMap<String, PhpMixed>;
}
