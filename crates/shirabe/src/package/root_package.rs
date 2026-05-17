//! ref: composer/src/Composer/Package/RootPackage.php

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::package::complete_package::CompletePackage;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::link::Link;
use crate::package::package_interface::PackageInterface;
use crate::package::root_package_interface::RootPackageInterface;
use crate::repository::repository_interface::RepositoryInterface;

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

impl std::fmt::Display for RootPackage {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl PackageInterface for RootPackage {
    fn get_name(&self) -> &str {
        todo!()
    }
    fn get_pretty_name(&self) -> &str {
        todo!()
    }
    fn get_names(&self, _provides: bool) -> Vec<String> {
        todo!()
    }
    fn set_id(&mut self, _id: i64) {
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
    fn set_installation_source(&mut self, _type: Option<String>) {
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
    fn set_source_mirrors(&mut self, _mirrors: Option<Vec<IndexMap<String, PhpMixed>>>) {
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
    fn set_dist_mirrors(&mut self, _mirrors: Option<Vec<IndexMap<String, PhpMixed>>>) {
        todo!()
    }
    fn get_version(&self) -> &str {
        todo!()
    }
    fn get_pretty_version(&self) -> &str {
        todo!()
    }
    fn get_full_pretty_version(&self, _truncate: bool, _display_mode: i64) -> String {
        todo!()
    }
    fn get_release_date(&self) -> Option<DateTime<Utc>> {
        todo!()
    }
    fn get_stability(&self) -> &str {
        todo!()
    }
    fn get_requires(&self) -> IndexMap<String, Link> {
        todo!()
    }
    fn get_conflicts(&self) -> Vec<Link> {
        todo!()
    }
    fn get_provides(&self) -> Vec<Link> {
        todo!()
    }
    fn get_replaces(&self) -> Vec<Link> {
        todo!()
    }
    fn get_dev_requires(&self) -> IndexMap<String, Link> {
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
    fn set_repository(&mut self, _repository: Box<dyn RepositoryInterface>) -> anyhow::Result<()> {
        todo!()
    }
    fn get_repository(&self) -> Option<&dyn RepositoryInterface> {
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
    fn set_transport_options(&mut self, _options: IndexMap<String, PhpMixed>) {
        todo!()
    }
    fn set_source_reference(&mut self, _reference: Option<String>) {
        todo!()
    }
    fn set_dist_url(&mut self, _url: Option<String>) {
        todo!()
    }
    fn set_dist_type(&mut self, _type: Option<String>) {
        todo!()
    }
    fn set_dist_reference(&mut self, _reference: Option<String>) {
        todo!()
    }
    fn set_source_dist_references(&mut self, _reference: &str) {
        todo!()
    }
}
