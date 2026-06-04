//! Shared handles over the package types.
//!
//! No weak handles are provided: an alias package never aliases another alias package, so the
//! `alias_of` references are acyclic.

use std::cell::RefCell;
use std::rc::Rc;

use crate::package::{
    AliasPackage, CompleteAliasPackage, CompletePackage, CompletePackageInterface, Package,
    PackageInterface, RootAliasPackage, RootPackage, RootPackageInterface,
};

/// Any package type.
#[derive(Debug, Clone)]
pub enum AnyPackage {
    Package(Package),
    CompletePackage(CompletePackage),
    RootPackage(RootPackage),
    AliasPackage(AliasPackage),
    CompleteAliasPackage(CompleteAliasPackage),
    RootAliasPackage(RootAliasPackage),
}

impl AnyPackage {
    pub fn as_package_interface(&self) -> &dyn PackageInterface {
        match self {
            Self::Package(p) => p,
            Self::CompletePackage(p) => p,
            Self::RootPackage(p) => p,
            Self::AliasPackage(p) => p,
            Self::CompleteAliasPackage(p) => p,
            Self::RootAliasPackage(p) => p,
        }
    }

    pub fn as_package_interface_mut(&mut self) -> &mut dyn PackageInterface {
        match self {
            Self::Package(p) => p,
            Self::CompletePackage(p) => p,
            Self::RootPackage(p) => p,
            Self::AliasPackage(p) => p,
            Self::CompleteAliasPackage(p) => p,
            Self::RootAliasPackage(p) => p,
        }
    }

