//! ref: composer/src/Composer/Package/AliasPackage.php

use crate::package::version::VersionParser;
use crate::package::{BasePackage, Link, Mirror, PackageHandle, PackageInterface};
use crate::repository::{RepositoryInterfaceHandle, RepositoryInterfaceWeakHandle};
use bstr::{BStr, BString, ByteSlice};
use chrono::{DateTime, Utc};
use indexmap::{IndexMap, IndexSet};
use shirabe_php_shim::{LogicException, PhpMixed, bconcat, strtolower};
use shirabe_semver::constraint::SimpleConstraint;

#[derive(Debug, Clone)]
pub struct AliasPackage {
    id: i64,
    name: BString,
    pretty_name: BString,
    /// Back-reference to the owning repository. `Weak` breaks the repository -> packages cycle.
    repository: Option<RepositoryInterfaceWeakHandle>,

    version: BString,
    pretty_version: BString,
    dev: bool,
    root_package_alias: bool,
    stability: BString,
    has_self_version_requires: bool,

    pub(crate) alias_of: PackageHandle,
    pub(crate) requires: std::rc::Rc<IndexMap<BString, Link>>,
    pub(crate) dev_requires: std::rc::Rc<IndexMap<BString, Link>>,
    pub(crate) conflicts: std::rc::Rc<IndexMap<BString, Link>>,
    pub(crate) provides: std::rc::Rc<IndexMap<BString, Link>>,
    pub(crate) replaces: std::rc::Rc<IndexMap<BString, Link>>,
}

