//! ref: composer/src/Composer/Package/AliasPackage.php

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, in_array};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;

use crate::package::BasePackage;
use crate::package::Link;
use crate::package::PackageHandle;
use crate::package::PackageInterface;
use crate::package::version::VersionParser;
use crate::repository::RepositoryInterface;

#[derive(Debug)]
pub struct AliasPackage {
    id: i64,
    name: String,
    pretty_name: String,
    repository: Option<Box<dyn RepositoryInterface>>,

    /// @var string
    pub(crate) version: String,
    /// @var string
    pub(crate) pretty_version: String,
    /// @var bool
    pub(crate) dev: bool,
    /// @var bool
    pub(crate) root_package_alias: bool,
    /// @var string
    /// @phpstan-var 'stable'|'RC'|'beta'|'alpha'|'dev'
    pub(crate) stability: String,
    /// @var bool
    pub(crate) has_self_version_requires: bool,

    /// @var BasePackage
    pub(crate) alias_of: PackageHandle,
    /// @var Link[]
    pub(crate) requires: IndexMap<String, Link>,
    /// @var Link[]
    pub(crate) dev_requires: IndexMap<String, Link>,
    /// @var array<string, Link>
    pub(crate) conflicts: IndexMap<String, Link>,
    /// @var array<string, Link>
    pub(crate) provides: IndexMap<String, Link>,
    /// @var array<string, Link>
    pub(crate) replaces: IndexMap<String, Link>,
}

impl AliasPackage {
    /// All descendants' constructors should call this parent constructor
    ///
    /// @param BasePackage $aliasOf       The package this package is an alias of
    /// @param string      $version       The version the alias must report
    /// @param string      $prettyVersion The alias's non-normalized version
    pub fn new(alias_of: PackageHandle, version: String, pretty_version: String) -> Self {
        let alias_name = alias_of.get_name();

        let stability = VersionParser::parse_stability(&version).to_string();
        let dev = stability == "dev";

        let mut this = Self {
            id: -1,
            name: alias_name.to_lowercase(),
            pretty_name: alias_name,
            repository: None,
            version,
            pretty_version,
            dev,
            root_package_alias: false,
            stability,
            has_self_version_requires: false,
            alias_of,
            requires: IndexMap::new(),
            dev_requires: IndexMap::new(),
            conflicts: IndexMap::new(),
            provides: IndexMap::new(),
            replaces: IndexMap::new(),
        };

        for r#type in Link::types() {
            // PHP: $aliasOf->{'get' . ucfirst($type)}()
            // TODO(phase-b): dynamic method dispatch — bridge each Link::TYPE_* to BasePackage getter
            let links: Vec<Link> = match r#type {
                Link::TYPE_REQUIRE => this.alias_of.get_requires().values().cloned().collect(),
                Link::TYPE_DEV_REQUIRE => {
                    this.alias_of.get_dev_requires().values().cloned().collect()
                }
                Link::TYPE_PROVIDE => this.alias_of.get_provides().values().cloned().collect(),
                Link::TYPE_CONFLICT => this.alias_of.get_conflicts().values().cloned().collect(),
                Link::TYPE_REPLACE => this.alias_of.get_replaces().values().cloned().collect(),
                _ => vec![],
            };
            let replaced = this.replace_self_version_dependencies(links, r#type);
            match r#type {
                Link::TYPE_REQUIRE => {
                    this.requires = replaced
                        .into_iter()
                        .map(|l| (l.get_target().to_string(), l))
                        .collect();
                }
                Link::TYPE_DEV_REQUIRE => {
                    this.dev_requires = replaced
                        .into_iter()
                        .map(|l| (l.get_target().to_string(), l))
                        .collect();
                }
                Link::TYPE_PROVIDE => {
                    this.provides = replaced
                        .into_iter()
                        .map(|l| (l.get_target().to_string(), l))
                        .collect()
                }
                Link::TYPE_CONFLICT => {
                    this.conflicts = replaced
                        .into_iter()
                        .map(|l| (l.get_target().to_string(), l))
                        .collect()
                }
                Link::TYPE_REPLACE => {
                    this.replaces = replaced
                        .into_iter()
                        .map(|l| (l.get_target().to_string(), l))
                        .collect()
                }
                _ => {}
            }
        }

        this
    }

    pub fn get_alias_of(&self) -> PackageHandle {
        self.alias_of.clone()
    }

    /// Stores whether this is an alias created by an aliasing in the requirements of the root package or not
    ///
    /// Use by the policy for sorting manually aliased packages first, see #576
    pub fn set_root_package_alias(&mut self, value: bool) {
        self.root_package_alias = value;
    }

    /// @see setRootPackageAlias
    pub fn is_root_package_alias(&self) -> bool {
        self.root_package_alias
    }

    /// @param Link[]       $links
    /// @param Link::TYPE_* $linkType
    ///
    /// @return Link[]
    pub(crate) fn replace_self_version_dependencies(
        &mut self,
        mut links: Vec<Link>,
        link_type: &str,
    ) -> Vec<Link> {
        // for self.version requirements, we use the original package's branch name instead, to avoid leaking the magic dev-master-alias to users
        let mut pretty_version = self.pretty_version.clone();
        if pretty_version == VersionParser::DEFAULT_BRANCH_ALIAS {
            pretty_version = self.alias_of.get_pretty_version().to_string();
        }

        if in_array(
            PhpMixed::String(link_type.to_string()),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::String(Link::TYPE_CONFLICT.to_string())),
                Box::new(PhpMixed::String(Link::TYPE_PROVIDE.to_string())),
                Box::new(PhpMixed::String(Link::TYPE_REPLACE.to_string())),
            ]),
            true,
        ) {
            let mut new_links: Vec<Link> = vec![];
            for link in &links {
                // link is self.version, but must be replacing also the replaced version
                if link.get_pretty_constraint().unwrap_or("") == "self.version" {
                    let constraint = SimpleConstraint::new(
                        "=".to_string(),
                        self.version.to_string(),
                        Some(pretty_version.clone()),
                    );
                    let new_link = Link::new(
                        link.get_source().to_string(),
                        link.get_target().to_string(),
                        constraint.into(),
                        Some(link_type.to_string()),
                        Some(pretty_version.clone()),
                    );
                    new_links.push(new_link);
                }
            }
            links.extend(new_links);
        } else {
            // PHP: foreach ($links as $index => $link)
            for index in 0..links.len() {
                if links[index].get_pretty_constraint().unwrap_or("") == "self.version" {
                    if link_type == Link::TYPE_REQUIRE {
                        self.has_self_version_requires = true;
                    }
                    let constraint = SimpleConstraint::new(
                        "=".to_string(),
                        self.version.to_string(),
                        Some(pretty_version.clone()),
                    );
                    let new_link = Link::new(
                        links[index].get_source().to_string(),
                        links[index].get_target().to_string(),
                        constraint.into(),
                        Some(link_type.to_string()),
                        Some(pretty_version.clone()),
                    );
                    links[index] = new_link;
                }
            }
        }

        links
    }

    pub fn has_self_version_requires(&self) -> bool {
        self.has_self_version_requires
    }
}

