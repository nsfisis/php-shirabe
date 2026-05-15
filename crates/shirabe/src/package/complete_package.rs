//! ref: composer/src/Composer/Package/CompletePackage.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::package::Package;

#[derive(Debug)]
pub struct CompletePackage {
    pub(crate) inner: Package,
    pub(crate) repositories: Vec<IndexMap<String, PhpMixed>>,
    pub(crate) license: Vec<String>,
    pub(crate) keywords: Vec<String>,
    pub(crate) authors: Vec<IndexMap<String, String>>,
    pub(crate) description: Option<String>,
    pub(crate) homepage: Option<String>,
    pub(crate) scripts: IndexMap<String, Vec<String>>,
    pub(crate) support: IndexMap<String, String>,
    pub(crate) funding: Vec<IndexMap<String, PhpMixed>>,
    pub(crate) abandoned: PhpMixed,
    pub(crate) archive_name: Option<String>,
    pub(crate) archive_excludes: Vec<String>,
}

impl CompletePackageInterface for CompletePackage {
    fn set_scripts(&mut self, scripts: IndexMap<String, Vec<String>>) {
        self.scripts = scripts;
    }

    fn get_scripts(&self) -> IndexMap<String, Vec<String>> {
        self.scripts.clone()
    }

    fn set_repositories(&mut self, repositories: Vec<IndexMap<String, PhpMixed>>) {
        self.repositories = repositories;
    }

    fn get_repositories(&self) -> Vec<IndexMap<String, PhpMixed>> {
        self.repositories.clone()
    }

    fn set_license(&mut self, license: Vec<String>) {
        self.license = license;
    }

    fn get_license(&self) -> Vec<String> {
        self.license.clone()
    }

    fn set_keywords(&mut self, keywords: Vec<String>) {
        self.keywords = keywords;
    }

    fn get_keywords(&self) -> Vec<String> {
        self.keywords.clone()
    }

    fn set_authors(&mut self, authors: Vec<IndexMap<String, String>>) {
        self.authors = authors;
    }

    fn get_authors(&self) -> Vec<IndexMap<String, String>> {
        self.authors.clone()
    }

    fn set_description(&mut self, description: String) {
        self.description = Some(description);
    }

    fn get_description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn set_homepage(&mut self, homepage: String) {
        self.homepage = Some(homepage);
    }

    fn get_homepage(&self) -> Option<&str> {
        self.homepage.as_deref()
    }

    fn set_support(&mut self, support: IndexMap<String, String>) {
        self.support = support;
    }

    fn get_support(&self) -> IndexMap<String, String> {
        self.support.clone()
    }

    fn set_funding(&mut self, funding: Vec<IndexMap<String, PhpMixed>>) {
        self.funding = funding;
    }

    fn get_funding(&self) -> Vec<IndexMap<String, PhpMixed>> {
        self.funding.clone()
    }

    fn is_abandoned(&self) -> bool {
        match &self.abandoned {
            PhpMixed::Bool(b) => *b,
            PhpMixed::String(s) => !s.is_empty(),
            _ => false,
        }
    }

    fn set_abandoned(&mut self, abandoned: PhpMixed) {
        self.abandoned = abandoned;
    }

    fn get_replacement_package(&self) -> Option<&str> {
        match &self.abandoned {
            PhpMixed::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    fn set_archive_name(&mut self, name: String) {
        self.archive_name = Some(name);
    }

    fn get_archive_name(&self) -> Option<&str> {
        self.archive_name.as_deref()
    }

    fn set_archive_excludes(&mut self, excludes: Vec<String>) {
        self.archive_excludes = excludes;
    }

    fn get_archive_excludes(&self) -> Vec<String> {
        self.archive_excludes.clone()
    }
}
