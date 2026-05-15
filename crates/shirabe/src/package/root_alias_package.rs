//! ref: composer/src/Composer/Package/RootAliasPackage.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::package::complete_alias_package::CompleteAliasPackage;
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
    fn get_aliases(&self) -> Vec<IndexMap<String, String>> {
        self.alias_of.get_aliases().clone()
    }

    fn get_minimum_stability(&self) -> &str {
        self.alias_of.get_minimum_stability()
    }

    fn get_stability_flags(&self) -> IndexMap<String, i64> {
        self.alias_of.get_stability_flags().clone()
    }

    fn get_references(&self) -> IndexMap<String, String> {
        self.alias_of.get_references().clone()
    }

    fn get_prefer_stable(&self) -> bool {
        self.alias_of.get_prefer_stable()
    }

    fn get_config(&self) -> IndexMap<String, PhpMixed> {
        self.alias_of.get_config().clone()
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
