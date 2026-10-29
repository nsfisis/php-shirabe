//! ref: composer/src/Composer/Package/CompletePackageInterface.php

use crate::package::PackageInterface;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

pub trait CompletePackageInterface: PackageInterface {
    fn get_scripts(&self) -> IndexMap<BString, Vec<BString>>;

    fn set_scripts(&mut self, scripts: IndexMap<BString, Vec<BString>>);

    fn get_repositories(&self) -> IndexMap<BString, PhpMixed>;

    fn set_repositories(&mut self, repositories: IndexMap<BString, PhpMixed>);

    fn get_license(&self) -> Vec<BString>;

    fn set_license(&mut self, license: Vec<BString>);

    fn get_keywords(&self) -> Vec<BString>;

    fn set_keywords(&mut self, keywords: Vec<BString>);

    fn get_description(&self) -> Option<BString>;

    fn set_description(&mut self, description: BString);

    fn get_homepage(&self) -> Option<BString>;

    fn set_homepage(&mut self, homepage: BString);

    fn get_authors(&self) -> Vec<IndexMap<BString, BString>>;

    fn set_authors(&mut self, authors: Vec<IndexMap<BString, BString>>);

    fn get_support(&self) -> IndexMap<BString, BString>;

    fn set_support(&mut self, support: IndexMap<BString, BString>);

    fn get_funding(&self) -> Vec<IndexMap<BString, PhpMixed>>;

    fn set_funding(&mut self, funding: Vec<IndexMap<BString, PhpMixed>>);

    fn is_abandoned(&self) -> bool;

    fn get_replacement_package(&self) -> Option<BString>;

    fn set_abandoned(&mut self, abandoned: PhpMixed);

    fn get_archive_name(&self) -> Option<BString>;

    fn set_archive_name(&mut self, name: BString);

    fn get_archive_excludes(&self) -> Vec<BString>;

    fn set_archive_excludes(&mut self, excludes: Vec<BString>);

    fn as_package_interface(&self) -> &dyn crate::package::PackageInterface;
}
