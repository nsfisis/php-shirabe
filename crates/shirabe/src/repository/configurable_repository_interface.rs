//! ref: composer/src/Composer/Repository/ConfigurableRepositoryInterface.php sha1:d5f2b0ada7d31cc0602150e85329060755dab9f0

use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait ConfigurableRepositoryInterface {
    fn get_repo_config(&self) -> IndexMap<BString, PhpMixed>;
}
