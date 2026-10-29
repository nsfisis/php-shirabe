//! ref: composer/src/Composer/Package/Package.php

use crate::package::version::VersionParser;
use crate::package::{BasePackage, DisplayMode, Link, PackageInterface};
use crate::repository::{RepositoryInterfaceHandle, RepositoryInterfaceWeakHandle};
use crate::util::ComposerMirror;
use bstr::{BStr, BString, ByteSlice};
use chrono::{DateTime, Utc};
use indexmap::{IndexMap, IndexSet};
use shirabe_php_shim::{
    E_USER_DEPRECATED, LogicException, PhpMixed, PregMatches, bconcat, ltrim2, php_regex,
    preg_is_match, preg_replace, preg_replace_callback, strpos, strtolower, trigger_error,
};

/// Mirror entry, e.g. `['url' => 'https://...', 'preferred' => true]`.
#[derive(Debug, Clone)]
pub struct Mirror {
    pub url: BString,
    pub preferred: bool,
}

/// Core package definitions that are needed to resolve dependencies and install packages
#[derive(Debug, Clone)]
pub struct Package {
    id: i64,
    name: BString,
    pretty_name: BString,
    /// Back-reference to the owning repository. `Weak` breaks the repository -> packages cycle.
    repository: Option<RepositoryInterfaceWeakHandle>,

    r#type: Option<BString>,
    target_dir: Option<BString>,
    /// `'source'` | `'dist'` | `null`
    installation_source: Option<BString>,
    source_type: Option<BString>,
    source_url: Option<BString>,
    source_reference: Option<BString>,
    source_mirrors: Option<Vec<Mirror>>,
    dist_type: Option<BString>,
    dist_url: Option<BString>,
    dist_reference: Option<BString>,
    dist_sha1_checksum: Option<BString>,
    dist_mirrors: Option<Vec<Mirror>>,
    version: BString,
    pretty_version: BString,
    release_date: Option<chrono::DateTime<chrono::Utc>>,
    extra: IndexMap<BString, PhpMixed>,
    binaries: Vec<BString>,
    dev: bool,
    /// `'stable'` | `'RC'` | `'beta'` | `'alpha'` | `'dev'`
    stability: BString,
    notification_url: Option<BString>,

    requires: std::rc::Rc<IndexMap<BString, Link>>,
    conflicts: std::rc::Rc<IndexMap<BString, Link>>,
    provides: std::rc::Rc<IndexMap<BString, Link>>,
    replaces: std::rc::Rc<IndexMap<BString, Link>>,
    dev_requires: std::rc::Rc<IndexMap<BString, Link>>,
    suggests: IndexMap<BString, BString>,
    autoload: IndexMap<BString, PhpMixed>,
    dev_autoload: IndexMap<BString, PhpMixed>,
    include_paths: Vec<BString>,
    is_default_branch: bool,
    transport_options: IndexMap<BString, PhpMixed>,
    php_ext: Option<IndexMap<BString, PhpMixed>>,
}

