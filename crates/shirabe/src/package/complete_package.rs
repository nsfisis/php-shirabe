//! ref: composer/src/Composer/Package/CompletePackage.php

use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::package::Package;
use crate::package::package_interface::PackageInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

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

impl PackageInterface for CompletePackage {
    fn get_name(&self) -> &str {
        todo!()
    }

    fn get_pretty_name(&self) -> &str {
        todo!()
    }

    fn get_names(&self, provides: bool) -> Vec<String> {
        todo!()
    }

    fn set_id(&mut self, id: i64) {
        todo!()
    }

    fn get_id(&self) -> i64 {
        todo!()
    }

    fn is_dev(&self) -> bool {
        todo!()
    }

    fn get_type(&self) -> &str {
        todo!()
    }

    fn get_target_dir(&self) -> Option<&str> {
        todo!()
    }

    fn get_extra(&self) -> IndexMap<String, PhpMixed> {
        todo!()
    }

    fn set_installation_source(&mut self, r#type: Option<String>) {
        todo!()
    }

    fn get_installation_source(&self) -> Option<&str> {
        todo!()
    }

    fn get_source_type(&self) -> Option<&str> {
        todo!()
    }

    fn get_source_url(&self) -> Option<&str> {
        todo!()
    }

    fn get_source_urls(&self) -> Vec<String> {
        todo!()
    }

    fn get_source_reference(&self) -> Option<&str> {
        todo!()
    }

    fn get_source_mirrors(&self) -> Option<Vec<IndexMap<String, PhpMixed>>> {
        todo!()
    }

    fn set_source_mirrors(&mut self, mirrors: Option<Vec<IndexMap<String, PhpMixed>>>) {
        todo!()
    }

    fn get_dist_type(&self) -> Option<&str> {
        todo!()
    }

    fn get_dist_url(&self) -> Option<&str> {
        todo!()
    }

    fn get_dist_urls(&self) -> Vec<String> {
        todo!()
    }

    fn get_dist_reference(&self) -> Option<&str> {
        todo!()
    }

    fn get_dist_sha1_checksum(&self) -> Option<&str> {
        todo!()
    }

    fn get_dist_mirrors(&self) -> Option<Vec<IndexMap<String, PhpMixed>>> {
        todo!()
    }

    fn set_dist_mirrors(&mut self, mirrors: Option<Vec<IndexMap<String, PhpMixed>>>) {
        todo!()
    }

    fn get_version(&self) -> &str {
        todo!()
    }

    fn get_pretty_version(&self) -> &str {
        todo!()
    }

    fn get_full_pretty_version(&self, truncate: bool, display_mode: i64) -> String {
        todo!()
    }

    fn get_release_date(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        todo!()
    }

    fn get_stability(&self) -> &str {
        todo!()
    }

    fn get_requires(&self) -> IndexMap<String, super::link::Link> {
        todo!()
    }

    fn get_conflicts(&self) -> Vec<super::link::Link> {
        todo!()
    }

    fn get_provides(&self) -> Vec<super::link::Link> {
        todo!()
    }

    fn get_replaces(&self) -> Vec<super::link::Link> {
        todo!()
    }

    fn get_dev_requires(&self) -> IndexMap<String, super::link::Link> {
        todo!()
    }

    fn get_suggests(&self) -> IndexMap<String, String> {
        todo!()
    }

    fn get_autoload(&self) -> IndexMap<String, PhpMixed> {
        todo!()
    }

    fn get_dev_autoload(&self) -> IndexMap<String, PhpMixed> {
        todo!()
    }

    fn get_include_paths(&self) -> Vec<String> {
        todo!()
    }

    fn get_php_ext(&self) -> Option<IndexMap<String, PhpMixed>> {
        todo!()
    }

    fn set_repository(
        &mut self,
        repository: Box<dyn crate::repository::repository_interface::RepositoryInterface>,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn get_repository(
        &self,
    ) -> Option<&dyn crate::repository::repository_interface::RepositoryInterface> {
        todo!()
    }

    fn get_binaries(&self) -> Vec<String> {
        todo!()
    }

    fn get_unique_name(&self) -> String {
        todo!()
    }

    fn get_notification_url(&self) -> Option<&str> {
        todo!()
    }

    fn get_pretty_string(&self) -> String {
        todo!()
    }

    fn is_default_branch(&self) -> bool {
        todo!()
    }

    fn get_transport_options(&self) -> IndexMap<String, PhpMixed> {
        todo!()
    }

    fn set_transport_options(&mut self, options: IndexMap<String, PhpMixed>) {
        todo!()
    }

    fn set_source_reference(&mut self, reference: Option<String>) {
        todo!()
    }

    fn set_dist_url(&mut self, url: Option<String>) {
        todo!()
    }

    fn set_dist_type(&mut self, r#type: Option<String>) {
        todo!()
    }

    fn set_dist_reference(&mut self, reference: Option<String>) {
        todo!()
    }

    fn set_source_dist_references(&mut self, reference: &str) {
        todo!()
    }
}
