//! ref: composer/src/Composer/Package/RootAliasPackage.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::package::complete_alias_package::CompleteAliasPackage;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::link::Link;
use crate::package::root_package::RootPackage;
use crate::package::root_package_interface::RootPackageInterface;

#[derive(Debug)]
pub struct RootAliasPackage {
    inner: CompleteAliasPackage,
    // overrides CompleteAliasPackage::alias_of with the more specific RootPackage type
    pub(crate) alias_of: RootPackage,
}

impl RootAliasPackage {
    pub fn new(alias_of: RootPackage, version: String, pretty_version: String) -> Self {
        // TODO(phase-b): RootPackage.inner (CompletePackage) is not accessible here
        let inner: CompleteAliasPackage = todo!();
        Self { inner, alias_of }
    }

    pub fn get_alias_of(&self) -> &RootPackage {
        &self.alias_of
    }
}

impl RootPackageInterface for RootAliasPackage {
    fn get_aliases(&self) -> &[IndexMap<String, String>] {
        self.alias_of.get_aliases()
    }

    fn get_minimum_stability(&self) -> &str {
        self.alias_of.get_minimum_stability()
    }

    fn get_stability_flags(&self) -> &IndexMap<String, i64> {
        self.alias_of.get_stability_flags()
    }

    fn get_references(&self) -> &IndexMap<String, String> {
        self.alias_of.get_references()
    }

    fn get_prefer_stable(&self) -> bool {
        self.alias_of.get_prefer_stable()
    }

    fn get_config(&self) -> &IndexMap<String, PhpMixed> {
        self.alias_of.get_config()
    }

    fn set_requires(&mut self, requires: Vec<Link>) {
        // TODO(phase-b): self.inner.requires = self.replace_self_version_dependencies(requires.clone(), Link::TYPE_REQUIRE)
        self.alias_of.set_requires(requires);
    }

    fn set_dev_requires(&mut self, dev_requires: Vec<Link>) {
        // TODO(phase-b): self.inner.dev_requires = self.replace_self_version_dependencies(dev_requires.clone(), Link::TYPE_DEV_REQUIRE)
        self.alias_of.set_dev_requires(dev_requires);
    }

    fn set_conflicts(&mut self, conflicts: Vec<Link>) {
        // TODO(phase-b): self.inner.conflicts = self.replace_self_version_dependencies(conflicts.clone(), Link::TYPE_CONFLICT)
        self.alias_of.set_conflicts(conflicts);
    }

    fn set_provides(&mut self, provides: Vec<Link>) {
        // TODO(phase-b): self.inner.provides = self.replace_self_version_dependencies(provides.clone(), Link::TYPE_PROVIDE)
        self.alias_of.set_provides(provides);
    }

    fn set_replaces(&mut self, replaces: Vec<Link>) {
        // TODO(phase-b): self.inner.replaces = self.replace_self_version_dependencies(replaces.clone(), Link::TYPE_REPLACE)
        self.alias_of.set_replaces(replaces);
    }

    fn set_autoload(&mut self, autoload: IndexMap<String, PhpMixed>) {
        self.alias_of.set_autoload(autoload);
    }

    fn set_dev_autoload(&mut self, dev_autoload: IndexMap<String, PhpMixed>) {
        self.alias_of.set_dev_autoload(dev_autoload);
    }

    fn set_stability_flags(&mut self, stability_flags: IndexMap<String, i64>) {
        self.alias_of.set_stability_flags(stability_flags);
    }

    fn set_minimum_stability(&mut self, minimum_stability: String) {
        self.alias_of.set_minimum_stability(minimum_stability);
    }

    fn set_prefer_stable(&mut self, prefer_stable: bool) {
        self.alias_of.set_prefer_stable(prefer_stable);
    }

    fn set_config(&mut self, config: IndexMap<String, PhpMixed>) {
        self.alias_of.set_config(config);
    }

    fn set_references(&mut self, references: IndexMap<String, String>) {
        self.alias_of.set_references(references);
    }

    fn set_aliases(&mut self, aliases: Vec<IndexMap<String, String>>) {
        self.alias_of.set_aliases(aliases);
    }

    fn set_suggests(&mut self, suggests: IndexMap<String, String>) {
        self.alias_of.set_suggests(suggests);
    }

    fn set_extra(&mut self, extra: IndexMap<String, PhpMixed>) {
        self.alias_of.set_extra(extra);
    }
}

impl CompletePackageInterface for RootAliasPackage {
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
