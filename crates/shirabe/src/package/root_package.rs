//! ref: composer/src/Composer/Package/RootPackage.php

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::package::CompletePackage;
use crate::package::CompletePackageInterface;
use crate::package::DisplayMode;
use crate::package::Link;
use crate::package::Mirror;
use crate::package::PackageInterface;
use crate::package::RootPackageInterface;
use crate::repository::RepositoryInterfaceHandle;

#[derive(Debug, Clone)]
pub struct RootPackage {
    pub(crate) inner: CompletePackage,
    pub(crate) minimum_stability: String,
    pub(crate) prefer_stable: bool,
    pub(crate) stability_flags: IndexMap<String, i64>,
    pub(crate) config: IndexMap<String, PhpMixed>,
    pub(crate) references: IndexMap<String, String>,
    pub(crate) aliases: Vec<IndexMap<String, String>>,
}

impl RootPackage {
    pub const DEFAULT_PRETTY_VERSION: &'static str = "1.0.0+no-version-set";

    pub fn new(name: String, version: String, pretty_version: String) -> Self {
        let inner = CompletePackage::new(name, version, pretty_version);
        Self {
            inner,
            minimum_stability: "stable".to_string(),
            prefer_stable: false,
            stability_flags: IndexMap::new(),
            config: IndexMap::new(),
            references: IndexMap::new(),
            aliases: Vec::new(),
        }
    }

    pub fn replace_version(&mut self, version: String, pretty_version: String) {
        self.inner.replace_version(version, pretty_version);
    }
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

    fn set_requires(&mut self, requires: IndexMap<String, Link>) {
        self.inner.inner.set_requires(requires);
    }

    fn set_dev_requires(&mut self, dev_requires: IndexMap<String, Link>) {
        self.inner.inner.set_dev_requires(dev_requires);
    }

    fn set_conflicts(&mut self, conflicts: IndexMap<String, Link>) {
        self.inner.inner.set_conflicts(conflicts);
    }

    fn set_provides(&mut self, provides: IndexMap<String, Link>) {
        self.inner.inner.set_provides(provides);
    }

    fn set_replaces(&mut self, replaces: IndexMap<String, Link>) {
        self.inner.inner.set_replaces(replaces);
    }

    fn set_autoload(&mut self, autoload: IndexMap<String, PhpMixed>) {
        self.inner.inner.set_autoload(autoload);
    }

    fn set_dev_autoload(&mut self, dev_autoload: IndexMap<String, PhpMixed>) {
        self.inner.inner.set_dev_autoload(dev_autoload);
    }

    fn set_suggests(&mut self, suggests: IndexMap<String, String>) {
        self.inner.inner.set_suggests(suggests);
    }

    fn set_extra(&mut self, extra: IndexMap<String, PhpMixed>) {
        self.inner.inner.set_extra(extra);
    }

    fn as_package_interface(&self) -> &dyn PackageInterface {
        self
    }
}

impl CompletePackageInterface for RootPackage {
    fn get_scripts(&self) -> IndexMap<String, Vec<String>> {
        CompletePackageInterface::get_scripts(&self.inner)
    }

    fn set_scripts(&mut self, scripts: IndexMap<String, Vec<String>>) {
        CompletePackageInterface::set_scripts(&mut self.inner, scripts)
    }

    fn get_repositories(&self) -> IndexMap<String, PhpMixed> {
        CompletePackageInterface::get_repositories(&self.inner)
    }

    fn set_repositories(&mut self, repositories: IndexMap<String, PhpMixed>) {
        CompletePackageInterface::set_repositories(&mut self.inner, repositories)
    }

    fn get_license(&self) -> Vec<String> {
        CompletePackageInterface::get_license(&self.inner)
    }

    fn set_license(&mut self, license: Vec<String>) {
        CompletePackageInterface::set_license(&mut self.inner, license)
    }

    fn get_keywords(&self) -> Vec<String> {
        CompletePackageInterface::get_keywords(&self.inner)
    }

