//! ref: composer/src/Composer/Package/CompletePackageInterface.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::package::package_interface::PackageInterface;

pub trait CompletePackageInterface: PackageInterface {
    fn get_scripts(&self) -> IndexMap<String, Vec<String>>;

    fn set_scripts(&mut self, scripts: IndexMap<String, Vec<String>>);

    fn get_repositories(&self) -> Vec<IndexMap<String, PhpMixed>>;

    fn set_repositories(&mut self, repositories: Vec<IndexMap<String, PhpMixed>>);

    fn get_license(&self) -> Vec<String>;

    fn set_license(&mut self, license: Vec<String>);

    fn get_keywords(&self) -> Vec<String>;

    fn set_keywords(&mut self, keywords: Vec<String>);

    fn get_description(&self) -> Option<&str>;

    fn set_description(&mut self, description: String);

    fn get_homepage(&self) -> Option<&str>;

    fn set_homepage(&mut self, homepage: String);

    fn get_authors(&self) -> Vec<IndexMap<String, String>>;

    fn set_authors(&mut self, authors: Vec<IndexMap<String, String>>);

    fn get_support(&self) -> IndexMap<String, String>;

    fn set_support(&mut self, support: IndexMap<String, String>);

    fn get_funding(&self) -> Vec<IndexMap<String, PhpMixed>>;

    fn set_funding(&mut self, funding: Vec<IndexMap<String, PhpMixed>>);

    fn is_abandoned(&self) -> bool;

    fn get_replacement_package(&self) -> Option<&str>;

    fn set_abandoned(&mut self, abandoned: PhpMixed);

    fn get_archive_name(&self) -> Option<&str>;

    fn set_archive_name(&mut self, name: String);

    fn get_archive_excludes(&self) -> Vec<String>;

    fn set_archive_excludes(&mut self, excludes: Vec<String>);
}
