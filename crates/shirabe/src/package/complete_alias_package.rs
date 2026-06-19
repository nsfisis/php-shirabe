//! ref: composer/src/Composer/Package/CompleteAliasPackage.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::package::AliasPackage;
use crate::package::CompletePackageHandle;
use crate::package::CompletePackageInterface;
use crate::package::PackageHandle;
use crate::package::handle::delegate_package_interface_to_inner;

#[derive(Debug, Clone)]
pub struct CompleteAliasPackage {
    pub(crate) inner: AliasPackage,
    // overrides AliasPackage::alias_of with the more specific CompletePackage type
    pub(crate) alias_of: CompletePackageHandle,
}

impl CompleteAliasPackage {
    pub fn new(alias_of: CompletePackageHandle, version: String, pretty_version: String) -> Self {
        let inner = AliasPackage::new(
            PackageHandle::from(alias_of.clone()),
            version,
            pretty_version,
        );
        Self { inner, alias_of }
    }

    pub fn get_alias_of(&self) -> CompletePackageHandle {
        self.alias_of.clone()
    }

    pub fn set_root_package_alias(&mut self, value: bool) {
        self.inner.set_root_package_alias(value);
    }

    pub fn is_root_package_alias(&self) -> bool {
        self.inner.is_root_package_alias()
    }

    pub fn has_self_version_requires(&self) -> bool {
        self.inner.has_self_version_requires()
    }
}

delegate_package_interface_to_inner!(CompleteAliasPackage, inner);

impl std::fmt::Display for CompleteAliasPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.inner, f)
    }
}

impl CompletePackageInterface for CompleteAliasPackage {
    fn get_scripts(&self) -> IndexMap<String, Vec<String>> {
        self.alias_of.get_scripts()
    }

    fn set_scripts(&mut self, scripts: IndexMap<String, Vec<String>>) {
        self.alias_of.set_scripts(scripts);
    }

    fn get_repositories(&self) -> IndexMap<String, PhpMixed> {
        self.alias_of.get_repositories()
    }

    fn set_repositories(&mut self, repositories: IndexMap<String, PhpMixed>) {
        self.alias_of.set_repositories(repositories);
    }

    fn get_license(&self) -> Vec<String> {
        self.alias_of.get_license()
    }

    fn set_license(&mut self, license: Vec<String>) {
        self.alias_of.set_license(license);
    }

    fn get_keywords(&self) -> Vec<String> {
        self.alias_of.get_keywords()
    }

    fn set_keywords(&mut self, keywords: Vec<String>) {
        self.alias_of.set_keywords(keywords);
    }

    fn get_description(&self) -> Option<&str> {
        todo!("CompleteAliasPackage::get_description cannot return &str across the aliasOf handle")
    }

    fn set_description(&mut self, description: String) {
        self.alias_of.set_description(description);
    }

    fn get_homepage(&self) -> Option<&str> {
        todo!("CompleteAliasPackage::get_homepage cannot return &str across the aliasOf handle")
    }

    fn set_homepage(&mut self, homepage: String) {
        self.alias_of.set_homepage(homepage);
    }

    fn get_authors(&self) -> Vec<IndexMap<String, String>> {
        self.alias_of.get_authors()
    }

    fn set_authors(&mut self, authors: Vec<IndexMap<String, String>>) {
        self.alias_of.set_authors(authors);
    }

    fn get_support(&self) -> IndexMap<String, String> {
        self.alias_of.get_support()
    }

    fn set_support(&mut self, support: IndexMap<String, String>) {
        self.alias_of.set_support(support);
    }

    fn get_funding(&self) -> Vec<IndexMap<String, PhpMixed>> {
        self.alias_of.get_funding()
    }

    fn set_funding(&mut self, funding: Vec<IndexMap<String, PhpMixed>>) {
        self.alias_of.set_funding(funding);
    }

    fn is_abandoned(&self) -> bool {
        self.alias_of.is_abandoned()
    }

    fn get_replacement_package(&self) -> Option<&str> {
        todo!(
            "CompleteAliasPackage::get_replacement_package cannot return &str across the aliasOf handle"
        )
    }

    fn set_abandoned(&mut self, abandoned: PhpMixed) {
        self.alias_of.set_abandoned(abandoned);
    }

    fn get_archive_name(&self) -> Option<&str> {
        todo!("CompleteAliasPackage::get_archive_name cannot return &str across the aliasOf handle")
    }

    fn set_archive_name(&mut self, name: String) {
        self.alias_of.set_archive_name(name);
    }

    fn get_archive_excludes(&self) -> Vec<String> {
        self.alias_of.get_archive_excludes()
    }

    fn set_archive_excludes(&mut self, excludes: Vec<String>) {
        self.alias_of.set_archive_excludes(excludes);
    }

    fn as_package_interface(&self) -> &dyn crate::package::PackageInterface {
        self
    }
}