impl Package {
    /// Creates a new in memory package.
    pub fn new(name: BString, version: BString, pretty_version: BString) -> Self {
        let stability = VersionParser::parse_stability(&version);
        let dev = stability == "dev";
        Self {
            id: -1,
            name: strtolower(&name),
            pretty_name: name,
            repository: None,
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
            requires: std::rc::Rc::new(IndexMap::new()),
            conflicts: std::rc::Rc::new(IndexMap::new()),
            provides: std::rc::Rc::new(IndexMap::new()),
            replaces: std::rc::Rc::new(IndexMap::new()),
            dev_requires: std::rc::Rc::new(IndexMap::new()),
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

    pub fn set_type(&mut self, r#type: BString) {
        self.r#type = Some(r#type);
    }

    pub fn get_type(&self) -> BString {
        self.r#type
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "library".into())
    }

    pub fn get_stability(&self) -> &BStr {
        self.stability.as_bstr()
    }

    pub fn set_target_dir(&mut self, target_dir: Option<BString>) {
        self.target_dir = target_dir;
    }

    pub fn get_target_dir(&self) -> Option<BString> {
        let target_dir = self.target_dir.as_ref()?;

        let replaced = preg_replace(
            php_regex!("{ (?:^|[\\\\/]+) \\.\\.? (?:[\\\\/]+|$) (?:\\.\\.? (?:[\\\\/]+|$) )*}x"),
            "/",
            target_dir,
        );
        Some(ltrim2(&replaced, "/"))
    }

    pub fn set_extra(&mut self, extra: IndexMap<BString, PhpMixed>) {
        self.extra = extra;
    }

    pub fn get_extra(&self) -> &IndexMap<BString, PhpMixed> {
        &self.extra
    }

    pub fn set_binaries(&mut self, binaries: Vec<BString>) {
        self.binaries = binaries;
    }

    pub fn get_binaries(&self) -> &Vec<BString> {
        &self.binaries
    }

    pub fn set_installation_source(&mut self, r#type: Option<BString>) {
        self.installation_source = r#type;
    }

    pub fn get_installation_source(&self) -> Option<BString> {
        self.installation_source.clone()
    }

    pub fn set_source_type(&mut self, r#type: Option<BString>) {
        self.source_type = r#type;
    }

    pub fn get_source_type(&self) -> Option<BString> {
        self.source_type.clone()
    }

    pub fn set_source_url(&mut self, url: Option<BString>) {
        self.source_url = url;
    }

    pub fn get_source_url(&self) -> Option<BString> {
        self.source_url.clone()
    }

    pub fn set_source_reference(&mut self, reference: Option<BString>) {
        self.source_reference = reference;
    }

    pub fn get_source_reference(&self) -> Option<BString> {
        self.source_reference.clone()
    }

    pub fn set_source_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.source_mirrors = mirrors;
    }

    pub fn get_source_mirrors(&self) -> Option<&Vec<Mirror>> {
        self.source_mirrors.as_ref()
    }

    pub fn get_source_urls(&self) -> Vec<BString> {
        self.get_urls(
            self.source_url.as_ref().map(|s| s.as_bstr()),
            self.source_mirrors.as_ref(),
            self.source_reference.as_ref().map(|s| s.as_bstr()),
            self.source_type.as_ref().map(|s| s.as_bstr()),
            "source",
        )
    }

    pub fn set_dist_type(&mut self, r#type: Option<BString>) {
        self.dist_type = r#type.filter(|t| !t.is_empty());
    }

    pub fn get_dist_type(&self) -> Option<BString> {
        self.dist_type.clone()
    }

    pub fn set_dist_url(&mut self, url: Option<BString>) {
        self.dist_url = url.filter(|u| !u.is_empty());
    }

    pub fn get_dist_url(&self) -> Option<BString> {
        self.dist_url.clone()
    }

    pub fn set_dist_reference(&mut self, reference: Option<BString>) {
        self.dist_reference = reference;
    }

    pub fn get_dist_reference(&self) -> Option<BString> {
        self.dist_reference.clone()
    }

    pub fn set_dist_sha1_checksum(&mut self, sha1checksum: Option<BString>) {
        self.dist_sha1_checksum = sha1checksum;
    }

    pub fn get_dist_sha1_checksum(&self) -> Option<BString> {
        self.dist_sha1_checksum.clone()
    }