    pub fn as_complete_package_interface(&self) -> Option<&dyn CompletePackageInterface> {
        match self {
            Self::CompletePackage(p) => Some(p),
            Self::RootPackage(p) => Some(p),
            Self::CompleteAliasPackage(p) => Some(p),
            Self::RootAliasPackage(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_complete_package_interface_mut(
        &mut self,
    ) -> Option<&mut dyn CompletePackageInterface> {
        match self {
            Self::CompletePackage(p) => Some(p),
            Self::RootPackage(p) => Some(p),
            Self::CompleteAliasPackage(p) => Some(p),
            Self::RootAliasPackage(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_root_package_interface(&self) -> Option<&dyn RootPackageInterface> {
        match self {
            Self::RootPackage(p) => Some(p),
            Self::RootAliasPackage(p) => Some(p),
            _ => None,
        }
    }

    pub fn as_root_package_interface_mut(&mut self) -> Option<&mut dyn RootPackageInterface> {
        match self {
            Self::RootPackage(p) => Some(p),
            Self::RootAliasPackage(p) => Some(p),
            _ => None,
        }
    }

    /// PHP `$p instanceof AliasPackage`.
    pub fn is_alias(&self) -> bool {
        matches!(
            self,
            Self::AliasPackage(_) | Self::CompleteAliasPackage(_) | Self::RootAliasPackage(_)
        )
    }

    /// PHP `$p instanceof CompletePackageInterface`.
    pub fn is_complete(&self) -> bool {
        matches!(
            self,
            Self::CompletePackage(_)
                | Self::RootPackage(_)
                | Self::CompleteAliasPackage(_)
                | Self::RootAliasPackage(_)
        )
    }

    /// PHP `$p instanceof RootPackageInterface`.
    pub fn is_root(&self) -> bool {
        matches!(self, Self::RootPackage(_) | Self::RootAliasPackage(_))
    }

    /// A real (non-alias) package: `Package` / `CompletePackage` / `RootPackage`.
    pub fn is_real(&self) -> bool {
        matches!(
            self,
            Self::Package(_) | Self::CompletePackage(_) | Self::RootPackage(_)
        )
    }

    /// A real `CompletePackage` or `RootPackage`.
    pub fn is_complete_real(&self) -> bool {
        matches!(self, Self::CompletePackage(_) | Self::RootPackage(_))
    }

    /// A real `RootPackage`.
    pub fn is_root_real(&self) -> bool {
        matches!(self, Self::RootPackage(_))
    }

    /// A `CompleteAliasPackage` or `RootAliasPackage`.
    pub fn is_complete_alias(&self) -> bool {
        matches!(
            self,
            Self::CompleteAliasPackage(_) | Self::RootAliasPackage(_)
        )
    }

    /// A `RootAliasPackage`.
    pub fn is_root_alias(&self) -> bool {
        matches!(self, Self::RootAliasPackage(_))
    }

    /// PHP `clone $package`: fresh object identity. Matches PHP's shallow
    /// clone for most types (scalars/arrays are copied, nested object
    /// references — including `aliasOf` on alias variants — are shared),
    /// except for RootAliasPackage where PHP's `__clone` hook explicitly
    /// reseats `aliasOf` to a fresh clone.
    pub fn dup(&self) -> Self {
        match self {
            Self::Package(p) => Self::Package(p.clone()),
            Self::CompletePackage(p) => Self::CompletePackage(p.clone()),
            Self::RootPackage(p) => Self::RootPackage(p.clone()),
            Self::AliasPackage(p) => Self::AliasPackage(p.clone()),
            Self::CompleteAliasPackage(p) => Self::CompleteAliasPackage(p.clone()),
            Self::RootAliasPackage(p) => {
                // PHP's RootAliasPackage overrides `__clone()`:
                //   $this->aliasOf = clone $this->aliasOf;
                let new_alias_of_inner = p.alias_of.0.borrow().dup();
                let new_alias_of_rc = Rc::new(RefCell::new(new_alias_of_inner));
                let new_root = RootPackageHandle(new_alias_of_rc.clone());
                let new_complete = CompletePackageHandle(new_alias_of_rc.clone());
                let new_pkg = PackageHandle(new_alias_of_rc);

                let mut cloned = p.clone();
                cloned.alias_of = new_root;
                cloned.inner.alias_of = new_complete;
                cloned.inner.inner.alias_of = new_pkg;
                Self::RootAliasPackage(cloned)
            }
        }
    }
}

macro_rules! delegate_package_interface_to_inner {
    ($Type:ty, $field:ident) => {
        impl crate::package::PackageInterface for $Type {
            fn get_name(&self) -> &str {
                self.$field.get_name()
            }
            fn get_pretty_name(&self) -> &str {
                self.$field.get_pretty_name()
            }
            fn get_names(&self, provides: bool) -> Vec<String> {
                self.$field.get_names(provides)
            }
            fn set_id(&mut self, id: i64) {
                self.$field.set_id(id);
            }
            fn get_id(&self) -> i64 {
                self.$field.get_id()
            }
            fn is_dev(&self) -> bool {
                self.$field.is_dev()
            }
            fn get_type(&self) -> &str {
                self.$field.get_type()
            }
            fn get_target_dir(&self) -> Option<String> {
                self.$field.get_target_dir()
            }
            fn get_extra(&self) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
                self.$field.get_extra()
            }
            fn set_installation_source(&mut self, r#type: Option<String>) {
                self.$field.set_installation_source(r#type);
            }
            fn get_installation_source(&self) -> Option<&str> {
                self.$field.get_installation_source()
            }
            fn get_source_type(&self) -> Option<&str> {
                self.$field.get_source_type()
            }
            fn get_source_url(&self) -> Option<&str> {
                self.$field.get_source_url()
            }
            fn get_source_urls(&self) -> Vec<String> {
                self.$field.get_source_urls()
            }
            fn get_source_reference(&self) -> Option<&str> {
                self.$field.get_source_reference()
            }
            fn get_source_mirrors(&self) -> Option<Vec<crate::package::Mirror>> {
                self.$field.get_source_mirrors()
            }
            fn set_source_mirrors(&mut self, mirrors: Option<Vec<crate::package::Mirror>>) {
                self.$field.set_source_mirrors(mirrors);
            }
            fn get_dist_type(&self) -> Option<&str> {
                self.$field.get_dist_type()
            }
            fn get_dist_url(&self) -> Option<&str> {
                self.$field.get_dist_url()
            }
            fn get_dist_urls(&self) -> Vec<String> {
                self.$field.get_dist_urls()
            }
            fn get_dist_reference(&self) -> Option<&str> {
                self.$field.get_dist_reference()
            }
            fn get_dist_sha1_checksum(&self) -> Option<&str> {
                self.$field.get_dist_sha1_checksum()
            }
            fn get_dist_mirrors(&self) -> Option<Vec<crate::package::Mirror>> {
                self.$field.get_dist_mirrors()
            }
            fn set_dist_mirrors(&mut self, mirrors: Option<Vec<crate::package::Mirror>>) {
                self.$field.set_dist_mirrors(mirrors);
            }
            fn get_version(&self) -> &str {
                self.$field.get_version()
            }
            fn get_pretty_version(&self) -> &str {
                self.$field.get_pretty_version()
            }
            fn get_full_pretty_version(
                &self,
                truncate: bool,
                display_mode: crate::package::DisplayMode,
            ) -> String {
                self.$field.get_full_pretty_version(truncate, display_mode)
            }
            fn get_release_date(&self) -> Option<chrono::DateTime<chrono::Utc>> {
                self.$field.get_release_date()
            }
            fn get_stability(&self) -> &str {
                self.$field.get_stability()
            }
            fn get_requires(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.$field.get_requires()
            }
            fn get_conflicts(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.$field.get_conflicts()
            }
            fn get_provides(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.$field.get_provides()
            }
            fn get_replaces(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.$field.get_replaces()
            }
            fn get_dev_requires(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.$field.get_dev_requires()
            }
            fn get_suggests(&self) -> indexmap::IndexMap<String, String> {
                self.$field.get_suggests()
            }
            fn get_autoload(&self) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
                self.$field.get_autoload()
            }
            fn get_dev_autoload(&self) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
                self.$field.get_dev_autoload()
            }
            fn get_include_paths(&self) -> Vec<String> {
                self.$field.get_include_paths()
            }
            fn get_php_ext(
                &self,
            ) -> Option<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>> {
                self.$field.get_php_ext()
            }
            fn set_repository(
                &mut self,
                repository: crate::repository::RepositoryInterfaceHandle,
            ) -> anyhow::Result<()> {
                self.$field.set_repository(repository)
            }
            fn get_repository(&self) -> Option<crate::repository::RepositoryInterfaceHandle> {
                self.$field.get_repository()
            }
            fn get_binaries(&self) -> Vec<String> {
                self.$field.get_binaries()
            }
            fn get_unique_name(&self) -> String {
                self.$field.get_unique_name()
            }
            fn get_notification_url(&self) -> Option<&str> {
                self.$field.get_notification_url()
            }
            fn get_pretty_string(&self) -> String {
                self.$field.get_pretty_string()
            }
            fn is_default_branch(&self) -> bool {
                self.$field.is_default_branch()
            }
            fn get_transport_options(
                &self,
            ) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
                self.$field.get_transport_options()
            }
            fn set_transport_options(
                &mut self,
                options: indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>,
            ) {
                self.$field.set_transport_options(options);
            }
            fn set_source_reference(&mut self, reference: Option<String>) {
                self.$field.set_source_reference(reference);
            }
            fn set_source_url(&mut self, url: Option<String>) {
                self.$field.set_source_url(url);
            }
            fn set_dist_url(&mut self, url: Option<String>) {
                self.$field.set_dist_url(url);
            }
            fn set_dist_type(&mut self, r#type: Option<String>) {
                self.$field.set_dist_type(r#type);
            }
            fn set_dist_reference(&mut self, reference: Option<String>) {
                self.$field.set_dist_reference(reference);
            }
            fn set_source_dist_references(&mut self, reference: &str) {
                self.$field.set_source_dist_references(reference);
            }
        }
    };
}
pub(crate) use delegate_package_interface_to_inner;

macro_rules! impl_package_interface_handle {
    ($Handle:ty) => {
        impl $Handle {
            pub fn get_name(&self) -> String {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_name()
                    .to_string()
            }

            pub fn get_pretty_name(&self) -> String {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_pretty_name()
                    .to_string()
            }

            pub fn get_names(&self, provides: bool) -> Vec<String> {
                self.0.borrow().as_package_interface().get_names(provides)
            }

            pub fn set_id(&self, id: i64) {
                self.0.borrow_mut().as_package_interface_mut().set_id(id);
            }

            pub fn get_id(&self) -> i64 {
                self.0.borrow().as_package_interface().get_id()
            }

            /// PHP `BasePackage::$id` accessor; alias of [`get_id`](Self::get_id).
            pub fn id(&self) -> i64 {
                self.0.borrow().as_package_interface().get_id()
            }

            pub fn is_dev(&self) -> bool {
                self.0.borrow().as_package_interface().is_dev()
            }

            pub fn get_type(&self) -> String {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_type()
                    .to_string()
            }

            pub fn get_target_dir(&self) -> Option<String> {
                self.0.borrow().as_package_interface().get_target_dir()
            }

            pub fn get_extra(&self) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
                self.0.borrow().as_package_interface().get_extra()
            }

            pub fn set_installation_source(&self, r#type: Option<String>) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_installation_source(r#type);
            }

            pub fn get_installation_source(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_installation_source()
                    .map(str::to_string)
            }

            pub fn get_source_type(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_source_type()
                    .map(str::to_string)
            }

            pub fn get_source_url(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_source_url()
                    .map(str::to_string)
            }

            pub fn get_source_urls(&self) -> Vec<String> {
                self.0.borrow().as_package_interface().get_source_urls()
            }

            pub fn get_source_reference(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_source_reference()
                    .map(str::to_string)
            }

            pub fn get_source_mirrors(&self) -> Option<Vec<crate::package::Mirror>> {
                self.0.borrow().as_package_interface().get_source_mirrors()
            }

            pub fn set_source_mirrors(&self, mirrors: Option<Vec<crate::package::Mirror>>) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_source_mirrors(mirrors);
            }

            pub fn get_dist_type(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_dist_type()
                    .map(str::to_string)
            }

            pub fn get_dist_url(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_dist_url()
                    .map(str::to_string)
            }

            pub fn get_dist_urls(&self) -> Vec<String> {
                self.0.borrow().as_package_interface().get_dist_urls()
            }

            pub fn get_dist_reference(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_dist_reference()
                    .map(str::to_string)
            }

            pub fn get_dist_sha1_checksum(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_dist_sha1_checksum()
                    .map(str::to_string)
            }

            pub fn get_dist_mirrors(&self) -> Option<Vec<crate::package::Mirror>> {
                self.0.borrow().as_package_interface().get_dist_mirrors()
            }

            pub fn set_dist_mirrors(&self, mirrors: Option<Vec<crate::package::Mirror>>) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_dist_mirrors(mirrors);
            }

            pub fn get_version(&self) -> String {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_version()
                    .to_string()
            }

            pub fn get_pretty_version(&self) -> String {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_pretty_version()
                    .to_string()
            }

            pub fn get_full_pretty_version(
                &self,
                truncate: bool,
                display_mode: crate::package::DisplayMode,
            ) -> String {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_full_pretty_version(truncate, display_mode)
            }

            pub fn get_release_date(&self) -> Option<chrono::DateTime<chrono::Utc>> {
                self.0.borrow().as_package_interface().get_release_date()
            }

            pub fn get_stability(&self) -> String {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_stability()
                    .to_string()
            }

            pub fn get_requires(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.0.borrow().as_package_interface().get_requires()
            }

            pub fn get_conflicts(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.0.borrow().as_package_interface().get_conflicts()
            }

            pub fn get_provides(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.0.borrow().as_package_interface().get_provides()
            }

            pub fn get_replaces(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.0.borrow().as_package_interface().get_replaces()
            }

            pub fn get_dev_requires(&self) -> indexmap::IndexMap<String, crate::package::Link> {
                self.0.borrow().as_package_interface().get_dev_requires()
            }

            pub fn get_suggests(&self) -> indexmap::IndexMap<String, String> {
                self.0.borrow().as_package_interface().get_suggests()
            }

            pub fn get_links_for_type(
                &self,
                link_type: &str,
            ) -> indexmap::IndexMap<String, crate::package::Link> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_links_for_type(link_type)
            }

            pub fn get_autoload(&self) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
                self.0.borrow().as_package_interface().get_autoload()
            }

            pub fn get_dev_autoload(
                &self,
            ) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
                self.0.borrow().as_package_interface().get_dev_autoload()
            }

