//! ref: composer/src/Composer/Package/Package.php

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::composer::util::composer_mirror::ComposerMirror;
use shirabe_php_shim::{E_USER_DEPRECATED, PhpMixed, strpos, trigger_error};

use crate::package::base_package::BasePackage;
use crate::package::link::Link;
use crate::package::version::version_parser::VersionParser;
use crate::repository::repository_interface::RepositoryInterface;

/// Mirror entry, e.g. `['url' => 'https://...', 'preferred' => true]`.
#[derive(Debug, Clone)]
pub struct Mirror {
    pub url: String,
    pub preferred: bool,
}

/// Core package definitions that are needed to resolve dependencies and install packages
#[derive(Debug)]
pub struct Package {
    id: i64,
    name: String,
    pretty_name: String,
    repository: Option<Box<dyn RepositoryInterface>>,

    pub(crate) r#type: Option<String>,
    pub(crate) target_dir: Option<String>,
    /// `'source'` | `'dist'` | `null`
    pub(crate) installation_source: Option<String>,
    pub(crate) source_type: Option<String>,
    pub(crate) source_url: Option<String>,
    pub(crate) source_reference: Option<String>,
    pub(crate) source_mirrors: Option<Vec<Mirror>>,
    pub(crate) dist_type: Option<String>,
    pub(crate) dist_url: Option<String>,
    pub(crate) dist_reference: Option<String>,
    pub(crate) dist_sha1_checksum: Option<String>,
    pub(crate) dist_mirrors: Option<Vec<Mirror>>,
    pub(crate) version: String,
    pub(crate) pretty_version: String,
    pub(crate) release_date: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) extra: IndexMap<String, PhpMixed>,
    pub(crate) binaries: Vec<String>,
    pub(crate) dev: bool,
    /// `'stable'` | `'RC'` | `'beta'` | `'alpha'` | `'dev'`
    pub(crate) stability: String,
    pub(crate) notification_url: Option<String>,

    pub(crate) requires: IndexMap<String, Link>,
    pub(crate) conflicts: IndexMap<String, Link>,
    pub(crate) provides: IndexMap<String, Link>,
    pub(crate) replaces: IndexMap<String, Link>,
    pub(crate) dev_requires: IndexMap<String, Link>,
    pub(crate) suggests: IndexMap<String, String>,
    pub(crate) autoload: IndexMap<String, PhpMixed>,
    pub(crate) dev_autoload: IndexMap<String, PhpMixed>,
    pub(crate) include_paths: Vec<String>,
    pub(crate) is_default_branch: bool,
    pub(crate) transport_options: IndexMap<String, PhpMixed>,
    pub(crate) php_ext: Option<IndexMap<String, PhpMixed>>,
}

impl Package {
    /// Creates a new in memory package.
    pub fn new(name: String, version: String, pretty_version: String) -> Self {
        let stability = VersionParser::parse_stability(&version).to_string();
        let dev = stability == "dev";
        Self {
            inner: BasePackage::new(name),
            r#type: None,
            target_dir: None,
            installation_source: None,
            source_type: None,
            source_url: None,
            source_reference: None,
            source_mirrors: None,
            dist_type: None,
            dist_url: None,
            dist_reference: None,
            dist_sha1_checksum: None,
            dist_mirrors: None,
            version,
            pretty_version,
            release_date: None,
            extra: IndexMap::new(),
            binaries: Vec::new(),
            dev,
            stability,
            notification_url: None,
            requires: IndexMap::new(),
            conflicts: IndexMap::new(),
            provides: IndexMap::new(),
            replaces: IndexMap::new(),
            dev_requires: IndexMap::new(),
            suggests: IndexMap::new(),
            autoload: IndexMap::new(),
            dev_autoload: IndexMap::new(),
            include_paths: Vec::new(),
            is_default_branch: false,
            transport_options: IndexMap::new(),
            php_ext: None,
        }
    }

    pub fn is_dev(&self) -> bool {
        self.dev
    }

    pub fn set_type(&mut self, r#type: String) {
        self.r#type = Some(r#type);
    }

