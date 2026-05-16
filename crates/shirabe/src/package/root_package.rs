//! ref: composer/src/Composer/Package/RootPackage.php

use crate::package::complete_package::CompletePackage;
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

    fn get_aliases(&self) -> &Vec<IndexMap<String, String>> {
        &self.aliases
    }
}
