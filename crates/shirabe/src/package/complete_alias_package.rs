//! ref: composer/src/Composer/Package/CompleteAliasPackage.php

use crate::package::alias_package::AliasPackage;
use crate::package::complete_package::CompletePackage;
use crate::package::complete_package_interface::CompletePackageInterface;

#[derive(Debug)]
pub struct CompleteAliasPackage {
    inner: AliasPackage,
    // overrides AliasPackage::alias_of with the more specific CompletePackage type
    pub(crate) alias_of: CompletePackage,
}

impl CompleteAliasPackage {
    pub fn new(alias_of: CompletePackage, version: String, pretty_version: String) -> Self {
        let inner = AliasPackage::new(alias_of.clone(), version, pretty_version);
        Self { inner, alias_of }
    }

    pub fn get_alias_of(&self) -> &CompletePackage {
        &self.alias_of
    }

    pub fn get_scripts(&self) -> indexmap::IndexMap<String, Vec<String>> {
        self.alias_of.get_scripts()
    }

    pub fn set_scripts(&mut self, scripts: indexmap::IndexMap<String, Vec<String>>) {
        self.alias_of.set_scripts(scripts);
    }

    pub fn get_repositories(&self) -> Vec<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>> {
        self.alias_of.get_repositories()
    }

    pub fn set_repositories(
        &mut self,
        repositories: Vec<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>>,
    ) {
        self.alias_of.set_repositories(repositories);
    }

    pub fn get_license(&self) -> Vec<String> {
        self.alias_of.get_license()
    }

    pub fn set_license(&mut self, license: Vec<String>) {
        self.alias_of.set_license(license);
    }

    pub fn get_keywords(&self) -> Vec<String> {
        self.alias_of.get_keywords()
    }

    pub fn set_keywords(&mut self, keywords: Vec<String>) {
        self.alias_of.set_keywords(keywords);
    }

    pub fn get_description(&self) -> Option<&str> {
        self.alias_of.get_description()
    }

    pub fn set_description(&mut self, description: Option<String>) {
        self.alias_of.set_description(description);
    }

    pub fn get_homepage(&self) -> Option<&str> {
        self.alias_of.get_homepage()
    }

    pub fn set_homepage(&mut self, homepage: Option<String>) {
        self.alias_of.set_homepage(homepage);
    }

    pub fn get_authors(&self) -> Vec<indexmap::IndexMap<String, String>> {
        self.alias_of.get_authors()
    }

    pub fn set_authors(&mut self, authors: Vec<indexmap::IndexMap<String, String>>) {
        self.alias_of.set_authors(authors);
    }

    pub fn get_support(&self) -> indexmap::IndexMap<String, String> {
        self.alias_of.get_support()
    }

    pub fn set_support(&mut self, support: indexmap::IndexMap<String, String>) {
        self.alias_of.set_support(support);
    }

    pub fn get_funding(&self) -> Vec<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>> {
        self.alias_of.get_funding()
    }

    pub fn set_funding(
        &mut self,
        funding: Vec<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>>,
    ) {
        self.alias_of.set_funding(funding);
    }

    pub fn is_abandoned(&self) -> bool {
        self.alias_of.is_abandoned()
    }

    pub fn get_replacement_package(&self) -> Option<&str> {
        self.alias_of.get_replacement_package()
    }

    pub fn set_abandoned(&mut self, abandoned: shirabe_php_shim::PhpMixed) {
        self.alias_of.set_abandoned(abandoned);
    }

    pub fn get_archive_name(&self) -> Option<&str> {
        self.alias_of.get_archive_name()
    }

    pub fn set_archive_name(&mut self, name: Option<String>) {
        self.alias_of.set_archive_name(name);
    }

    pub fn get_archive_excludes(&self) -> Vec<String> {
        self.alias_of.get_archive_excludes()
    }

    pub fn set_archive_excludes(&mut self, excludes: Vec<String>) {
        self.alias_of.set_archive_excludes(excludes);
    }
}
