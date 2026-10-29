//! ref: composer/src/Composer/Package/CompletePackage.php

use crate::package::{CompletePackageInterface, DisplayMode, Mirror, Package, PackageInterface};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug, Clone)]
pub struct CompletePackage {
    pub(crate) inner: Package,
    repositories: IndexMap<BString, PhpMixed>,
    license: Vec<BString>,
    keywords: Vec<BString>,
    authors: Vec<IndexMap<BString, BString>>,
    description: Option<BString>,
    homepage: Option<BString>,
    scripts: IndexMap<BString, Vec<BString>>,
    support: IndexMap<BString, BString>,
    funding: Vec<IndexMap<BString, PhpMixed>>,
    abandoned: PhpMixed,
    archive_name: Option<BString>,
    archive_excludes: Vec<BString>,
}

impl CompletePackage {
    pub fn new(name: BString, version: BString, pretty_version: BString) -> Self {
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

    pub fn replace_version(&mut self, version: BString, pretty_version: BString) {
        self.inner.replace_version(version, pretty_version);
    }

    pub fn set_release_date(&mut self, release_date: Option<chrono::DateTime<chrono::Utc>>) {
        self.inner.set_release_date(release_date);
    }
}

impl CompletePackageInterface for CompletePackage {
    fn set_scripts(&mut self, scripts: IndexMap<BString, Vec<BString>>) {
        self.scripts = scripts;
    }

    fn get_scripts(&self) -> IndexMap<BString, Vec<BString>> {
        self.scripts.clone()
    }

    fn set_repositories(&mut self, repositories: IndexMap<BString, PhpMixed>) {
        self.repositories = repositories;
    }

    fn get_repositories(&self) -> IndexMap<BString, PhpMixed> {
        self.repositories.clone()
    }

    fn set_license(&mut self, license: Vec<BString>) {
        self.license = license;
    }

    fn get_license(&self) -> Vec<BString> {
        self.license.clone()
    }

    fn set_keywords(&mut self, keywords: Vec<BString>) {
        self.keywords = keywords;
    }

    fn get_keywords(&self) -> Vec<BString> {
        self.keywords.clone()
    }

    fn set_authors(&mut self, authors: Vec<IndexMap<BString, BString>>) {
        self.authors = authors;
    }

    fn get_authors(&self) -> Vec<IndexMap<BString, BString>> {
        self.authors.clone()
    }

    fn set_description(&mut self, description: BString) {
        self.description = Some(description);
    }

    fn get_description(&self) -> Option<BString> {
        self.description.clone()
    }

    fn set_homepage(&mut self, homepage: BString) {
        self.homepage = Some(homepage);
    }

    fn get_homepage(&self) -> Option<BString> {
        self.homepage.clone()
    }

    fn set_support(&mut self, support: IndexMap<BString, BString>) {
        self.support = support;
    }

    fn get_support(&self) -> IndexMap<BString, BString> {
        self.support.clone()
    }

    fn set_funding(&mut self, funding: Vec<IndexMap<BString, PhpMixed>>) {
        self.funding = funding;
    }

    fn get_funding(&self) -> Vec<IndexMap<BString, PhpMixed>> {
        self.funding.clone()
    }

    fn is_abandoned(&self) -> bool {
        self.abandoned.to_bool()
    }

    fn set_abandoned(&mut self, abandoned: PhpMixed) {
        self.abandoned = abandoned;
    }

    fn get_replacement_package(&self) -> Option<BString> {
        match &self.abandoned {
            PhpMixed::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn set_archive_name(&mut self, name: BString) {
        self.archive_name = Some(name);
    }

    fn get_archive_name(&self) -> Option<BString> {
        self.archive_name.clone()
    }

    fn set_archive_excludes(&mut self, excludes: Vec<BString>) {
        self.archive_excludes = excludes;
    }

    fn get_archive_excludes(&self) -> Vec<BString> {
        self.archive_excludes.clone()
    }

    fn as_package_interface(&self) -> &dyn PackageInterface {
        self
    }
}

impl PackageInterface for CompletePackage {
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
        self.inner.get_extra().clone()
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
        self.inner.get_source_mirrors().cloned()
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
        self.inner.get_dist_mirrors().cloned()
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

    fn get_release_date(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.inner.get_release_date().copied()
    }

    fn get_stability(&self) -> &BStr {
        self.inner.get_stability()
    }

    fn get_requires(&self) -> std::rc::Rc<IndexMap<BString, super::Link>> {
        PackageInterface::get_requires(&self.inner)
    }

    fn get_conflicts(&self) -> std::rc::Rc<IndexMap<BString, super::Link>> {
        PackageInterface::get_conflicts(&self.inner)
    }

    fn get_provides(&self) -> std::rc::Rc<IndexMap<BString, super::Link>> {
        PackageInterface::get_provides(&self.inner)
    }

    fn get_replaces(&self) -> std::rc::Rc<IndexMap<BString, super::Link>> {
        PackageInterface::get_replaces(&self.inner)
    }

    fn get_dev_requires(&self) -> std::rc::Rc<IndexMap<BString, super::Link>> {
        PackageInterface::get_dev_requires(&self.inner)
    }

    fn get_suggests(&self) -> IndexMap<BString, BString> {
        self.inner.get_suggests().clone()
    }

    fn get_autoload(&self) -> IndexMap<BString, PhpMixed> {
        self.inner.get_autoload().clone()
    }

    fn get_dev_autoload(&self) -> IndexMap<BString, PhpMixed> {
        self.inner.get_dev_autoload().clone()
    }

    fn get_include_paths(&self) -> Vec<BString> {
        self.inner.get_include_paths().clone()
    }

    fn get_php_ext(&self) -> Option<IndexMap<BString, PhpMixed>> {
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

    fn get_binaries(&self) -> Vec<BString> {
        self.inner.get_binaries().clone()
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
        self.inner.get_transport_options().clone()
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