    pub fn set_dist_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.dist_mirrors = mirrors;
    }

    pub fn get_dist_mirrors(&self) -> Option<&Vec<Mirror>> {
        self.dist_mirrors.as_ref()
    }

    pub fn get_dist_urls(&self) -> Vec<BString> {
        self.get_urls(
            self.dist_url.as_ref().map(|s| s.as_bstr()),
            self.dist_mirrors.as_ref(),
            self.dist_reference.as_ref().map(|s| s.as_bstr()),
            self.dist_type.as_ref().map(|s| s.as_bstr()),
            "dist",
        )
    }

    pub fn get_transport_options(&self) -> &IndexMap<BString, PhpMixed> {
        &self.transport_options
    }

    pub fn set_transport_options(&mut self, options: IndexMap<BString, PhpMixed>) {
        self.transport_options = options;
    }

    pub fn get_version(&self) -> &BStr {
        self.version.as_bstr()
    }

    pub fn get_pretty_version(&self) -> &BStr {
        self.pretty_version.as_bstr()
    }

    pub fn set_release_date(&mut self, release_date: Option<chrono::DateTime<chrono::Utc>>) {
        self.release_date = release_date;
    }

    pub fn get_release_date(&self) -> Option<&chrono::DateTime<chrono::Utc>> {
        self.release_date.as_ref()
    }

    /// Set the required packages
    pub fn set_requires(&mut self, mut requires: IndexMap<BString, Link>) {
        if requires.contains_key(b"0".as_slice()) {
            requires = self.convert_links_to_map(requires, "setRequires");
        }

        self.requires = std::rc::Rc::new(requires);
    }

    pub fn get_requires(&self) -> &IndexMap<BString, Link> {
        &self.requires
    }

    pub fn set_conflicts(&mut self, mut conflicts: IndexMap<BString, Link>) {
        if conflicts.contains_key(b"0".as_slice()) {
            conflicts = self.convert_links_to_map(conflicts, "setConflicts");
        }

        self.conflicts = std::rc::Rc::new(conflicts);
    }

    pub fn get_conflicts(&self) -> &IndexMap<BString, Link> {
        &self.conflicts
    }

    pub fn set_provides(&mut self, mut provides: IndexMap<BString, Link>) {
        if provides.contains_key(b"0".as_slice()) {
            provides = self.convert_links_to_map(provides, "setProvides");
        }

        self.provides = std::rc::Rc::new(provides);
    }

    pub fn get_provides(&self) -> &IndexMap<BString, Link> {
        &self.provides
    }

    pub fn set_replaces(&mut self, mut replaces: IndexMap<BString, Link>) {
        if replaces.contains_key(b"0".as_slice()) {
            replaces = self.convert_links_to_map(replaces, "setReplaces");
        }

        self.replaces = std::rc::Rc::new(replaces);
    }

    pub fn get_replaces(&self) -> &IndexMap<BString, Link> {
        &self.replaces
    }

    pub fn set_dev_requires(&mut self, mut dev_requires: IndexMap<BString, Link>) {
        if dev_requires.contains_key(b"0".as_slice()) {
            dev_requires = self.convert_links_to_map(dev_requires, "setDevRequires");
        }

        self.dev_requires = std::rc::Rc::new(dev_requires);
    }

    pub fn get_dev_requires(&self) -> &IndexMap<BString, Link> {
        &self.dev_requires
    }

    pub fn set_suggests(&mut self, suggests: IndexMap<BString, BString>) {
        self.suggests = suggests;
    }

    pub fn get_suggests(&self) -> &IndexMap<BString, BString> {
        &self.suggests
    }

    pub fn set_autoload(&mut self, autoload: IndexMap<BString, PhpMixed>) {
        self.autoload = autoload;
    }

    pub fn get_autoload(&self) -> &IndexMap<BString, PhpMixed> {
        &self.autoload
    }

    pub fn set_dev_autoload(&mut self, dev_autoload: IndexMap<BString, PhpMixed>) {
        self.dev_autoload = dev_autoload;
    }

    pub fn get_dev_autoload(&self) -> &IndexMap<BString, PhpMixed> {
        &self.dev_autoload
    }

    pub fn set_include_paths(&mut self, include_paths: Vec<BString>) {
        self.include_paths = include_paths;
    }

    pub fn get_include_paths(&self) -> &Vec<BString> {
        &self.include_paths
    }

    pub fn set_php_ext(&mut self, php_ext: Option<IndexMap<BString, PhpMixed>>) {
        self.php_ext = php_ext;
    }

    pub fn get_php_ext(&self) -> Option<&IndexMap<BString, PhpMixed>> {
        self.php_ext.as_ref()
    }

    pub fn set_notification_url(&mut self, notification_url: BString) {
        self.notification_url = Some(notification_url);
    }

    pub fn get_notification_url(&self) -> Option<BString> {
        self.notification_url.clone()
    }

    pub fn set_is_default_branch(&mut self, default_branch: bool) {
        self.is_default_branch = default_branch;
    }

    pub fn is_default_branch(&self) -> bool {
        self.is_default_branch
    }

    pub fn set_source_dist_references(&mut self, reference: BString) {
        self.set_source_reference(Some(reference.clone()));

        // only bitbucket, github and gitlab have auto generated dist URLs that easily allow replacing the reference in the dist URL
        // TODO generalize this a bit for self-managed/on-prem versions? Some kind of replace token in dist urls which allow this?
        if self.get_dist_url().is_some()
            && preg_is_match(
                php_regex!(
                    "{^https?://(?:(?:www\\.)?bitbucket\\.org|(api\\.)?github\\.com|(?:www\\.)?gitlab\\.com)/}i"
                ),
                self.get_dist_url().unwrap_or_default(),
            )
        {
            self.set_dist_reference(Some(reference.clone()));
            // Regex pattern compatibility:
            // The `regex` crate has no look-around, so `(?<=/|sha=)[a-f0-9]{40}(?=/|$)` is
            // rewritten to capture the boundary delimiters instead of asserting them, and the
            // callback re-emits them around the replaced reference. Unlike the zero-width
            // lookaround, the capturing version consumes its boundary delimiter, so two 40-hex
            // SHAs sharing a single `/` between them would not both match; harmless here since a
            // dist URL never carries more than one SHA reference.
            self.set_dist_url(Some(
                preg_replace_callback(
                    php_regex!("{(/|sha=)[a-f0-9]{40}(/|$)}i"),
                    |m: &PregMatches| -> anyhow::Result<BString> {
                        Ok(bconcat!(
                            m.get(1).unwrap_or_default(),
                            reference,
                            m.get(2).unwrap_or_default()
                        ))
                    },
                    &self.get_dist_url().unwrap_or_default(),
                )
                .expect("the replacement callback cannot fail"),
            ));
        } else if self.get_dist_reference().is_some() {
            // update the dist reference if there was one, but if none was provided ignore it
            self.set_dist_reference(Some(reference));
        }
    }

    /// Replaces current version and pretty version with passed values.
    /// It also sets stability.
    pub fn replace_version(&mut self, version: BString, pretty_version: BString) {
        self.version = version;
        self.pretty_version = pretty_version;

        self.stability = VersionParser::parse_stability(&self.version);
        self.dev = self.stability == "dev";
    }

    fn get_urls(
        &self,
        url: Option<&BStr>,
        mirrors: Option<&Vec<Mirror>>,
        r#ref: Option<&BStr>,
        r#type: Option<&BStr>,
        url_type: &str,
    ) -> Vec<BString> {
        let url = match url {
            Some(u) if !u.is_empty() => u,
            _ => return Vec::new(),
        };

        let url = if url_type == "dist" && strpos(url, "%").is_some() {
            ComposerMirror::process_url(
                url,
                self.name.as_bstr(),
                self.version.as_bstr(),
                r#ref,
                r#type,
                Some(self.pretty_version.as_bstr()),
            )
        } else {
            BString::from(url)
        };

        let mut urls: Vec<BString> = vec![url.clone()];
        if let Some(mirrors) = mirrors {
            for mirror in mirrors {
                let mirror_url = if url_type == "dist" {
                    ComposerMirror::process_url(
                        mirror.url.as_bstr(),
                        self.name.as_bstr(),
                        self.version.as_bstr(),
                        r#ref,
                        r#type,
                        Some(self.pretty_version.as_bstr()),
                    )
                } else if url_type == "source" && r#type.is_some_and(|t| t == "git") {
                    ComposerMirror::process_git_url(
                        mirror.url.as_bstr(),
                        self.name.as_bstr(),
                        url.as_bstr(),
                        r#type,
                    )
                } else if url_type == "source" && r#type.is_some_and(|t| t == "hg") {
                    ComposerMirror::process_hg_url(
                        mirror.url.as_bstr(),
                        self.name.as_bstr(),
                        url.as_bstr(),
                        r#type.unwrap_or_default(),
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
        links: IndexMap<BString, Link>,
        source: &str,
    ) -> IndexMap<BString, Link> {
        trigger_error(
            &format!(
                "Package::{} must be called with a map of lowercased package name => Link object, got a indexed array, this is deprecated and you should fix your usage.",
                source
            ),
            E_USER_DEPRECATED,
        );
        let mut new_links: IndexMap<BString, Link> = IndexMap::new();
        for (_k, link) in links {
            new_links.insert(BString::from(link.get_target()), link);
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

    fn name(&self) -> &BStr {
        self.name.as_bstr()
    }

    fn name_mut(&mut self) -> &mut BString {
        &mut self.name
    }

    fn pretty_name(&self) -> &BStr {
        self.pretty_name.as_bstr()
    }

    fn pretty_name_mut(&mut self) -> &mut BString {
        &mut self.pretty_name
    }

    fn repository_opt(&self) -> Option<RepositoryInterfaceHandle> {
        self.repository
            .as_ref()
            .and_then(|w| w.upgrade())
            .map(RepositoryInterfaceHandle::from_rc)
    }

    fn set_repository_box(&mut self, repository: RepositoryInterfaceHandle) {
        self.repository = Some(repository.downgrade());
    }

    fn take_repository(&mut self) -> Option<RepositoryInterfaceHandle> {
        self.repository
            .take()
            .and_then(|w| w.upgrade())
            .map(RepositoryInterfaceHandle::from_rc)
    }
}

impl PackageInterface for Package {
    fn get_name(&self) -> &BStr {
        self.name.as_bstr()
    }
    fn get_pretty_name(&self) -> &BStr {
        self.pretty_name.as_bstr()
    }
    fn get_names(&self, provides: bool) -> Vec<BString> {
        let mut names: IndexSet<BString> = IndexSet::new();
        names.insert(BString::from(self.get_name()));

        if provides {
            for (_, link) in self.get_provides().iter() {
                names.insert(BString::from(link.get_target()));
            }
        }

        for (_, link) in self.get_replaces().iter() {
            names.insert(BString::from(link.get_target()));
        }

        names.into_iter().collect()
    }
    fn set_id(&mut self, id: i64) {
        self.id = id;
    }
    fn get_id(&self) -> i64 {
        self.id
    }
    fn is_dev(&self) -> bool {
        self.dev
    }
    fn get_type(&self) -> BString {
        Package::get_type(self)
    }
    fn get_target_dir(&self) -> Option<BString> {
        Package::get_target_dir(self)
    }
    fn get_extra(&self) -> IndexMap<BString, PhpMixed> {
        self.extra.clone()
    }
    fn set_installation_source(&mut self, r#type: Option<BString>) {
        self.installation_source = r#type;
    }
    fn get_installation_source(&self) -> Option<BString> {
        self.installation_source.clone()
    }
    fn get_source_type(&self) -> Option<BString> {
        self.source_type.clone()
    }
    fn get_source_url(&self) -> Option<BString> {
        self.source_url.clone()
    }
    fn get_source_urls(&self) -> Vec<BString> {
        Package::get_source_urls(self)
    }
    fn get_source_reference(&self) -> Option<BString> {
        self.source_reference.clone()
    }
    fn get_source_mirrors(&self) -> Option<Vec<Mirror>> {
        self.source_mirrors.clone()
    }
    fn set_source_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.source_mirrors = mirrors;
    }
    fn get_dist_type(&self) -> Option<BString> {
        self.dist_type.clone()
    }
    fn get_dist_url(&self) -> Option<BString> {
        self.dist_url.clone()
    }
    fn get_dist_urls(&self) -> Vec<BString> {
        Package::get_dist_urls(self)
    }
    fn get_dist_reference(&self) -> Option<BString> {
        self.dist_reference.clone()
    }
    fn get_dist_sha1_checksum(&self) -> Option<BString> {
        self.dist_sha1_checksum.clone()
    }
    fn get_dist_mirrors(&self) -> Option<Vec<Mirror>> {
        self.dist_mirrors.clone()
    }
    fn set_dist_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.dist_mirrors = mirrors;
    }
    fn get_version(&self) -> &BStr {
        self.version.as_bstr()
    }
    fn get_pretty_version(&self) -> &BStr {
        self.pretty_version.as_bstr()
    }
    fn get_full_pretty_version(&self, truncate: bool, display_mode: DisplayMode) -> BString {
        BasePackage::get_full_pretty_version(self, truncate, display_mode)
    }
    fn get_release_date(&self) -> Option<DateTime<Utc>> {
        self.release_date
    }
    fn get_stability(&self) -> &BStr {
        self.stability.as_bstr()
    }
    fn get_requires(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        std::rc::Rc::clone(&self.requires)
    }
    fn get_conflicts(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        std::rc::Rc::clone(&self.conflicts)
    }
    fn get_provides(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        std::rc::Rc::clone(&self.provides)
    }
    fn get_replaces(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        std::rc::Rc::clone(&self.replaces)
    }
    fn get_dev_requires(&self) -> std::rc::Rc<IndexMap<BString, Link>> {
        std::rc::Rc::clone(&self.dev_requires)
    }
    fn get_suggests(&self) -> IndexMap<BString, BString> {
        self.suggests.clone()
    }
    fn get_autoload(&self) -> IndexMap<BString, PhpMixed> {
        self.autoload.clone()
    }
    fn get_dev_autoload(&self) -> IndexMap<BString, PhpMixed> {
        self.dev_autoload.clone()
    }
    fn get_include_paths(&self) -> Vec<BString> {
        self.include_paths.clone()
    }
    fn get_php_ext(&self) -> Option<IndexMap<BString, PhpMixed>> {
        self.php_ext.clone()
    }
    fn set_repository(&mut self, repository: RepositoryInterfaceHandle) -> anyhow::Result<()> {
        if let Some(existing) = self.repository.as_ref().and_then(|w| w.upgrade())
            && !std::rc::Rc::ptr_eq(&existing, repository.as_rc())
        {
            return Err(LogicException::new(
                "A package can only be added to one repository".to_string(),
            )
            .into());
        }
        self.repository = Some(repository.downgrade());
        Ok(())
    }
    fn get_repository(&self) -> Option<RepositoryInterfaceHandle> {
        self.repository
            .as_ref()
            .and_then(|w| w.upgrade())
            .map(RepositoryInterfaceHandle::from_rc)
    }
    fn get_binaries(&self) -> Vec<BString> {
        self.binaries.clone()
    }
    fn get_unique_name(&self) -> BString {
        bconcat!(self.get_name(), "-", self.get_version())
    }
    fn get_notification_url(&self) -> Option<BString> {
        self.notification_url.clone()
    }
    fn get_pretty_string(&self) -> BString {
        bconcat!(self.get_pretty_name(), " ", self.get_pretty_version())
    }
    fn is_default_branch(&self) -> bool {
        self.is_default_branch
    }
    fn get_transport_options(&self) -> IndexMap<BString, PhpMixed> {
        self.transport_options.clone()
    }
    fn set_transport_options(&mut self, options: IndexMap<BString, PhpMixed>) {
        self.transport_options = options;
    }
    fn set_source_reference(&mut self, reference: Option<BString>) {
        self.source_reference = reference;
    }
    fn set_source_url(&mut self, url: Option<BString>) {
        self.source_url = url;
    }
    fn set_dist_url(&mut self, url: Option<BString>) {
        self.dist_url = url.filter(|u| !u.is_empty());
    }
    fn set_dist_type(&mut self, r#type: Option<BString>) {
        self.dist_type = r#type.filter(|t| !t.is_empty());
    }
    fn set_dist_reference(&mut self, reference: Option<BString>) {
        self.dist_reference = reference;
    }
    fn set_source_dist_references(&mut self, reference: BString) {
        Package::set_source_dist_references(self, reference);
    }
}
