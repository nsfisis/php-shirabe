//! ref: composer/src/Composer/Package/RootPackage.php

use crate::package::{
    CompletePackage, CompletePackageInterface, DisplayMode, Link, Mirror, PackageInterface,
    RootPackageInterface,
};
use crate::repository::RepositoryInterfaceHandle;
use bstr::{BStr, BString};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug, Clone)]
pub struct RootPackage {
    pub(crate) inner: CompletePackage,
    minimum_stability: BString,
    prefer_stable: bool,
    stability_flags: IndexMap<BString, i64>,
    config: IndexMap<BString, PhpMixed>,
    references: IndexMap<BString, BString>,
    aliases: Vec<IndexMap<BString, BString>>,
}

impl RootPackage {
    pub const DEFAULT_PRETTY_VERSION: &'static str = "1.0.0+no-version-set";

    pub fn new(name: BString, version: BString, pretty_version: BString) -> Self {
        let inner = CompletePackage::new(name, version, pretty_version);
        Self {
            inner,
            minimum_stability: "stable".into(),
            prefer_stable: false,
            stability_flags: IndexMap::new(),
            config: IndexMap::new(),
            references: IndexMap::new(),
            aliases: Vec::new(),
        }
    }

    pub fn replace_version(&mut self, version: BString, pretty_version: BString) {
        self.inner.replace_version(version, pretty_version);
    }

    pub fn set_release_date(&mut self, release_date: Option<DateTime<Utc>>) {
        self.inner.set_release_date(release_date);
    }
}

impl RootPackageInterface for RootPackage {
    fn set_minimum_stability(&mut self, minimum_stability: BString) {
        self.minimum_stability = minimum_stability;
    }

    fn get_minimum_stability(&self) -> BString {
        self.minimum_stability.clone()
    }

    fn set_stability_flags(&mut self, stability_flags: IndexMap<BString, i64>) {
        self.stability_flags = stability_flags;
    }

    fn get_stability_flags(&self) -> IndexMap<BString, i64> {
        self.stability_flags.clone()
    }

    fn set_prefer_stable(&mut self, prefer_stable: bool) {
        self.prefer_stable = prefer_stable;
    }

    fn get_prefer_stable(&self) -> bool {
        self.prefer_stable
    }

    fn set_config(&mut self, config: IndexMap<BString, PhpMixed>) {
        self.config = config;
    }

    fn get_config(&self) -> IndexMap<BString, PhpMixed> {
        self.config.clone()
    }

    fn set_references(&mut self, references: IndexMap<BString, BString>) {
        self.references = references;
    }

    fn get_references(&self) -> IndexMap<BString, BString> {
        self.references.clone()
    }

    fn set_aliases(&mut self, aliases: Vec<IndexMap<BString, BString>>) {
        self.aliases = aliases;
    }

    fn get_aliases(&self) -> Vec<IndexMap<BString, BString>> {
        self.aliases.clone()
    }

    fn set_requires(&mut self, requires: IndexMap<BString, Link>) {
        self.inner.inner.set_requires(requires);
    }

    fn set_dev_requires(&mut self, dev_requires: IndexMap<BString, Link>) {
        self.inner.inner.set_dev_requires(dev_requires);
    }

    fn set_conflicts(&mut self, conflicts: IndexMap<BString, Link>) {
        self.inner.inner.set_conflicts(conflicts);
    }

    fn set_provides(&mut self, provides: IndexMap<BString, Link>) {
        self.inner.inner.set_provides(provides);
    }

    fn set_replaces(&mut self, replaces: IndexMap<BString, Link>) {
        self.inner.inner.set_replaces(replaces);
    }

    fn set_autoload(&mut self, autoload: IndexMap<BString, PhpMixed>) {
        self.inner.inner.set_autoload(autoload);
    }

    fn set_dev_autoload(&mut self, dev_autoload: IndexMap<BString, PhpMixed>) {
        self.inner.inner.set_dev_autoload(dev_autoload);
    }

    fn set_suggests(&mut self, suggests: IndexMap<BString, BString>) {
        self.inner.inner.set_suggests(suggests);
    }

    fn set_extra(&mut self, extra: IndexMap<BString, PhpMixed>) {
        self.inner.inner.set_extra(extra);
    }

    fn as_package_interface(&self) -> &dyn PackageInterface {
        self
    }
}

