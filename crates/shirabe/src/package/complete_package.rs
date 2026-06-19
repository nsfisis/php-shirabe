//! ref: composer/src/Composer/Package/CompletePackage.php

use crate::package::CompletePackageInterface;
use crate::package::DisplayMode;
use crate::package::Mirror;
use crate::package::Package;
use crate::package::PackageInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug, Clone)]
pub struct CompletePackage {
    pub(crate) inner: Package,
    pub(crate) repositories: IndexMap<String, PhpMixed>,
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

impl CompletePackage {
    pub fn new(name: String, version: String, pretty_version: String) -> Self {
        Self {
            inner: crate::package::Package::new(name, version, pretty_version),
            repositories: IndexMap::new(),
            license: Vec::new(),
            keywords: Vec::new(),
            authors: Vec::new(),
            description: None,
            homepage: None,
            scripts: IndexMap::new(),
            support: IndexMap::new(),
            funding: Vec::new(),
            abandoned: PhpMixed::Bool(false),
            archive_name: None,
            archive_excludes: Vec::new(),
        }
    }

    pub fn replace_version(&mut self, version: String, pretty_version: String) {
        self.inner.replace_version(version, pretty_version);
    }
}

impl CompletePackageInterface for CompletePackage {
    fn set_scripts(&mut self, scripts: IndexMap<String, Vec<String>>) {
        self.scripts = scripts;
    }

    fn get_scripts(&self) -> IndexMap<String, Vec<String>> {
        self.scripts.clone()
    }

    fn set_repositories(&mut self, repositories: IndexMap<String, PhpMixed>) {
        self.repositories = repositories;
    }

    fn get_repositories(&self) -> IndexMap<String, PhpMixed> {
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

    fn as_package_interface(&self) -> &dyn PackageInterface {
        self
    }
}

impl PackageInterface for CompletePackage {
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

    fn get_type(&self) -> &str {
        PackageInterface::get_type(&self.inner)
    }

    fn get_target_dir(&self) -> Option<String> {
        self.inner.get_target_dir()
    }

    fn get_extra(&self) -> IndexMap<String, PhpMixed> {
        self.inner.get_extra().clone()
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
        self.inner.get_source_mirrors().cloned()
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
        self.inner.get_dist_mirrors().cloned()
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
        PackageInterface::get_full_pretty_version(&self.inner, truncate, display_mode)
    }

    fn get_release_date(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.inner.get_release_date().copied()
    }

    fn get_stability(&self) -> &str {
        self.inner.get_stability()
    }

    fn get_requires(&self) -> IndexMap<String, super::Link> {
        self.inner.get_requires().clone()
    }

    fn get_conflicts(&self) -> IndexMap<String, super::Link> {
        self.inner.get_conflicts().clone()
    }

    fn get_provides(&self) -> IndexMap<String, super::Link> {
        self.inner.get_provides().clone()
    }

    fn get_replaces(&self) -> IndexMap<String, super::Link> {
        self.inner.get_replaces().clone()
    }

    fn get_dev_requires(&self) -> IndexMap<String, super::Link> {
        self.inner.get_dev_requires().clone()
    }

    fn get_suggests(&self) -> IndexMap<String, String> {
        self.inner.get_suggests().clone()
    }

    fn get_autoload(&self) -> IndexMap<String, PhpMixed> {
        self.inner.get_autoload().clone()
    }

    fn get_dev_autoload(&self) -> IndexMap<String, PhpMixed> {
        self.inner.get_dev_autoload().clone()
    }

    fn get_include_paths(&self) -> Vec<String> {
        self.inner.get_include_paths().clone()
    }

    fn get_php_ext(&self) -> Option<IndexMap<String, PhpMixed>> {
        self.inner.get_php_ext().cloned()
    }

    fn set_repository(
        &mut self,
        repository: crate::repository::RepositoryInterfaceHandle,
    ) -> anyhow::Result<()> {
        self.inner.set_repository(repository)
    }

    fn get_repository(&self) -> Option<crate::repository::RepositoryInterfaceHandle> {
        self.inner.get_repository()
    }

    fn get_binaries(&self) -> Vec<String> {
        self.inner.get_binaries().clone()
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
        self.inner.get_transport_options().clone()
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
        PackageInterface::set_source_dist_references(&mut self.inner, reference);
    }
}

impl std::fmt::Display for CompletePackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_unique_name())
    }
}
