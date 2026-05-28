//! Shared handle over `RepositoryInterface`.

use std::cell::{Ref, RefCell, RefMut};
use std::rc::{Rc, Weak};

use indexmap::IndexMap;
use shirabe_php_shim::Countable;
use shirabe_semver::constraint::AnyConstraint;

use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::repository::{
    FindPackageConstraint, InstalledRepositoryInterface, LoadPackagesResult, LockArrayRepository,
    ProviderInfo, RepositoryInterface, SearchResult, WritableRepositoryInterface,
};

/// Shared reference to a repository. Corresponds to PHP `RepositoryInterface`.
#[derive(Debug, Clone)]
pub struct RepositoryInterfaceHandle(Rc<RefCell<dyn RepositoryInterface>>);

/// Weak back-reference held by packages to the repository that owns them.
pub type RepositoryInterfaceWeakHandle = Weak<RefCell<dyn RepositoryInterface>>;

impl RepositoryInterfaceHandle {
    /// Wraps a concrete repository in a shared handle and injects its own weak reference so that
    /// `add_package` can wire package -> repository back-references (PHP `setRepository($this)`).
    pub fn new<T: RepositoryInterface + 'static>(repository: T) -> Self {
        let rc: Rc<RefCell<dyn RepositoryInterface>> = Rc::new(RefCell::new(repository));
        rc.borrow().set_self_handle(Rc::downgrade(&rc));
        Self(rc)
    }

    pub fn from_rc(rc: Rc<RefCell<dyn RepositoryInterface>>) -> Self {
        Self(rc)
    }

    pub fn as_rc(&self) -> &Rc<RefCell<dyn RepositoryInterface>> {
        &self.0
    }

    pub fn downgrade(&self) -> RepositoryInterfaceWeakHandle {
        Rc::downgrade(&self.0)
    }

    pub fn borrow(&self) -> Ref<'_, dyn RepositoryInterface> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, dyn RepositoryInterface> {
        self.0.borrow_mut()
    }

    /// PHP `===` (reference identity).
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    /// Stable identity usable as a map key (PHP `spl_object_hash`).
    pub fn ptr_id(&self) -> usize {
        Rc::as_ptr(&self.0) as *const () as usize
    }

    /// PHP `instanceof T` for a concrete repository type. Keeps the `RefCell` borrow internal.
    pub fn is<T: RepositoryInterface + 'static>(&self) -> bool {
        self.0.borrow().as_any().is::<T>()
    }

    pub fn count(&self) -> i64 {
        self.0.borrow().count()
    }

    pub fn get_repo_name(&self) -> String {
        self.0.borrow().get_repo_name()
    }

    pub fn get_packages(&self) -> Vec<BasePackageHandle> {
        self.0.borrow().get_packages()
    }

    pub fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        self.0.borrow().has_package(package)
    }

    pub fn find_package(
        &self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> Option<BasePackageHandle> {
        self.0.borrow().find_package(name, constraint)
    }

    pub fn find_packages(
        &self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> Vec<BasePackageHandle> {
        self.0.borrow().find_packages(name, constraint)
    }

    pub fn load_packages(
        &self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> LoadPackagesResult {
        self.0.borrow().load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        )
    }

    pub fn search(&self, query: String, mode: i64, r#type: Option<String>) -> Vec<SearchResult> {
        self.0.borrow().search(query, mode, r#type)
    }

    pub fn get_providers(&self, package_name: String) -> IndexMap<String, ProviderInfo> {
        self.0.borrow().get_providers(package_name)
    }

    // --- InstalledRepositoryInterface helpers (valid only when the wrapped repository is one) ---

    pub fn is_fresh(&self) -> bool {
        self.0
            .borrow()
            .as_installed_repository_interface()
            .map_or(false, |r| r.is_fresh())
    }

    pub fn get_dev_mode(&self) -> Option<bool> {
        self.0
            .borrow()
            .as_installed_repository_interface()
            .and_then(|r| r.get_dev_mode())
    }

    pub fn get_canonical_packages(&self) -> Vec<PackageInterfaceHandle> {
        self.0
            .borrow()
            .as_installed_repository_interface()
            .map(|r| r.get_canonical_packages())
            .unwrap_or_default()
    }

    pub fn get_dev_package_names(&self) -> Vec<String> {
        self.0
            .borrow()
            .as_installed_repository_interface()
            .map(|r| r.get_dev_package_names().clone())
            .unwrap_or_default()
    }

    pub fn set_dev_package_names(&self, dev_package_names: Vec<String>) {
        if let Some(r) = self.0.borrow_mut().as_installed_repository_interface_mut() {
            r.set_dev_package_names(dev_package_names);
        }
    }
}

impl PartialEq for RepositoryInterfaceHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for RepositoryInterfaceHandle {}

impl std::hash::Hash for RepositoryInterfaceHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ptr_id().hash(state);
    }
}

/// Typed shared handle over `LockArrayRepository`. Preserves the PHP `?LockArrayRepository`
/// typing where a `RepositoryInterfaceHandle` would be too wide.
#[derive(Debug, Clone)]
pub struct LockArrayRepositoryHandle(Rc<RefCell<LockArrayRepository>>);

impl LockArrayRepositoryHandle {
    pub fn new(repository: LockArrayRepository) -> Self {
        let rc: Rc<RefCell<LockArrayRepository>> = Rc::new(RefCell::new(repository));
        let rc_dyn: Rc<RefCell<dyn RepositoryInterface>> = rc.clone();
        rc.borrow().set_self_handle(Rc::downgrade(&rc_dyn));
        Self(rc)
    }

    pub fn from_rc(rc: Rc<RefCell<LockArrayRepository>>) -> Self {
        Self(rc)
    }

    pub fn as_rc(&self) -> &Rc<RefCell<LockArrayRepository>> {
        &self.0
    }

    pub fn borrow(&self) -> Ref<'_, LockArrayRepository> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, LockArrayRepository> {
        self.0.borrow_mut()
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn ptr_id(&self) -> usize {
        Rc::as_ptr(&self.0) as *const () as usize
    }
}

impl From<LockArrayRepositoryHandle> for RepositoryInterfaceHandle {
    fn from(h: LockArrayRepositoryHandle) -> Self {
        let rc: Rc<RefCell<dyn RepositoryInterface>> = h.0;
        RepositoryInterfaceHandle::from_rc(rc)
    }
}

impl PartialEq for LockArrayRepositoryHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for LockArrayRepositoryHandle {}

impl std::hash::Hash for LockArrayRepositoryHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ptr_id().hash(state);
    }
}