impl CompletePackageInterface for RootPackage {
    fn get_scripts(&self) -> IndexMap<BString, Vec<BString>> {
        self.inner.get_scripts()
    }

    fn set_scripts(&mut self, scripts: IndexMap<BString, Vec<BString>>) {
        self.inner.set_scripts(scripts)
    }

    fn get_repositories(&self) -> IndexMap<BString, PhpMixed> {
        self.inner.get_repositories()
    }

    fn set_repositories(&mut self, repositories: IndexMap<BString, PhpMixed>) {
        self.inner.set_repositories(repositories)
    }

    fn get_license(&self) -> Vec<BString> {
        self.inner.get_license()
    }

    fn set_license(&mut self, license: Vec<BString>) {
        self.inner.set_license(license)
    }

    fn get_keywords(&self) -> Vec<BString> {
        self.inner.get_keywords()
    }

    fn set_keywords(&mut self, keywords: Vec<BString>) {
        self.inner.set_keywords(keywords)
    }

    fn get_description(&self) -> Option<BString> {
        self.inner.get_description()
    }

    fn set_description(&mut self, description: BString) {
        self.inner.set_description(description)
    }

    fn get_homepage(&self) -> Option<BString> {
        self.inner.get_homepage()
    }

    fn set_homepage(&mut self, homepage: BString) {
        self.inner.set_homepage(homepage)
    }

    fn get_authors(&self) -> Vec<IndexMap<BString, BString>> {
        self.inner.get_authors()
    }

    fn set_authors(&mut self, authors: Vec<IndexMap<BString, BString>>) {
        self.inner.set_authors(authors)
    }

    fn get_support(&self) -> IndexMap<BString, BString> {
        self.inner.get_support()
    }

    fn set_support(&mut self, support: IndexMap<BString, BString>) {
        self.inner.set_support(support)
    }

    fn get_funding(&self) -> Vec<IndexMap<BString, PhpMixed>> {
        self.inner.get_funding()
    }

    fn set_funding(&mut self, funding: Vec<IndexMap<BString, PhpMixed>>) {
        self.inner.set_funding(funding)
    }

    fn is_abandoned(&self) -> bool {
        self.inner.is_abandoned()
    }

    fn get_replacement_package(&self) -> Option<BString> {
        self.inner.get_replacement_package()
    }

    fn set_abandoned(&mut self, abandoned: PhpMixed) {
        self.inner.set_abandoned(abandoned)
    }

    fn get_archive_name(&self) -> Option<BString> {
        self.inner.get_archive_name()
    }

    fn set_archive_name(&mut self, name: BString) {
        self.inner.set_archive_name(name)
    }

    fn get_archive_excludes(&self) -> Vec<BString> {
        self.inner.get_archive_excludes()
    }

    fn set_archive_excludes(&mut self, excludes: Vec<BString>) {
        self.inner.set_archive_excludes(excludes)
    }

    fn as_package_interface(&self) -> &dyn PackageInterface {
        self
    }
}