            pub fn get_include_paths(&self) -> Vec<String> {
                self.0.borrow().as_package_interface().get_include_paths()
            }

            pub fn get_php_ext(
                &self,
            ) -> Option<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>> {
                self.0.borrow().as_package_interface().get_php_ext()
            }

            pub fn set_repository(
                &self,
                repository: crate::repository::RepositoryInterfaceHandle,
            ) -> anyhow::Result<()> {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_repository(repository)
            }

            pub fn get_repository(&self) -> Option<crate::repository::RepositoryInterfaceHandle> {
                self.0.borrow().as_package_interface().get_repository()
            }

            pub fn get_binaries(&self) -> Vec<String> {
                self.0.borrow().as_package_interface().get_binaries()
            }

            pub fn get_unique_name(&self) -> String {
                self.0.borrow().as_package_interface().get_unique_name()
            }

            pub fn get_notification_url(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_notification_url()
                    .map(str::to_string)
            }

            pub fn get_pretty_string(&self) -> String {
                self.0.borrow().as_package_interface().get_pretty_string()
            }

            pub fn is_default_branch(&self) -> bool {
                self.0.borrow().as_package_interface().is_default_branch()
            }

            pub fn get_transport_options(
                &self,
            ) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
                self.0
                    .borrow()
                    .as_package_interface()
                    .get_transport_options()
            }

            pub fn set_transport_options(
                &self,
                options: indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>,
            ) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_transport_options(options);
            }

            pub fn set_source_reference(&self, reference: Option<String>) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_source_reference(reference);
            }

            pub fn set_source_url(&self, url: Option<String>) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_source_url(url);
            }

            pub fn set_dist_url(&self, url: Option<String>) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_dist_url(url);
            }

            pub fn set_dist_type(&self, r#type: Option<String>) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_dist_type(r#type);
            }

            pub fn set_dist_reference(&self, reference: Option<String>) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_dist_reference(reference);
            }

            pub fn set_source_dist_references(&self, reference: &str) {
                self.0
                    .borrow_mut()
                    .as_package_interface_mut()
                    .set_source_dist_references(reference);
            }
        }
    };
}

