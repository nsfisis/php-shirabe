//! ref: composer/src/Composer/Package/RootAliasPackage.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::package::CompleteAliasPackage;
use crate::package::CompletePackageHandle;
use crate::package::CompletePackageInterface;
use crate::package::Link;
use crate::package::RootPackageHandle;
use crate::package::RootPackageInterface;
use crate::package::handle::delegate_package_interface_to_inner;

#[derive(Debug, Clone)]
pub struct RootAliasPackage {
    pub(crate) inner: CompleteAliasPackage,
    // overrides CompleteAliasPackage::alias_of with the more specific RootPackage type
    pub(crate) alias_of: RootPackageHandle,
}

impl RootAliasPackage {
    pub fn new(alias_of: RootPackageHandle, version: String, pretty_version: String) -> Self {
        let inner = CompleteAliasPackage::new(
            CompletePackageHandle::from(alias_of.clone()),
            version,
            pretty_version,
        );
        Self { inner, alias_of }
    }

    pub fn get_alias_of(&self) -> RootPackageHandle {
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

delegate_package_interface_to_inner!(RootAliasPackage, inner);

impl std::fmt::Display for RootAliasPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.inner, f)
    }
}

impl RootPackageInterface for RootAliasPackage {
    fn get_aliases(&self) -> &[IndexMap<String, String>] {
        todo!("RootAliasPackage::get_aliases cannot return a borrow across the aliasOf handle")
    }

    fn get_minimum_stability(&self) -> &str {
        todo!(
            "RootAliasPackage::get_minimum_stability cannot return &str across the aliasOf handle"
        )
    }

    fn get_stability_flags(&self) -> &IndexMap<String, i64> {
        todo!(
            "RootAliasPackage::get_stability_flags cannot return a borrow across the aliasOf handle"
        )
    }

    fn get_references(&self) -> &IndexMap<String, String> {
        todo!("RootAliasPackage::get_references cannot return a borrow across the aliasOf handle")
    }

    fn get_prefer_stable(&self) -> bool {
        self.alias_of.get_prefer_stable()
    }

    fn get_config(&self) -> &IndexMap<String, PhpMixed> {
        todo!("RootAliasPackage::get_config cannot return a borrow across the aliasOf handle")
    }

    fn set_requires(&mut self, requires: Vec<Link>) {
        let replaced = self
            .inner
            .inner
            .replace_self_version_dependencies(requires.clone(), Link::TYPE_REQUIRE);
        self.inner.inner.requires = replaced
            .into_iter()
            .map(|l| (l.get_target().to_string(), l))
            .collect();
        self.alias_of.set_requires(requires);
    }

    fn set_dev_requires(&mut self, dev_requires: Vec<Link>) {
        self.alias_of.set_dev_requires(dev_requires);
    }

    fn set_conflicts(&mut self, conflicts: Vec<Link>) {
        self.alias_of.set_conflicts(conflicts);
    }

    fn set_provides(&mut self, provides: Vec<Link>) {
        self.alias_of.set_provides(provides);
    }

    fn set_replaces(&mut self, replaces: Vec<Link>) {
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
        self.inner.get_scripts()
    }

    fn set_scripts(&mut self, scripts: IndexMap<String, Vec<String>>) {
        self.inner.set_scripts(scripts);
    }

    fn get_repositories(&self) -> Vec<IndexMap<String, PhpMixed>> {
        self.inner.get_repositories()
    }

    fn set_repositories(&mut self, repositories: Vec<IndexMap<String, PhpMixed>>) {
        self.inner.set_repositories(repositories);
    }

    fn get_license(&self) -> Vec<String> {
        self.inner.get_license()
    }

    fn set_license(&mut self, license: Vec<String>) {
        self.inner.set_license(license);
    }

    fn get_keywords(&self) -> Vec<String> {
        self.inner.get_keywords()
    }

    fn set_keywords(&mut self, keywords: Vec<String>) {
        self.inner.set_keywords(keywords);
    }

    fn get_description(&self) -> Option<&str> {
        self.inner.get_description()
    }

    fn set_description(&mut self, description: String) {
        self.inner.set_description(description);
    }

    fn get_homepage(&self) -> Option<&str> {
        self.inner.get_homepage()
    }

    fn set_homepage(&mut self, homepage: String) {
        self.inner.set_homepage(homepage);
    }

    fn get_authors(&self) -> Vec<IndexMap<String, String>> {
        self.inner.get_authors()
    }

    fn set_authors(&mut self, authors: Vec<IndexMap<String, String>>) {
        self.inner.set_authors(authors);
    }

    fn get_support(&self) -> IndexMap<String, String> {
        self.inner.get_support()
    }

    fn set_support(&mut self, support: IndexMap<String, String>) {
        self.inner.set_support(support);
    }

    fn get_funding(&self) -> Vec<IndexMap<String, PhpMixed>> {
        self.inner.get_funding()
    }

    fn set_funding(&mut self, funding: Vec<IndexMap<String, PhpMixed>>) {
        self.inner.set_funding(funding);
    }

    fn is_abandoned(&self) -> bool {
        self.inner.is_abandoned()
    }

    fn get_replacement_package(&self) -> Option<&str> {
        self.inner.get_replacement_package()
    }

    fn set_abandoned(&mut self, abandoned: PhpMixed) {
        self.inner.set_abandoned(abandoned);
    }

    fn get_archive_name(&self) -> Option<&str> {
        self.inner.get_archive_name()
    }

    fn set_archive_name(&mut self, name: String) {
        self.inner.set_archive_name(name);
    }

    fn get_archive_excludes(&self) -> Vec<String> {
        self.inner.get_archive_excludes()
    }

    fn set_archive_excludes(&mut self, excludes: Vec<String>) {
        self.inner.set_archive_excludes(excludes);
    }
}