impl PackageInterface for RootPackage {
    fn get_name(&self) -> &BStr {
        self.inner.get_name()
    }
    fn get_pretty_name(&self) -> &BStr {
        self.inner.get_pretty_name()
    }
    fn get_names(&self, provides: bool) -> Vec<BString> {
        self.inner.get_names(provides)
    }
    fn set_id(&mut self, id: i64) {
        self.inner.set_id(id);
    }
    fn get_id(&self) -> i64 {
        self.inner.get_id()
    }
    fn is_dev(&self) -> bool {
        self.inner.is_dev()
    }
    fn get_type(&self) -> BString {
        self.inner.get_type()
    }
    fn get_target_dir(&self) -> Option<BString> {
        self.inner.get_target_dir()
    }
    fn get_extra(&self) -> IndexMap<BString, PhpMixed> {
        self.inner.get_extra()
    }
    fn set_installation_source(&mut self, r#type: Option<BString>) {
        self.inner.set_installation_source(r#type);
    }
    fn get_installation_source(&self) -> Option<BString> {
        self.inner.get_installation_source()
    }
    fn get_source_type(&self) -> Option<BString> {
        self.inner.get_source_type()
    }
    fn get_source_url(&self) -> Option<BString> {
        self.inner.get_source_url()
    }
    fn get_source_urls(&self) -> Vec<BString> {
        self.inner.get_source_urls()
    }
    fn get_source_reference(&self) -> Option<BString> {
        self.inner.get_source_reference()
    }
    fn get_source_mirrors(&self) -> Option<Vec<Mirror>> {
        self.inner.get_source_mirrors()
    }
    fn set_source_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.inner.set_source_mirrors(mirrors);
    }
    fn get_dist_type(&self) -> Option<BString> {
        self.inner.get_dist_type()
    }
    fn get_dist_url(&self) -> Option<BString> {
        self.inner.get_dist_url()
    }
    fn get_dist_urls(&self) -> Vec<BString> {
        self.inner.get_dist_urls()
    }
    fn get_dist_reference(&self) -> Option<BString> {
        self.inner.get_dist_reference()
    }
    fn get_dist_sha1_checksum(&self) -> Option<BString> {
        self.inner.get_dist_sha1_checksum()
    }
    fn get_dist_mirrors(&self) -> Option<Vec<Mirror>> {
        self.inner.get_dist_mirrors()
    }
    fn set_dist_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.inner.set_dist_mirrors(mirrors);
    }
    fn get_version(&self) -> &BStr {
        self.inner.get_version()
    }
    fn get_pretty_version(&self) -> &BStr {
        self.inner.get_pretty_version()
    }
    fn get_full_pretty_version(&self, truncate: bool, display_mode: DisplayMode) -> BString {
        self.inner.get_full_pretty_version(truncate, display_mode)
    }
    fn get_release_date(&self) -> Option<DateTime<Utc>> {
        self.inner.get_release_date()
    }
    fn get_stability(&self) -> &BStr {
        self.inner.get_stability()
    }
    fn get_requires(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        self.inner.get_requires()
    }
    fn get_conflicts(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        self.inner.get_conflicts()
    }
    fn get_provides(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        self.inner.get_provides()
    }
    fn get_replaces(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        self.inner.get_replaces()
    }
    fn get_dev_requires(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        self.inner.get_dev_requires()
    }
    fn get_suggests(&self) -> IndexMap<BString, BString> {
        self.inner.get_suggests()
    }
    fn get_autoload(&self) -> IndexMap<BString, PhpMixed> {
        self.inner.get_autoload()
    }
    fn get_dev_autoload(&self) -> IndexMap<BString, PhpMixed> {
        self.inner.get_dev_autoload()
    }
    fn get_include_paths(&self) -> Vec<BString> {
        self.inner.get_include_paths()
    }
    fn get_php_ext(&self) -> Option<IndexMap<BString, PhpMixed>> {
        self.inner.get_php_ext()
    }
    fn set_repository(&mut self, repository: RepositoryInterfaceHandle) -> anyhow::Result<()> {
        self.inner.set_repository(repository)
    }
    fn get_repository(&self) -> Option<RepositoryInterfaceHandle> {
        self.inner.get_repository()
    }
    fn get_binaries(&self) -> Vec<BString> {
        self.inner.get_binaries()
    }
    fn get_unique_name(&self) -> BString {
        self.inner.get_unique_name()
    }
    fn get_notification_url(&self) -> Option<BString> {
        self.inner.get_notification_url()
    }
    fn get_pretty_string(&self) -> BString {
        self.inner.get_pretty_string()
    }
    fn is_default_branch(&self) -> bool {
        self.inner.is_default_branch()
    }
    fn get_transport_options(&self) -> IndexMap<BString, PhpMixed> {
        self.inner.get_transport_options()
    }
    fn set_transport_options(&mut self, options: IndexMap<BString, PhpMixed>) {
        self.inner.set_transport_options(options);
    }
    fn set_source_reference(&mut self, reference: Option<BString>) {
        self.inner.set_source_reference(reference);
    }
    fn set_source_url(&mut self, url: Option<BString>) {
        self.inner.set_source_url(url);
    }
    fn set_dist_url(&mut self, url: Option<BString>) {
        self.inner.set_dist_url(url);
    }
    fn set_dist_type(&mut self, r#type: Option<BString>) {
        self.inner.set_dist_type(r#type);
    }
    fn set_dist_reference(&mut self, reference: Option<BString>) {
        self.inner.set_dist_reference(reference);
    }
    fn set_source_dist_references(&mut self, reference: BString) {
        self.inner.set_source_dist_references(reference);
    }
}