impl std::fmt::Display for AliasPackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}alias of {})",
            self.alias_of,
            if self.root_package_alias { "root " } else { "" },
            self.alias_of.get_version(),
        )
    }
}

impl PackageInterface for AliasPackage {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_name(&self) -> &str {
        // PHP delegates to aliasOf; the local name mirrors aliasOf->getName(),
        // so it is returned here to avoid borrowing across the shared handle.
        &self.name
    }

    fn get_pretty_name(&self) -> &str {
        &self.pretty_name
    }

    fn get_names(&self, provides: bool) -> Vec<String> {
        self.alias_of.get_names(provides)
    }

    fn set_id(&mut self, id: i64) {
        self.alias_of.set_id(id);
    }

    fn get_id(&self) -> i64 {
        self.alias_of.get_id()
    }

    fn is_dev(&self) -> bool {
        self.dev
    }

    fn get_version(&self) -> &str {
        &self.version
    }

    fn get_stability(&self) -> &str {
        &self.stability
    }

    fn get_pretty_version(&self) -> &str {
        &self.pretty_version
    }

    fn get_requires(&self) -> IndexMap<String, Link> {
        self.requires.clone()
    }

    /// @inheritDoc
    /// @return array<string, Link>
    fn get_conflicts(&self) -> IndexMap<String, Link> {
        self.conflicts.clone()
    }

    /// @inheritDoc
    /// @return array<string, Link>
    fn get_provides(&self) -> IndexMap<String, Link> {
        self.provides.clone()
    }

    /// @inheritDoc
    /// @return array<string, Link>
    fn get_replaces(&self) -> IndexMap<String, Link> {
        self.replaces.clone()
    }

    fn get_dev_requires(&self) -> IndexMap<String, Link> {
        self.dev_requires.clone()
    }

    fn get_type(&self) -> &str {
        // Delegates to the shared `aliasOf` handle, whose getters yield owned
        // `String`s; a borrow cannot escape the `RefCell`. Use the handle API
        // (`AliasPackageHandle::get_alias_of().get_type()`) instead.
        todo!("AliasPackage::get_type cannot return &str across the aliasOf handle")
    }

    fn get_target_dir(&self) -> Option<&str> {
        todo!("AliasPackage::get_target_dir cannot return &str across the aliasOf handle")
    }

    fn get_extra(&self) -> IndexMap<String, PhpMixed> {
        self.alias_of.get_extra()
    }