    fn set_keywords(&mut self, keywords: Vec<String>) {
        CompletePackageInterface::set_keywords(&mut self.inner, keywords)
    }

    fn get_description(&self) -> Option<&str> {
        CompletePackageInterface::get_description(&self.inner)
    }

    fn set_description(&mut self, description: String) {
        CompletePackageInterface::set_description(&mut self.inner, description)
    }

    fn get_homepage(&self) -> Option<&str> {
        CompletePackageInterface::get_homepage(&self.inner)
    }

    fn set_homepage(&mut self, homepage: String) {
        CompletePackageInterface::set_homepage(&mut self.inner, homepage)
    }

    fn get_authors(&self) -> Vec<IndexMap<String, String>> {
        CompletePackageInterface::get_authors(&self.inner)
    }

    fn set_authors(&mut self, authors: Vec<IndexMap<String, String>>) {
        CompletePackageInterface::set_authors(&mut self.inner, authors)
    }

    fn get_support(&self) -> IndexMap<String, String> {
        CompletePackageInterface::get_support(&self.inner)
    }

    fn set_support(&mut self, support: IndexMap<String, String>) {
        CompletePackageInterface::set_support(&mut self.inner, support)
    }

    fn get_funding(&self) -> Vec<IndexMap<String, PhpMixed>> {
        CompletePackageInterface::get_funding(&self.inner)
    }

    fn set_funding(&mut self, funding: Vec<IndexMap<String, PhpMixed>>) {
        CompletePackageInterface::set_funding(&mut self.inner, funding)
    }

    fn is_abandoned(&self) -> bool {
        CompletePackageInterface::is_abandoned(&self.inner)
    }

    fn get_replacement_package(&self) -> Option<&str> {
        CompletePackageInterface::get_replacement_package(&self.inner)
    }

    fn set_abandoned(&mut self, abandoned: PhpMixed) {
        CompletePackageInterface::set_abandoned(&mut self.inner, abandoned)
    }

    fn get_archive_name(&self) -> Option<&str> {
        CompletePackageInterface::get_archive_name(&self.inner)
    }

    fn set_archive_name(&mut self, name: String) {
        CompletePackageInterface::set_archive_name(&mut self.inner, name)
    }

    fn get_archive_excludes(&self) -> Vec<String> {
        CompletePackageInterface::get_archive_excludes(&self.inner)
    }

    fn set_archive_excludes(&mut self, excludes: Vec<String>) {
        CompletePackageInterface::set_archive_excludes(&mut self.inner, excludes)
    }

    fn as_package_interface(&self) -> &dyn PackageInterface {
        self
    }
}

impl std::fmt::Display for RootPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_unique_name())
    }
}

