//! ref: composer/src/Composer/Package/RootPackage.php

use crate::package::complete_package::CompletePackage;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::root_package_interface::RootPackageInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct RootPackage {
    inner: CompletePackage,
    pub(crate) minimum_stability: String,
    pub(crate) prefer_stable: bool,
    pub(crate) stability_flags: IndexMap<String, i64>,
    pub(crate) config: IndexMap<String, PhpMixed>,
    pub(crate) references: IndexMap<String, String>,
    pub(crate) aliases: Vec<IndexMap<String, String>>,
}

impl RootPackage {
    pub const DEFAULT_PRETTY_VERSION: &'static str = "1.0.0+no-version-set";
}

impl RootPackageInterface for RootPackage {
    fn set_minimum_stability(&mut self, minimum_stability: String) {
        self.minimum_stability = minimum_stability;
    }

    fn get_minimum_stability(&self) -> &str {
        &self.minimum_stability
    }

    fn set_stability_flags(&mut self, stability_flags: IndexMap<String, i64>) {
        self.stability_flags = stability_flags;
    }

    fn get_stability_flags(&self) -> &IndexMap<String, i64> {
        &self.stability_flags
    }

    fn set_prefer_stable(&mut self, prefer_stable: bool) {
        self.prefer_stable = prefer_stable;
    }

    fn get_prefer_stable(&self) -> bool {
        self.prefer_stable
    }

    fn set_config(&mut self, config: IndexMap<String, PhpMixed>) {
        self.config = config;
    }

    fn get_config(&self) -> &IndexMap<String, PhpMixed> {
        &self.config
    }

    fn set_references(&mut self, references: IndexMap<String, String>) {
        self.references = references;
    }

    fn get_references(&self) -> &IndexMap<String, String> {
        &self.references
    }

    fn set_aliases(&mut self, aliases: Vec<IndexMap<String, String>>) {
        self.aliases = aliases;
    }

    fn get_aliases(&self) -> &[IndexMap<String, String>] {
        &self.aliases
    }

    fn set_requires(&mut self, requires: Vec<super::link::Link>) {
        todo!()
    }

    fn set_dev_requires(&mut self, dev_requires: Vec<super::link::Link>) {
        todo!()
    }

    fn set_conflicts(&mut self, conflicts: Vec<super::link::Link>) {
        todo!()
    }

    fn set_provides(&mut self, provides: Vec<super::link::Link>) {
        todo!()
    }

    fn set_replaces(&mut self, replaces: Vec<super::link::Link>) {
        todo!()
    }

    fn set_autoload(&mut self, autoload: IndexMap<String, PhpMixed>) {
        todo!()
    }

    fn set_dev_autoload(&mut self, dev_autoload: IndexMap<String, PhpMixed>) {
        todo!()
    }

    fn set_suggests(&mut self, suggests: IndexMap<String, String>) {
        todo!()
    }

    fn set_extra(&mut self, extra: IndexMap<String, PhpMixed>) {
        todo!()
    }
}

impl CompletePackageInterface for RootPackage {
    fn get_scripts(&self) -> IndexMap<String, Vec<String>> {
        todo!()
    }

    fn set_scripts(&mut self, scripts: IndexMap<String, Vec<String>>) {
        todo!()
    }

    fn get_repositories(&self) -> Vec<IndexMap<String, PhpMixed>> {
        todo!()
    }

    fn set_repositories(&mut self, repositories: Vec<IndexMap<String, PhpMixed>>) {
        todo!()
    }

    fn get_license(&self) -> Vec<String> {
        todo!()
    }

    fn set_license(&mut self, license: Vec<String>) {
        todo!()
    }

    fn get_keywords(&self) -> Vec<String> {
        todo!()
    }

    fn set_keywords(&mut self, keywords: Vec<String>) {
        todo!()
    }

    fn get_description(&self) -> Option<&str> {
        todo!()
    }

    fn set_description(&mut self, description: String) {
        todo!()
    }

    fn get_homepage(&self) -> Option<&str> {
        todo!()
    }

    fn set_homepage(&mut self, homepage: String) {
        todo!()
    }

    fn get_authors(&self) -> Vec<IndexMap<String, String>> {
        todo!()
    }

    fn set_authors(&mut self, authors: Vec<IndexMap<String, String>>) {
        todo!()
    }

    fn get_support(&self) -> IndexMap<String, String> {
        todo!()
    }

    fn set_support(&mut self, support: IndexMap<String, String>) {
        todo!()
    }

    fn get_funding(&self) -> Vec<IndexMap<String, PhpMixed>> {
        todo!()
    }

    fn set_funding(&mut self, funding: Vec<IndexMap<String, PhpMixed>>) {
        todo!()
    }

    fn is_abandoned(&self) -> bool {
        todo!()
    }

    fn get_replacement_package(&self) -> Option<&str> {
        todo!()
    }

    fn set_abandoned(&mut self, abandoned: PhpMixed) {
        todo!()
    }

    fn get_archive_name(&self) -> Option<&str> {
        todo!()
    }

    fn set_archive_name(&mut self, name: String) {
        todo!()
    }

    fn get_archive_excludes(&self) -> Vec<String> {
        todo!()
    }

    fn set_archive_excludes(&mut self, excludes: Vec<String>) {
        todo!()
    }
}