    pub fn get_type(&self) -> String {
        self.r#type
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "library".to_string())
    }

    pub fn get_stability(&self) -> &str {
        &self.stability
    }

    pub fn set_target_dir(&mut self, target_dir: Option<String>) {
        self.target_dir = target_dir;
    }

    pub fn get_target_dir(&self) -> Option<String> {
        let target_dir = self.target_dir.as_ref()?;

        let replaced = Preg::replace(
            "{ (?:^|[\\\\/]+) \\.\\.? (?:[\\\\/]+|$) (?:\\.\\.? (?:[\\\\/]+|$) )*}x",
            "/",
            target_dir,
        )
        .unwrap_or_else(|_| target_dir.clone());
        Some(replaced.trim_start_matches('/').to_string())
    }

    pub fn set_extra(&mut self, extra: IndexMap<String, PhpMixed>) {
        self.extra = extra;
    }

    pub fn get_extra(&self) -> &IndexMap<String, PhpMixed> {
        &self.extra
    }

    pub fn set_binaries(&mut self, binaries: Vec<String>) {
        self.binaries = binaries;
    }

    pub fn get_binaries(&self) -> &Vec<String> {
        &self.binaries
    }

    pub fn set_installation_source(&mut self, r#type: Option<String>) {
        self.installation_source = r#type;
    }

    pub fn get_installation_source(&self) -> Option<&str> {
        self.installation_source.as_deref()
    }

    pub fn set_source_type(&mut self, r#type: Option<String>) {
        self.source_type = r#type;
    }

    pub fn get_source_type(&self) -> Option<&str> {
        self.source_type.as_deref()
    }

    pub fn set_source_url(&mut self, url: Option<String>) {
        self.source_url = url;
    }

    pub fn get_source_url(&self) -> Option<&str> {
        self.source_url.as_deref()
    }

    pub fn set_source_reference(&mut self, reference: Option<String>) {
        self.source_reference = reference;
    }

    pub fn get_source_reference(&self) -> Option<&str> {
        self.source_reference.as_deref()
    }

    pub fn set_source_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.source_mirrors = mirrors;
    }

    pub fn get_source_mirrors(&self) -> Option<&Vec<Mirror>> {
        self.source_mirrors.as_ref()
    }

    pub fn get_source_urls(&self) -> Vec<String> {
        self.get_urls(
            self.source_url.as_deref(),
            self.source_mirrors.as_ref(),
            self.source_reference.as_deref(),
            self.source_type.as_deref(),
            "source",
        )
    }

    pub fn set_dist_type(&mut self, r#type: Option<String>) {
        self.dist_type = match r#type.as_deref() {
            Some("") => None,
            _ => r#type,
        };
    }

    pub fn get_dist_type(&self) -> Option<&str> {
        self.dist_type.as_deref()
    }

    pub fn set_dist_url(&mut self, url: Option<String>) {
        self.dist_url = match url.as_deref() {
            Some("") => None,
            _ => url,
        };
    }

    pub fn get_dist_url(&self) -> Option<&str> {
        self.dist_url.as_deref()
    }

    pub fn set_dist_reference(&mut self, reference: Option<String>) {
        self.dist_reference = reference;
    }

    pub fn get_dist_reference(&self) -> Option<&str> {
        self.dist_reference.as_deref()
    }

    pub fn set_dist_sha1_checksum(&mut self, sha1checksum: Option<String>) {
        self.dist_sha1_checksum = sha1checksum;
    }

    pub fn get_dist_sha1_checksum(&self) -> Option<&str> {
        self.dist_sha1_checksum.as_deref()
    }

    pub fn set_dist_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.dist_mirrors = mirrors;
    }

    pub fn get_dist_mirrors(&self) -> Option<&Vec<Mirror>> {
        self.dist_mirrors.as_ref()
    }

    pub fn get_dist_urls(&self) -> Vec<String> {
        self.get_urls(
            self.dist_url.as_deref(),
            self.dist_mirrors.as_ref(),
            self.dist_reference.as_deref(),
            self.dist_type.as_deref(),
            "dist",
        )
    }

    pub fn get_transport_options(&self) -> &IndexMap<String, PhpMixed> {
        &self.transport_options
    }

    pub fn set_transport_options(&mut self, options: IndexMap<String, PhpMixed>) {
        self.transport_options = options;
    }

    pub fn get_version(&self) -> &str {
        &self.version
    }

    pub fn get_pretty_version(&self) -> &str {
        &self.pretty_version
    }

    pub fn set_release_date(&mut self, release_date: Option<chrono::DateTime<chrono::Utc>>) {
        self.release_date = release_date;
    }

    pub fn get_release_date(&self) -> Option<&chrono::DateTime<chrono::Utc>> {
        self.release_date.as_ref()
    }

    /// Set the required packages
    pub fn set_requires(&mut self, mut requires: IndexMap<String, Link>) {
        if requires.contains_key("0") {
            requires = self.convert_links_to_map(requires, "setRequires");
        }

        self.requires = requires;
    }

    pub fn get_requires(&self) -> &IndexMap<String, Link> {
        &self.requires
    }

    pub fn set_conflicts(&mut self, mut conflicts: IndexMap<String, Link>) {
        if conflicts.contains_key("0") {
            conflicts = self.convert_links_to_map(conflicts, "setConflicts");
        }

        self.conflicts = conflicts;
    }

    pub fn get_conflicts(&self) -> &IndexMap<String, Link> {
        &self.conflicts
    }

    pub fn set_provides(&mut self, mut provides: IndexMap<String, Link>) {
        if provides.contains_key("0") {
            provides = self.convert_links_to_map(provides, "setProvides");
        }

        self.provides = provides;
    }

    pub fn get_provides(&self) -> &IndexMap<String, Link> {
        &self.provides
    }

    pub fn set_replaces(&mut self, mut replaces: IndexMap<String, Link>) {
        if replaces.contains_key("0") {
            replaces = self.convert_links_to_map(replaces, "setReplaces");
        }

        self.replaces = replaces;
    }

    pub fn get_replaces(&self) -> &IndexMap<String, Link> {
        &self.replaces
    }

    pub fn set_dev_requires(&mut self, mut dev_requires: IndexMap<String, Link>) {
        if dev_requires.contains_key("0") {
            dev_requires = self.convert_links_to_map(dev_requires, "setDevRequires");
        }

        self.dev_requires = dev_requires;
    }

    pub fn get_dev_requires(&self) -> &IndexMap<String, Link> {
        &self.dev_requires
    }

    pub fn set_suggests(&mut self, suggests: IndexMap<String, String>) {
        self.suggests = suggests;
    }

    pub fn get_suggests(&self) -> &IndexMap<String, String> {
        &self.suggests
    }

    pub fn set_autoload(&mut self, autoload: IndexMap<String, PhpMixed>) {
        self.autoload = autoload;
    }

    pub fn get_autoload(&self) -> &IndexMap<String, PhpMixed> {
        &self.autoload
    }

    pub fn set_dev_autoload(&mut self, dev_autoload: IndexMap<String, PhpMixed>) {
        self.dev_autoload = dev_autoload;
    }

    pub fn get_dev_autoload(&self) -> &IndexMap<String, PhpMixed> {
        &self.dev_autoload
    }

    pub fn set_include_paths(&mut self, include_paths: Vec<String>) {
        self.include_paths = include_paths;
    }

    pub fn get_include_paths(&self) -> &Vec<String> {
        &self.include_paths
    }

    pub fn set_php_ext(&mut self, php_ext: Option<IndexMap<String, PhpMixed>>) {
        self.php_ext = php_ext;
    }

    pub fn get_php_ext(&self) -> Option<&IndexMap<String, PhpMixed>> {
        self.php_ext.as_ref()
    }

    pub fn set_notification_url(&mut self, notification_url: String) {
        self.notification_url = Some(notification_url);
    }

    pub fn get_notification_url(&self) -> Option<&str> {
        self.notification_url.as_deref()
    }

    pub fn set_is_default_branch(&mut self, default_branch: bool) {
        self.is_default_branch = default_branch;
    }

    pub fn is_default_branch(&self) -> bool {
        self.is_default_branch
    }

    pub fn set_source_dist_references(&mut self, reference: String) {
        self.set_source_reference(Some(reference.clone()));

        // only bitbucket, github and gitlab have auto generated dist URLs that easily allow replacing the reference in the dist URL
        // TODO generalize this a bit for self-managed/on-prem versions? Some kind of replace token in dist urls which allow this?
        if self.get_dist_url().is_some()
            && Preg::is_match(
                "{^https?://(?:(?:www\\.)?bitbucket\\.org|(api\\.)?github\\.com|(?:www\\.)?gitlab\\.com)/}i",
                self.get_dist_url().unwrap_or(""),
            )
            .unwrap_or(false)
        {
            self.set_dist_reference(Some(reference.clone()));
            self.set_dist_url(Some(
                Preg::replace(
                    "{(?<=/|sha=)[a-f0-9]{40}(?=/|$)}i",
                    &reference,
                    self.get_dist_url().unwrap_or(""),
                )
                .unwrap_or_default(),
            ));
        } else if self.get_dist_reference().is_some() {
            // update the dist reference if there was one, but if none was provided ignore it
            self.set_dist_reference(Some(reference));
        }
    }

    /// Replaces current version and pretty version with passed values.
    /// It also sets stability.
    pub fn replace_version(&mut self, version: String, pretty_version: String) {
        self.version = version;
        self.pretty_version = pretty_version;

        self.stability = VersionParser::parse_stability(&self.version).to_string();
        self.dev = self.stability == "dev";
    }

    fn get_urls(
        &self,
        url: Option<&str>,
        mirrors: Option<&Vec<Mirror>>,
        r#ref: Option<&str>,
        r#type: Option<&str>,
        url_type: &str,
    ) -> Vec<String> {
        let url = match url {
            Some(u) if !u.is_empty() => u,
            _ => return Vec::new(),
        };

        let url = if url_type == "dist" && strpos(url, "%").is_some() {
            ComposerMirror::process_url(
                url,
                &self.inner.name,
                &self.version,
                r#ref.unwrap_or(""),
                r#type.unwrap_or(""),
                &self.pretty_version,
            )
        } else {
            url.to_string()
        };

        let mut urls: Vec<String> = vec![url.clone()];
        if let Some(mirrors) = mirrors {
            for mirror in mirrors {
                let mirror_url = if url_type == "dist" {
                    ComposerMirror::process_url(
                        &mirror.url,
                        &self.inner.name,
                        &self.version,
                        r#ref.unwrap_or(""),
                        r#type.unwrap_or(""),
                        &self.pretty_version,
                    )
                } else if url_type == "source" && r#type == Some("git") {
                    ComposerMirror::process_git_url(
                        &mirror.url,
                        &self.inner.name,
                        &url,
                        r#type.unwrap_or(""),
                    )
                } else if url_type == "source" && r#type == Some("hg") {
                    ComposerMirror::process_hg_url(
                        &mirror.url,
                        &self.inner.name,
                        &url,
                        r#type.unwrap_or(""),
                    )
                } else {
                    continue;
                };
                if !urls.contains(&mirror_url) {
                    if mirror.preferred {
                        urls.insert(0, mirror_url);
                    } else {
                        urls.push(mirror_url);
                    }
                }
            }
        }

        urls
    }

    fn convert_links_to_map(
        &self,
        links: IndexMap<String, Link>,
        source: &str,
    ) -> IndexMap<String, Link> {
        trigger_error(
            &format!(
                "Package::{} must be called with a map of lowercased package name => Link object, got a indexed array, this is deprecated and you should fix your usage.",
                source
            ),
            E_USER_DEPRECATED,
        );
        let mut new_links: IndexMap<String, Link> = IndexMap::new();
        for (_k, link) in links {
            new_links.insert(link.get_target().to_string(), link);
        }

        new_links
    }
}

impl BasePackage for Package {
    fn id(&self) -> i64 {
        self.id
    }

    fn id_mut(&mut self) -> &mut i64 {
        &mut self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    fn pretty_name(&self) -> &str {
        &self.pretty_name
    }

    fn pretty_name_mut(&mut self) -> &mut String {
        &mut self.pretty_name
    }

    fn repository_opt(&self) -> Option<&dyn RepositoryInterface> {
        self.repository.as_ref()
    }

    fn set_repository_box(&mut self, repository: Box<dyn RepositoryInterface>) {
        todo!()
    }

    fn take_repository(&mut self) -> Option<Box<dyn RepositoryInterface>> {
        todo!()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        todo!()
    }

    fn clone_box(&self) -> Box<dyn BasePackage> {
        todo!()
    }
}