macro_rules! impl_complete_package_interface_handle {
    ($Handle:ty) => {
        impl $Handle {
            pub fn get_scripts(&self) -> indexmap::IndexMap<String, Vec<String>> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_scripts()
            }

            pub fn set_scripts(&self, scripts: indexmap::IndexMap<String, Vec<String>>) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_scripts(scripts);
            }

            pub fn get_repositories(
                &self,
            ) -> Vec<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_repositories()
            }

            pub fn set_repositories(
                &self,
                repositories: Vec<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>>,
            ) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_repositories(repositories);
            }

            pub fn get_license(&self) -> Vec<String> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_license()
            }

            pub fn set_license(&self, license: Vec<String>) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_license(license);
            }

            pub fn get_keywords(&self) -> Vec<String> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_keywords()
            }

            pub fn set_keywords(&self, keywords: Vec<String>) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_keywords(keywords);
            }

            pub fn get_description(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_description()
                    .map(str::to_string)
            }

            pub fn set_description(&self, description: String) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_description(description);
            }

            pub fn get_homepage(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_homepage()
                    .map(str::to_string)
            }

            pub fn set_homepage(&self, homepage: String) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_homepage(homepage);
            }

            pub fn get_authors(&self) -> Vec<indexmap::IndexMap<String, String>> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_authors()
            }

            pub fn set_authors(&self, authors: Vec<indexmap::IndexMap<String, String>>) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_authors(authors);
            }

            pub fn get_support(&self) -> indexmap::IndexMap<String, String> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_support()
            }

            pub fn set_support(&self, support: indexmap::IndexMap<String, String>) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_support(support);
            }

            pub fn get_funding(
                &self,
            ) -> Vec<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_funding()
            }

            pub fn set_funding(
                &self,
                funding: Vec<indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>>,
            ) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_funding(funding);
            }

            pub fn is_abandoned(&self) -> bool {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .is_abandoned()
            }

            pub fn get_replacement_package(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_replacement_package()
                    .map(str::to_string)
            }

            pub fn set_abandoned(&self, abandoned: shirabe_php_shim::PhpMixed) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_abandoned(abandoned);
            }

            pub fn get_archive_name(&self) -> Option<String> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_archive_name()
                    .map(str::to_string)
            }

            pub fn set_archive_name(&self, name: String) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_archive_name(name);
            }

            pub fn get_archive_excludes(&self) -> Vec<String> {
                self.0
                    .borrow()
                    .as_complete_package_interface()
                    .expect("CompletePackage handle invariant")
                    .get_archive_excludes()
            }

            pub fn set_archive_excludes(&self, excludes: Vec<String>) {
                self.0
                    .borrow_mut()
                    .as_complete_package_interface_mut()
                    .expect("CompletePackage handle invariant")
                    .set_archive_excludes(excludes);
            }
        }
    };
}