impl AliasPackage {
    /// All descendants' constructors should call this parent constructor
    pub fn new(alias_of: PackageHandle, version: BString, pretty_version: BString) -> Self {
        let alias_name = alias_of.get_name();

        let stability = VersionParser::parse_stability(&version);
        let dev = stability == "dev";

        let mut this = Self {
            id: -1,
            name: strtolower(&alias_name),
            pretty_name: alias_name,
            repository: None,
            version,
            pretty_version,
            dev,
            root_package_alias: false,
            stability,
            has_self_version_requires: false,
            alias_of,
            requires: std::rc::Rc::new(IndexMap::new()),
            dev_requires: std::rc::Rc::new(IndexMap::new()),
            conflicts: std::rc::Rc::new(IndexMap::new()),
            provides: std::rc::Rc::new(IndexMap::new()),
            replaces: std::rc::Rc::new(IndexMap::new()),
        };

        for r#type in Link::types() {
            let links: std::rc::Rc<IndexMap<BString, Link>> = match r#type {
                Link::TYPE_REQUIRE => this.alias_of.get_requires(),
                Link::TYPE_DEV_REQUIRE => this.alias_of.get_dev_requires(),
                Link::TYPE_PROVIDE => this.alias_of.get_provides(),
                Link::TYPE_CONFLICT => this.alias_of.get_conflicts(),
                Link::TYPE_REPLACE => this.alias_of.get_replaces(),
                _ => unreachable!(),
            };
            let replaced =
                std::rc::Rc::new(this.replace_self_version_dependencies((*links).clone(), r#type));
            match r#type {
                Link::TYPE_REQUIRE => this.requires = replaced,
                Link::TYPE_DEV_REQUIRE => this.dev_requires = replaced,
                Link::TYPE_PROVIDE => this.provides = replaced,
                Link::TYPE_CONFLICT => this.conflicts = replaced,
                Link::TYPE_REPLACE => this.replaces = replaced,
                _ => unreachable!(),
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

    pub fn is_root_package_alias(&self) -> bool {
        self.root_package_alias
    }

    pub(crate) fn replace_self_version_dependencies(
        &mut self,
        mut links: IndexMap<BString, Link>,
        link_type: &str,
    ) -> IndexMap<BString, Link> {
        // for self.version requirements, we use the original package's branch name instead, to avoid leaking the magic dev-master-alias to users
        let mut pretty_version = self.pretty_version.clone();
        if pretty_version == VersionParser::DEFAULT_BRANCH_ALIAS {
            pretty_version = self.alias_of.get_pretty_version();
        }

        if matches!(
            link_type,
            Link::TYPE_CONFLICT | Link::TYPE_PROVIDE | Link::TYPE_REPLACE
        ) {
            let mut new_links: Vec<Link> = vec![];
            for link in links.values() {
                // link is self.version, but must be replacing also the replaced version
                if link.get_pretty_constraint() == "self.version" {
                    let constraint = SimpleConstraint::new(
                        "=",
                        self.version.clone(),
                        Some(pretty_version.clone()),
                    );
                    let new_link = Link::new(
                        BString::from(link.get_source()),
                        BString::from(link.get_target()),
                        constraint.into(),
                        Some(link_type.into()),
                        pretty_version.clone(),
                    );
                    new_links.push(new_link);
                }
            }
            for (index, new_link) in new_links.into_iter().enumerate() {
                links.insert(BString::from(index.to_string()), new_link);
            }
        } else {
            for link in links.values_mut() {
                if link.get_pretty_constraint() == "self.version" {
                    if link_type == Link::TYPE_REQUIRE {
                        self.has_self_version_requires = true;
                    }
                    let constraint = SimpleConstraint::new(
                        "=",
                        self.version.clone(),
                        Some(pretty_version.clone()),
                    );
                    *link = Link::new(
                        BString::from(link.get_source()),
                        BString::from(link.get_target()),
                        constraint.into(),
                        Some(link_type.into()),
                        pretty_version.clone(),
                    );
                }
            }
        }

        links
    }

    pub fn has_self_version_requires(&self) -> bool {
        self.has_self_version_requires
    }
}

impl PackageInterface for AliasPackage {
    fn get_name(&self) -> &BStr {
        // PHP delegates to aliasOf; the local name mirrors aliasOf->getName().
        self.name.as_bstr()
    }

    fn get_pretty_name(&self) -> &BStr {
        self.pretty_name.as_bstr()
    }

    fn get_names(&self, provides: bool) -> Vec<BString> {
        let mut names: IndexSet<BString> = IndexSet::new();
        names.insert(BString::from(self.get_name()));

        if provides {
            for link in self.provides.values() {
                names.insert(BString::from(link.get_target()));
            }
        }

        for link in self.replaces.values() {
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

    fn get_version(&self) -> &BStr {
        self.version.as_bstr()
    }

    fn get_stability(&self) -> &BStr {
        self.stability.as_bstr()
    }

    fn get_pretty_version(&self) -> &BStr {
        self.pretty_version.as_bstr()
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

    fn get_type(&self) -> BString {
        self.alias_of.get_type()
    }

    fn get_target_dir(&self) -> Option<BString> {
        self.alias_of.get_target_dir()
    }

    fn get_extra(&self) -> IndexMap<BString, PhpMixed> {
        self.alias_of.get_extra()
    }

    fn set_installation_source(&mut self, r#type: Option<BString>) {
        self.alias_of.set_installation_source(r#type);
    }

    fn get_installation_source(&self) -> Option<BString> {
        self.alias_of.get_installation_source()
    }

    fn get_source_type(&self) -> Option<BString> {
        self.alias_of.get_source_type()
    }

    fn get_source_url(&self) -> Option<BString> {
        self.alias_of.get_source_url()
    }

    fn get_source_urls(&self) -> Vec<BString> {
        self.alias_of.get_source_urls()
    }

    fn get_source_reference(&self) -> Option<BString> {
        self.alias_of.get_source_reference()
    }

    fn set_source_reference(&mut self, reference: Option<BString>) {
        self.alias_of.set_source_reference(reference);
    }

    fn set_source_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.alias_of.set_source_mirrors(mirrors);
    }

    fn get_source_mirrors(&self) -> Option<Vec<Mirror>> {
        self.alias_of.get_source_mirrors()
    }

    fn get_dist_type(&self) -> Option<BString> {
        self.alias_of.get_dist_type()
    }

    fn get_dist_url(&self) -> Option<BString> {
        self.alias_of.get_dist_url()
    }

    fn get_dist_urls(&self) -> Vec<BString> {
        self.alias_of.get_dist_urls()
    }

    fn get_dist_reference(&self) -> Option<BString> {
        self.alias_of.get_dist_reference()
    }

    fn set_dist_reference(&mut self, reference: Option<BString>) {
        self.alias_of.set_dist_reference(reference);
    }

    fn get_dist_sha1_checksum(&self) -> Option<BString> {
        self.alias_of.get_dist_sha1_checksum()
    }

    fn set_transport_options(&mut self, options: IndexMap<BString, PhpMixed>) {
        self.alias_of.set_transport_options(options);
    }

    fn get_transport_options(&self) -> IndexMap<BString, PhpMixed> {
        self.alias_of.get_transport_options()
    }

    fn set_dist_mirrors(&mut self, mirrors: Option<Vec<Mirror>>) {
        self.alias_of.set_dist_mirrors(mirrors);
    }

    fn get_dist_mirrors(&self) -> Option<Vec<Mirror>> {
        self.alias_of.get_dist_mirrors()
    }

    fn get_autoload(&self) -> IndexMap<BString, PhpMixed> {
        self.alias_of.get_autoload()
    }

    fn get_dev_autoload(&self) -> IndexMap<BString, PhpMixed> {
        self.alias_of.get_dev_autoload()
    }

    fn get_include_paths(&self) -> Vec<BString> {
        self.alias_of.get_include_paths()
    }

    fn get_php_ext(&self) -> Option<IndexMap<BString, PhpMixed>> {
        self.alias_of.get_php_ext()
    }

    fn get_release_date(&self) -> Option<DateTime<Utc>> {
        self.alias_of.get_release_date()
    }

    fn get_binaries(&self) -> Vec<BString> {
        self.alias_of.get_binaries()
    }

    fn get_suggests(&self) -> IndexMap<BString, BString> {
        self.alias_of.get_suggests()
    }

    fn get_notification_url(&self) -> Option<BString> {
        self.alias_of.get_notification_url()
    }

    fn is_default_branch(&self) -> bool {
        self.alias_of.is_default_branch()
    }

    fn set_source_url(&mut self, url: Option<BString>) {
        self.alias_of.set_source_url(url);
    }

    fn set_dist_url(&mut self, url: Option<BString>) {
        self.alias_of.set_dist_url(url);
    }

    fn set_dist_type(&mut self, r#type: Option<BString>) {
        self.alias_of.set_dist_type(r#type);
    }

    fn set_source_dist_references(&mut self, reference: BString) {
        self.alias_of.set_source_dist_references(reference);
    }

    fn get_full_pretty_version(
        &self,
        truncate: bool,
        display_mode: crate::package::DisplayMode,
    ) -> BString {
        BasePackage::get_full_pretty_version(self, truncate, display_mode)
    }

    fn get_unique_name(&self) -> BString {
        bconcat!(self.get_name(), "-", self.get_version())
    }

    fn to_php_string(&self) -> BString {
        bconcat!(
            self.get_unique_name(),
            " (",
            if self.root_package_alias { "root " } else { "" },
            "alias of ",
            self.alias_of.get_version(),
            ")"
        )
    }

    fn get_pretty_string(&self) -> BString {
        bconcat!(self.get_pretty_name(), " ", self.get_pretty_version())
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
}

impl BasePackage for AliasPackage {
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
