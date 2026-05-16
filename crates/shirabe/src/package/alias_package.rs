//! ref: composer/src/Composer/Package/AliasPackage.php

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, in_array};
use shirabe_semver::constraint::constraint::Constraint;

use crate::package::base_package::BasePackage;
use crate::package::link::Link;
use crate::package::package_interface::PackageInterface;
use crate::package::version::version_parser::VersionParser;
use crate::repository::repository_interface::RepositoryInterface;

#[derive(Debug)]
pub struct AliasPackage {
    pub(crate) inner: BasePackage,
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
    pub(crate) alias_of: Box<BasePackage>,
    /// @var Link[]
    pub(crate) requires: IndexMap<String, Link>,
    /// @var Link[]
    pub(crate) dev_requires: IndexMap<String, Link>,
    /// @var Link[]
    pub(crate) conflicts: Vec<Link>,
    /// @var Link[]
    pub(crate) provides: Vec<Link>,
    /// @var Link[]
    pub(crate) replaces: Vec<Link>,
}

impl AliasPackage {
    /// All descendants' constructors should call this parent constructor
    ///
    /// @param BasePackage $aliasOf       The package this package is an alias of
    /// @param string      $version       The version the alias must report
    /// @param string      $prettyVersion The alias's non-normalized version
    pub fn new(alias_of: Box<BasePackage>, version: String, pretty_version: String) -> Self {
        let inner = BasePackage::new(alias_of.get_name().to_string());

        let stability = VersionParser::parse_stability(&version);
        let dev = stability == "dev";

        let mut this = Self {
            inner,
            version,
            pretty_version,
            dev,
            root_package_alias: false,
            stability,
            has_self_version_requires: false,
            alias_of,
            requires: IndexMap::new(),
            dev_requires: IndexMap::new(),
            conflicts: vec![],
            provides: vec![],
            replaces: vec![],
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
                Link::TYPE_PROVIDE => this.provides = replaced,
                Link::TYPE_CONFLICT => this.conflicts = replaced,
                Link::TYPE_REPLACE => this.replaces = replaced,
                _ => {}
            }
        }

        this
    }

    pub fn get_alias_of(&self) -> &BasePackage {
        &self.alias_of
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
                    let mut constraint = Constraint::new("=", &self.version);
                    let new_link = Link::new(
                        link.get_source().to_string(),
                        link.get_target().to_string(),
                        Box::new(constraint.clone()),
                        Some(link_type.to_string()),
                        Some(pretty_version.clone()),
                    );
                    constraint.set_pretty_string(&pretty_version);
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
                    let mut constraint = Constraint::new("=", &self.version);
                    let new_link = Link::new(
                        links[index].get_source().to_string(),
                        links[index].get_target().to_string(),
                        Box::new(constraint.clone()),
                        Some(link_type.to_string()),
                        Some(pretty_version.clone()),
                    );
                    constraint.set_pretty_string(&pretty_version);
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
            self.inner,
            if self.root_package_alias { "root " } else { "" },
            self.alias_of.get_version(),
        )
    }
}

impl PackageInterface for AliasPackage {
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
    /// @return array<string|int, Link>
    fn get_conflicts(&self) -> Vec<Link> {
        self.conflicts.clone()
    }

    /// @inheritDoc
    /// @return array<string|int, Link>
    fn get_provides(&self) -> Vec<Link> {
        self.provides.clone()
    }

    /// @inheritDoc
    /// @return array<string|int, Link>
    fn get_replaces(&self) -> Vec<Link> {
        self.replaces.clone()
    }

    fn get_dev_requires(&self) -> IndexMap<String, Link> {
        self.dev_requires.clone()
    }

    fn get_type(&self) -> &str {
        self.alias_of.get_type()
    }

    fn get_target_dir(&self) -> Option<&str> {
        self.alias_of.get_target_dir()
    }

    fn get_extra(&self) -> IndexMap<String, PhpMixed> {
        self.alias_of.get_extra()
    }

    fn set_installation_source(&mut self, r#type: Option<String>) {
        self.alias_of.set_installation_source(r#type);
    }

    fn get_installation_source(&self) -> Option<&str> {
        self.alias_of.get_installation_source()
    }

    fn get_source_type(&self) -> Option<&str> {
        self.alias_of.get_source_type()
    }

    fn get_source_url(&self) -> Option<&str> {
        self.alias_of.get_source_url()
    }

    fn get_source_urls(&self) -> Vec<String> {
        self.alias_of.get_source_urls()
    }

    fn get_source_reference(&self) -> Option<&str> {
        self.alias_of.get_source_reference()
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
        self.alias_of.get_dist_type()
    }

    fn get_dist_url(&self) -> Option<&str> {
        self.alias_of.get_dist_url()
    }

    fn get_dist_urls(&self) -> Vec<String> {
        self.alias_of.get_dist_urls()
    }

    fn get_dist_reference(&self) -> Option<&str> {
        self.alias_of.get_dist_reference()
    }

    fn set_dist_reference(&mut self, reference: Option<String>) {
        self.alias_of.set_dist_reference(reference);
    }

    fn get_dist_sha1_checksum(&self) -> Option<&str> {
        self.alias_of.get_dist_sha1_checksum()
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
        self.alias_of.get_notification_url()
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
        // TODO(phase-b): BasePackage.get_full_pretty_version returns Result; bridge here
        self.inner
            .get_full_pretty_version(truncate, display_mode)
            .unwrap_or_default()
    }

    fn get_unique_name(&self) -> String {
        self.inner.get_unique_name()
    }

    fn get_pretty_string(&self) -> String {
        self.inner.get_pretty_string()
    }

    fn set_repository(&mut self, repository: Box<dyn RepositoryInterface>) -> anyhow::Result<()> {
        self.inner.set_repository(repository)
    }

    fn get_repository(&self) -> Option<&dyn RepositoryInterface> {
        self.inner.get_repository()
    }
}