macro_rules! impl_root_package_interface_handle {
    ($Handle:ty) => {
        impl $Handle {
            pub fn get_aliases(&self) -> Vec<indexmap::IndexMap<String, String>> {
                self.0
                    .borrow()
                    .as_root_package_interface()
                    .expect("RootPackage handle invariant")
                    .get_aliases()
                    .to_vec()
            }

            pub fn get_minimum_stability(&self) -> String {
                self.0
                    .borrow()
                    .as_root_package_interface()
                    .expect("RootPackage handle invariant")
                    .get_minimum_stability()
                    .to_string()
            }

            pub fn get_stability_flags(&self) -> indexmap::IndexMap<String, i64> {
                self.0
                    .borrow()
                    .as_root_package_interface()
                    .expect("RootPackage handle invariant")
                    .get_stability_flags()
                    .clone()
            }

            pub fn get_references(&self) -> indexmap::IndexMap<String, String> {
                self.0
                    .borrow()
                    .as_root_package_interface()
                    .expect("RootPackage handle invariant")
                    .get_references()
                    .clone()
            }

            pub fn get_prefer_stable(&self) -> bool {
                self.0
                    .borrow()
                    .as_root_package_interface()
                    .expect("RootPackage handle invariant")
                    .get_prefer_stable()
            }

            pub fn get_config(&self) -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
                self.0
                    .borrow()
                    .as_root_package_interface()
                    .expect("RootPackage handle invariant")
                    .get_config()
                    .clone()
            }

            pub fn set_requires(&self, requires: indexmap::IndexMap<String, crate::package::Link>) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_requires(requires);
            }

            pub fn set_dev_requires(
                &self,
                dev_requires: indexmap::IndexMap<String, crate::package::Link>,
            ) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_dev_requires(dev_requires);
            }

            pub fn set_conflicts(
                &self,
                conflicts: indexmap::IndexMap<String, crate::package::Link>,
            ) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_conflicts(conflicts);
            }

            pub fn set_provides(&self, provides: indexmap::IndexMap<String, crate::package::Link>) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_provides(provides);
            }

            pub fn set_replaces(&self, replaces: indexmap::IndexMap<String, crate::package::Link>) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_replaces(replaces);
            }

            pub fn set_autoload(
                &self,
                autoload: indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>,
            ) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_autoload(autoload);
            }

            pub fn set_dev_autoload(
                &self,
                dev_autoload: indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>,
            ) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_dev_autoload(dev_autoload);
            }

            pub fn set_stability_flags(&self, stability_flags: indexmap::IndexMap<String, i64>) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_stability_flags(stability_flags);
            }

            pub fn set_minimum_stability(&self, minimum_stability: String) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_minimum_stability(minimum_stability);
            }

            pub fn set_prefer_stable(&self, prefer_stable: bool) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_prefer_stable(prefer_stable);
            }

            pub fn set_config(
                &self,
                config: indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>,
            ) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_config(config);
            }

            pub fn set_references(&self, references: indexmap::IndexMap<String, String>) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_references(references);
            }

            pub fn set_aliases(&self, aliases: Vec<indexmap::IndexMap<String, String>>) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_aliases(aliases);
            }

            pub fn set_suggests(&self, suggests: indexmap::IndexMap<String, String>) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_suggests(suggests);
            }

            pub fn set_extra(&self, extra: indexmap::IndexMap<String, shirabe_php_shim::PhpMixed>) {
                self.0
                    .borrow_mut()
                    .as_root_package_interface_mut()
                    .expect("RootPackage handle invariant")
                    .set_extra(extra);
            }
        }
    };
}