    fn set_installation_source(&mut self, r#type: Option<String>) {
        self.alias_of.set_installation_source(r#type);
    }

    fn get_installation_source(&self) -> Option<&str> {
        todo!("AliasPackage::get_installation_source cannot return &str across the aliasOf handle")
    }

    fn get_source_type(&self) -> Option<&str> {
        todo!("AliasPackage::get_source_type cannot return &str across the aliasOf handle")
    }

    fn get_source_url(&self) -> Option<&str> {
        todo!("AliasPackage::get_source_url cannot return &str across the aliasOf handle")
    }

    fn get_source_urls(&self) -> Vec<String> {
        self.alias_of.get_source_urls()
    }

    fn get_source_reference(&self) -> Option<&str> {
        todo!("AliasPackage::get_source_reference cannot return &str across the aliasOf handle")
    }

    fn set_source_reference(&mut self, reference: Option<String>) {
        self.alias_of.set_source_reference(reference);
    }

    fn set_source_mirrors(&mut self, mirrors: Option<Vec<IndexMap<String, PhpMixed>>>) {
        self.alias_of.set_source_mirrors(mirrors);
    }

    fn get_source_mirrors(&self) -> Option<Vec<IndexMap<String, PhpMixed>>> {
        self.alias_of.get_source_mirrors()
    }

    fn get_dist_type(&self) -> Option<&str> {
        todo!("AliasPackage::get_dist_type cannot return &str across the aliasOf handle")
    }

    fn get_dist_url(&self) -> Option<&str> {
        todo!("AliasPackage::get_dist_url cannot return &str across the aliasOf handle")
    }

    fn get_dist_urls(&self) -> Vec<String> {
        self.alias_of.get_dist_urls()
    }

    fn get_dist_reference(&self) -> Option<&str> {
        todo!("AliasPackage::get_dist_reference cannot return &str across the aliasOf handle")
    }

    fn set_dist_reference(&mut self, reference: Option<String>) {
        self.alias_of.set_dist_reference(reference);
    }

    fn get_dist_sha1_checksum(&self) -> Option<&str> {
        todo!("AliasPackage::get_dist_sha1_checksum cannot return &str across the aliasOf handle")
    }

    fn set_transport_options(&mut self, options: IndexMap<String, PhpMixed>) {
        self.alias_of.set_transport_options(options);
    }

    fn get_transport_options(&self) -> IndexMap<String, PhpMixed> {
        self.alias_of.get_transport_options()
    }

    fn set_dist_mirrors(&mut self, mirrors: Option<Vec<IndexMap<String, PhpMixed>>>) {
        self.alias_of.set_dist_mirrors(mirrors);
    }

    fn get_dist_mirrors(&self) -> Option<Vec<IndexMap<String, PhpMixed>>> {
        self.alias_of.get_dist_mirrors()
    }

    fn get_autoload(&self) -> IndexMap<String, PhpMixed> {
        self.alias_of.get_autoload()
    }

    fn get_dev_autoload(&self) -> IndexMap<String, PhpMixed> {
        self.alias_of.get_dev_autoload()
    }

    fn get_include_paths(&self) -> Vec<String> {
        self.alias_of.get_include_paths()
    }

    fn get_php_ext(&self) -> Option<IndexMap<String, PhpMixed>> {
        self.alias_of.get_php_ext()
    }

    fn get_release_date(&self) -> Option<DateTime<Utc>> {
        self.alias_of.get_release_date()
    }

    fn get_binaries(&self) -> Vec<String> {
        self.alias_of.get_binaries()
    }

    fn get_suggests(&self) -> IndexMap<String, String> {
        self.alias_of.get_suggests()
    }

    fn get_notification_url(&self) -> Option<&str> {
        todo!("AliasPackage::get_notification_url cannot return &str across the aliasOf handle")
    }

    fn is_default_branch(&self) -> bool {
        self.alias_of.is_default_branch()
    }

    fn set_dist_url(&mut self, url: Option<String>) {
        self.alias_of.set_dist_url(url);
    }

    fn set_dist_type(&mut self, r#type: Option<String>) {
        self.alias_of.set_dist_type(r#type);
    }

    fn set_source_dist_references(&mut self, reference: &str) {
        self.alias_of.set_source_dist_references(reference);
    }

    fn get_full_pretty_version(&self, truncate: bool, display_mode: i64) -> String {
        self.alias_of
            .get_full_pretty_version(truncate, display_mode)
    }

    fn get_unique_name(&self) -> String {
        self.alias_of.get_unique_name()
    }

    fn get_pretty_string(&self) -> String {
        self.alias_of.get_pretty_string()
    }

    fn set_repository(&mut self, repository: Box<dyn RepositoryInterface>) -> anyhow::Result<()> {
        self.alias_of.set_repository(repository)
    }

    fn get_repository(&self) -> Option<&dyn RepositoryInterface> {
        todo!("AliasPackage::get_repository cannot return a borrow across the aliasOf handle")
    }
}

impl BasePackage for AliasPackage {
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
        self.repository.as_deref()
    }

    fn set_repository_box(&mut self, repository: Box<dyn RepositoryInterface>) {
        todo!()
    }

    fn take_repository(&mut self) -> Option<Box<dyn RepositoryInterface>> {
        todo!()
    }
}