impl PackageInterface for RootPackage {
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }
    fn get_pretty_name(&self) -> &str {
        self.inner.get_pretty_name()
    }
    fn get_names(&self, provides: bool) -> Vec<String> {
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
    fn get_type(&self) -> String {
        self.inner.get_type()
    }
    fn get_target_dir(&self) -> Option<String> {
        self.inner.get_target_dir()
    }
    fn get_extra(&self) -> IndexMap<String, PhpMixed> {
        self.inner.get_extra()
    }
    fn set_installation_source(&mut self, r#type: Option<String>) {
        self.inner.set_installation_source(r#type);
    }
    fn get_installation_source(&self) -> Option<&str> {
        self.inner.get_installation_source()
    }
    fn get_source_type(&self) -> Option<&str> {
        self.inner.get_source_type()
    }
    fn get_source_url(&self) -> Option<&str> {
        self.inner.get_source_url()
    }
    fn get_source_urls(&self) -> Vec<String> {
        self.inner.get_source_urls()
    }
    fn get_source_reference(&self) -> Option<&str> {
        self.inner.get_source_reference()
    }
    fn get_source_mirrors(&self) -> Option<Vec<Mirror>> {
        self.inner.get_source_mirrors()
    }
    fn set_source_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.inner.set_source_mirrors(mirrors);
    }
    fn get_dist_type(&self) -> Option<&str> {
        self.inner.get_dist_type()
    }
    fn get_dist_url(&self) -> Option<&str> {
        self.inner.get_dist_url()
    }
    fn get_dist_urls(&self) -> Vec<String> {
        self.inner.get_dist_urls()
    }
    fn get_dist_reference(&self) -> Option<&str> {
        self.inner.get_dist_reference()
    }
    fn get_dist_sha1_checksum(&self) -> Option<&str> {
        self.inner.get_dist_sha1_checksum()
    }
    fn get_dist_mirrors(&self) -> Option<Vec<Mirror>> {
        self.inner.get_dist_mirrors()
    }
    fn set_dist_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.inner.set_dist_mirrors(mirrors);
    }
    fn get_version(&self) -> &str {
        self.inner.get_version()
    }
    fn get_pretty_version(&self) -> &str {
        self.inner.get_pretty_version()
    }
    fn get_full_pretty_version(&self, truncate: bool, display_mode: DisplayMode) -> String {
        self.inner.get_full_pretty_version(truncate, display_mode)
    }
    fn get_release_date(&self) -> Option<DateTime<Utc>> {
        self.inner.get_release_date()
    }
    fn get_stability(&self) -> &str {
        self.inner.get_stability()
    }
    fn get_requires(&self) -> IndexMap<String, Link> {
        self.inner.get_requires()
    }
    fn get_conflicts(&self) -> IndexMap<String, Link> {
        self.inner.get_conflicts()
    }
    fn get_provides(&self) -> IndexMap<String, Link> {
        self.inner.get_provides()
    }
    fn get_replaces(&self) -> IndexMap<String, Link> {
        self.inner.get_replaces()
    }
    fn get_dev_requires(&self) -> IndexMap<String, Link> {
        self.inner.get_dev_requires()
    }
    fn get_suggests(&self) -> IndexMap<String, String> {
        self.inner.get_suggests()
    }
    fn get_autoload(&self) -> IndexMap<String, PhpMixed> {
        self.inner.get_autoload()
    }
    fn get_dev_autoload(&self) -> IndexMap<String, PhpMixed> {
        self.inner.get_dev_autoload()
    }
    fn get_include_paths(&self) -> Vec<String> {
        self.inner.get_include_paths()
    }
    fn get_php_ext(&self) -> Option<IndexMap<String, PhpMixed>> {
        self.inner.get_php_ext()
    }
    fn set_repository(&mut self, repository: RepositoryInterfaceHandle) -> anyhow::Result<()> {
        self.inner.set_repository(repository)
    }
    fn get_repository(&self) -> Option<RepositoryInterfaceHandle> {
        self.inner.get_repository()
    }
    fn get_binaries(&self) -> Vec<String> {
        self.inner.get_binaries()
    }
    fn get_unique_name(&self) -> String {
        self.inner.get_unique_name()
    }
    fn get_notification_url(&self) -> Option<&str> {
        self.inner.get_notification_url()
    }
    fn get_pretty_string(&self) -> String {
        self.inner.get_pretty_string()
    }
    fn is_default_branch(&self) -> bool {
        self.inner.is_default_branch()
    }
    fn get_transport_options(&self) -> IndexMap<String, PhpMixed> {
        self.inner.get_transport_options()
    }
    fn set_transport_options(&mut self, options: IndexMap<String, PhpMixed>) {
        self.inner.set_transport_options(options);
    }
    fn set_source_reference(&mut self, reference: Option<String>) {
        self.inner.set_source_reference(reference);
    }
    fn set_source_url(&mut self, url: Option<String>) {
        self.inner.set_source_url(url);
    }
    fn set_dist_url(&mut self, url: Option<String>) {
        self.inner.set_dist_url(url);
    }
    fn set_dist_type(&mut self, r#type: Option<String>) {
        self.inner.set_dist_type(r#type);
    }
    fn set_dist_reference(&mut self, reference: Option<String>) {
        self.inner.set_dist_reference(reference);
    }
    fn set_source_dist_references(&mut self, reference: &str) {
        self.inner.set_source_dist_references(reference);
    }
}