macro_rules! impl_handle_common {
    ($Handle:ty) => {
        impl $Handle {
            pub fn as_rc(&self) -> &std::rc::Rc<std::cell::RefCell<AnyPackage>> {
                &self.0
            }

            pub fn from_rc_unchecked(rc: std::rc::Rc<std::cell::RefCell<AnyPackage>>) -> Self {
                Self(rc)
            }

            /// Stable identity usable as a map key (PHP `spl_object_hash`).
            pub fn ptr_id(&self) -> usize {
                std::rc::Rc::as_ptr(&self.0) as *const () as usize
            }

            /// PHP `===` (reference identity).
            pub fn ptr_eq(&self, other: &Self) -> bool {
                std::rc::Rc::ptr_eq(&self.0, &other.0)
            }

            /// PHP `clone $x`: fresh object identity. See [`AnyPackage::dup`]
            /// for the per-variant semantics (including the RootAliasPackage
            /// `__clone` hook).
            pub fn dup(other: &Self) -> Self {
                Self(std::rc::Rc::new(std::cell::RefCell::new(
                    other.0.borrow().dup(),
                )))
            }
        }

        impl PartialEq for $Handle {
            fn eq(&self, other: &Self) -> bool {
                std::rc::Rc::ptr_eq(&self.0, &other.0)
            }
        }

        impl Eq for $Handle {}

        impl std::hash::Hash for $Handle {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.ptr_id().hash(state);
            }
        }

        impl std::fmt::Display for $Handle {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                std::fmt::Display::fmt(self.0.borrow().as_package_interface(), f)
            }
        }
    };
}

macro_rules! impl_handle_upcast {
    ($Narrow:ty => $Wide:ty) => {
        impl From<$Narrow> for $Wide {
            fn from(h: $Narrow) -> Self {
                Self(h.0)
            }
        }
    };
}

/// Shared reference to any package. Corresponds to PHP `PackageInterface`.
#[derive(Debug, Clone)]
pub struct PackageInterfaceHandle(Rc<RefCell<AnyPackage>>);

/// Shared reference to any package. Corresponds to PHP `BasePackage`.
/// It is exactly the same as `PackageInterface` in Shirabe. It is only for mirroing PHP type
/// annotations.
pub type BasePackageHandle = PackageInterfaceHandle;

/// Shared reference to a complete package. Corresponds to PHP `CompletePackageInterface`.
#[derive(Debug, Clone)]
pub struct CompletePackageInterfaceHandle(Rc<RefCell<AnyPackage>>);

/// Shared reference to a root package. Corresponds to PHP `RootPackageInterface`.
#[derive(Debug, Clone)]
pub struct RootPackageInterfaceHandle(Rc<RefCell<AnyPackage>>);

/// Shared reference to a real (non-alias) package. Corresponds to PHP `Package`.
#[derive(Debug, Clone)]
pub struct PackageHandle(Rc<RefCell<AnyPackage>>);

/// Shared reference to a real complete package. Corresponds to PHP `CompletePackage`.
#[derive(Debug, Clone)]
pub struct CompletePackageHandle(Rc<RefCell<AnyPackage>>);

/// Shared reference to a real root package. Corresponds to PHP `RootPackage`.
#[derive(Debug, Clone)]
pub struct RootPackageHandle(Rc<RefCell<AnyPackage>>);

/// Shared reference to an alias package. Corresponds to PHP `AliasPackage`.
#[derive(Debug, Clone)]
pub struct AliasPackageHandle(Rc<RefCell<AnyPackage>>);

/// Shared reference to a complete alias package. Corresponds to PHP `CompleteAliasPackage`.
#[derive(Debug, Clone)]
pub struct CompleteAliasPackageHandle(Rc<RefCell<AnyPackage>>);

/// Shared reference to a root alias package. Corresponds to PHP `RootAliasPackage`.
#[derive(Debug, Clone)]
pub struct RootAliasPackageHandle(Rc<RefCell<AnyPackage>>);

impl_handle_common!(PackageInterfaceHandle);
impl_handle_common!(CompletePackageInterfaceHandle);
impl_handle_common!(RootPackageInterfaceHandle);
impl_handle_common!(PackageHandle);
impl_handle_common!(CompletePackageHandle);
impl_handle_common!(RootPackageHandle);
impl_handle_common!(AliasPackageHandle);
impl_handle_common!(CompleteAliasPackageHandle);
impl_handle_common!(RootAliasPackageHandle);

impl_package_interface_handle!(PackageInterfaceHandle);
impl_package_interface_handle!(CompletePackageInterfaceHandle);
impl_package_interface_handle!(RootPackageInterfaceHandle);
impl_package_interface_handle!(PackageHandle);
impl_package_interface_handle!(CompletePackageHandle);
impl_package_interface_handle!(RootPackageHandle);
impl_package_interface_handle!(AliasPackageHandle);
impl_package_interface_handle!(CompleteAliasPackageHandle);
impl_package_interface_handle!(RootAliasPackageHandle);

impl_complete_package_interface_handle!(CompletePackageInterfaceHandle);
impl_complete_package_interface_handle!(RootPackageInterfaceHandle);
impl_complete_package_interface_handle!(CompletePackageHandle);
impl_complete_package_interface_handle!(RootPackageHandle);
impl_complete_package_interface_handle!(CompleteAliasPackageHandle);
impl_complete_package_interface_handle!(RootAliasPackageHandle);

impl_root_package_interface_handle!(RootPackageInterfaceHandle);
impl_root_package_interface_handle!(RootPackageHandle);
impl_root_package_interface_handle!(RootAliasPackageHandle);

impl_handle_upcast!(CompletePackageInterfaceHandle => PackageInterfaceHandle);

impl_handle_upcast!(RootPackageInterfaceHandle => CompletePackageInterfaceHandle);
impl_handle_upcast!(RootPackageInterfaceHandle => PackageInterfaceHandle);

impl_handle_upcast!(PackageHandle => PackageInterfaceHandle);

impl_handle_upcast!(CompletePackageHandle => PackageHandle);
impl_handle_upcast!(CompletePackageHandle => CompletePackageInterfaceHandle);
impl_handle_upcast!(CompletePackageHandle => PackageInterfaceHandle);

impl_handle_upcast!(RootPackageHandle => CompletePackageHandle);
impl_handle_upcast!(RootPackageHandle => PackageHandle);
impl_handle_upcast!(RootPackageHandle => RootPackageInterfaceHandle);
impl_handle_upcast!(RootPackageHandle => CompletePackageInterfaceHandle);
impl_handle_upcast!(RootPackageHandle => PackageInterfaceHandle);

impl_handle_upcast!(AliasPackageHandle => PackageInterfaceHandle);

impl_handle_upcast!(CompleteAliasPackageHandle => AliasPackageHandle);
impl_handle_upcast!(CompleteAliasPackageHandle => CompletePackageInterfaceHandle);
impl_handle_upcast!(CompleteAliasPackageHandle => PackageInterfaceHandle);

impl_handle_upcast!(RootAliasPackageHandle => CompleteAliasPackageHandle);
impl_handle_upcast!(RootAliasPackageHandle => AliasPackageHandle);
impl_handle_upcast!(RootAliasPackageHandle => RootPackageInterfaceHandle);
impl_handle_upcast!(RootAliasPackageHandle => CompletePackageInterfaceHandle);
impl_handle_upcast!(RootAliasPackageHandle => PackageInterfaceHandle);

macro_rules! impl_handle_downcasts {
    ($Handle:ty) => {
        impl $Handle {
            /// PHP `$p instanceof AliasPackage`.
            pub fn as_alias(&self) -> Option<AliasPackageHandle> {
                self.0
                    .borrow()
                    .is_alias()
                    .then(|| AliasPackageHandle(self.0.clone()))
            }

            /// PHP `$p instanceof CompletePackageInterface`.
            pub fn as_complete(&self) -> Option<CompletePackageInterfaceHandle> {
                self.0
                    .borrow()
                    .is_complete()
                    .then(|| CompletePackageInterfaceHandle(self.0.clone()))
            }

            /// PHP `$p instanceof RootPackageInterface`.
            pub fn as_root(&self) -> Option<RootPackageInterfaceHandle> {
                self.0
                    .borrow()
                    .is_root()
                    .then(|| RootPackageInterfaceHandle(self.0.clone()))
            }

            /// PHP `$p instanceof Package` (real, non-alias).
            pub fn as_package(&self) -> Option<PackageHandle> {
                self.0
                    .borrow()
                    .is_real()
                    .then(|| PackageHandle(self.0.clone()))
            }

            /// PHP `$p instanceof CompletePackage` (real).
            pub fn as_complete_package(&self) -> Option<CompletePackageHandle> {
                self.0
                    .borrow()
                    .is_complete_real()
                    .then(|| CompletePackageHandle(self.0.clone()))
            }

            /// PHP `$p instanceof RootPackage` (real).
            pub fn as_root_package(&self) -> Option<RootPackageHandle> {
                self.0
                    .borrow()
                    .is_root_real()
                    .then(|| RootPackageHandle(self.0.clone()))
            }

            /// PHP `$p instanceof CompleteAliasPackage`.
            pub fn as_complete_alias_package(&self) -> Option<CompleteAliasPackageHandle> {
                self.0
                    .borrow()
                    .is_complete_alias()
                    .then(|| CompleteAliasPackageHandle(self.0.clone()))
            }

            /// PHP `$p instanceof RootAliasPackage`.
            pub fn as_root_alias_package(&self) -> Option<RootAliasPackageHandle> {
                self.0
                    .borrow()
                    .is_root_alias()
                    .then(|| RootAliasPackageHandle(self.0.clone()))
            }

            pub fn is_alias(&self) -> bool {
                self.0.borrow().is_alias()
            }
        }
    };
}

impl_handle_downcasts!(PackageInterfaceHandle);

impl PackageHandle {
    pub fn from_package(package: Package) -> Self {
        Self(Rc::new(RefCell::new(AnyPackage::Package(package))))
    }

    pub fn new(name: String, version: String, pretty_version: String) -> Self {
        Self::from_package(Package::new(name, version, pretty_version))
    }
}

impl CompletePackageHandle {
    pub fn from_complete_package(package: CompletePackage) -> Self {
        Self(Rc::new(RefCell::new(AnyPackage::CompletePackage(package))))
    }

    pub fn new(name: String, version: String, pretty_version: String) -> Self {
        Self::from_complete_package(CompletePackage::new(name, version, pretty_version))
    }
}

impl RootPackageHandle {
    pub fn from_root_package(package: RootPackage) -> Self {
        Self(Rc::new(RefCell::new(AnyPackage::RootPackage(package))))
    }

    pub fn new(name: String, version: String, pretty_version: String) -> Self {
        Self::from_root_package(RootPackage::new(name, version, pretty_version))
    }
}

impl AliasPackageHandle {
    pub fn from_alias_package(package: AliasPackage) -> Self {
        Self(Rc::new(RefCell::new(AnyPackage::AliasPackage(package))))
    }

    pub fn new(alias_of: PackageHandle, version: String, pretty_version: String) -> Self {
        Self::from_alias_package(AliasPackage::new(alias_of, version, pretty_version))
    }

    /// PHP `getAliasOf()`. The aliased package is always real.
    pub fn get_alias_of(&self) -> PackageHandle {
        match &*self.0.borrow() {
            AnyPackage::AliasPackage(p) => p.alias_of.clone(),
            AnyPackage::CompleteAliasPackage(p) => PackageHandle::from(p.alias_of.clone()),
            AnyPackage::RootAliasPackage(p) => PackageHandle::from(p.alias_of.clone()),
            _ => unreachable!("AliasPackageHandle invariant violated"),
        }
    }

    pub fn set_root_package_alias(&self, value: bool) {
        match &mut *self.0.borrow_mut() {
            AnyPackage::AliasPackage(p) => p.set_root_package_alias(value),
            AnyPackage::CompleteAliasPackage(p) => p.set_root_package_alias(value),
            AnyPackage::RootAliasPackage(p) => p.set_root_package_alias(value),
            _ => unreachable!("AliasPackageHandle invariant violated"),
        }
    }

    pub fn is_root_package_alias(&self) -> bool {
        match &*self.0.borrow() {
            AnyPackage::AliasPackage(p) => p.is_root_package_alias(),
            AnyPackage::CompleteAliasPackage(p) => p.is_root_package_alias(),
            AnyPackage::RootAliasPackage(p) => p.is_root_package_alias(),
            _ => unreachable!("AliasPackageHandle invariant violated"),
        }
    }

    pub fn has_self_version_requires(&self) -> bool {
        match &*self.0.borrow() {
            AnyPackage::AliasPackage(p) => p.has_self_version_requires(),
            AnyPackage::CompleteAliasPackage(p) => p.has_self_version_requires(),
            AnyPackage::RootAliasPackage(p) => p.has_self_version_requires(),
            _ => unreachable!("AliasPackageHandle invariant violated"),
        }
    }
}

impl CompleteAliasPackageHandle {
    pub fn from_complete_alias_package(package: CompleteAliasPackage) -> Self {
        Self(Rc::new(RefCell::new(AnyPackage::CompleteAliasPackage(
            package,
        ))))
    }

    pub fn new(alias_of: CompletePackageHandle, version: String, pretty_version: String) -> Self {
        Self::from_complete_alias_package(CompleteAliasPackage::new(
            alias_of,
            version,
            pretty_version,
        ))
    }

    /// PHP `getAliasOf()` narrowed to `CompletePackage`.
    pub fn get_alias_of(&self) -> CompletePackageHandle {
        match &*self.0.borrow() {
            AnyPackage::CompleteAliasPackage(p) => p.alias_of.clone(),
            AnyPackage::RootAliasPackage(p) => CompletePackageHandle::from(p.alias_of.clone()),
            _ => unreachable!("CompleteAliasPackageHandle invariant violated"),
        }
    }

    pub fn set_root_package_alias(&self, value: bool) {
        match &mut *self.0.borrow_mut() {
            AnyPackage::CompleteAliasPackage(p) => p.set_root_package_alias(value),
            AnyPackage::RootAliasPackage(p) => p.set_root_package_alias(value),
            _ => unreachable!("CompleteAliasPackageHandle invariant violated"),
        }
    }
}

impl RootAliasPackageHandle {
    pub fn from_root_alias_package(package: RootAliasPackage) -> Self {
        Self(Rc::new(RefCell::new(AnyPackage::RootAliasPackage(package))))
    }

    pub fn new(alias_of: RootPackageHandle, version: String, pretty_version: String) -> Self {
        Self::from_root_alias_package(RootAliasPackage::new(alias_of, version, pretty_version))
    }

    /// PHP `getAliasOf()` narrowed to `RootPackage`.
    pub fn get_alias_of(&self) -> RootPackageHandle {
        match &*self.0.borrow() {
            AnyPackage::RootAliasPackage(p) => p.alias_of.clone(),
            _ => unreachable!("RootAliasPackageHandle invariant violated"),
        }
    }
}
