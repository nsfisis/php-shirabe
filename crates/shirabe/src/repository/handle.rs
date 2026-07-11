//! Shared handle over `RepositoryInterface`.

use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::repository::{
    FindPackageConstraint, LoadPackagesResult, LockArrayRepository, PlatformRepository,
    ProviderInfo, RepositoryInterface, SearchResult,
};
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;
use std::cell::{Ref, RefMut};
use std::rc::Weak;

/// Shared reference to a repository. Corresponds to PHP `RepositoryInterface`.
#[derive(Debug, Clone)]
pub struct RepositoryInterfaceHandle(std::rc::Rc<std::cell::RefCell<dyn RepositoryInterface>>);

/// Weak back-reference held by packages to the repository that owns them.
pub type RepositoryInterfaceWeakHandle = Weak<std::cell::RefCell<dyn RepositoryInterface>>;

impl RepositoryInterfaceHandle {
    /// Wraps a concrete repository in a shared handle and injects its own weak reference so that
    /// `add_package` can wire package -> repository back-references (PHP `setRepository($this)`).
    pub fn new<T: RepositoryInterface + 'static>(repository: T) -> Self {
        let rc: std::rc::Rc<std::cell::RefCell<dyn RepositoryInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(repository));
        rc.borrow().set_self_handle(std::rc::Rc::downgrade(&rc));
        Self(rc)
    }

    pub fn from_rc(rc: std::rc::Rc<std::cell::RefCell<dyn RepositoryInterface>>) -> Self {
        Self(rc)
    }

    pub fn as_rc(&self) -> &std::rc::Rc<std::cell::RefCell<dyn RepositoryInterface>> {
        &self.0
    }

    pub fn downgrade(&self) -> RepositoryInterfaceWeakHandle {
        std::rc::Rc::downgrade(&self.0)
    }

    pub fn borrow(&self) -> Ref<'_, dyn RepositoryInterface> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, dyn RepositoryInterface> {
        self.0.borrow_mut()
    }

    /// PHP `===` (reference identity).
    pub fn ptr_eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }

    /// Stable identity usable as a map key (PHP `spl_object_hash`).
    pub fn ptr_id(&self) -> usize {
        std::rc::Rc::as_ptr(&self.0) as *const () as usize
    }

    /// PHP `instanceof T` for a concrete repository type. Keeps the `RefCell` borrow internal.
    pub fn is<T: RepositoryInterface + 'static>(&self) -> bool {
        self.0.borrow().as_any().is::<T>()
    }

    /// Downcasts the shared handle to a concrete repository type, preserving shared ownership.
    pub fn downcast_rc<T: RepositoryInterface + 'static>(
        &self,
    ) -> Option<std::rc::Rc<std::cell::RefCell<T>>> {
        if self.0.borrow().as_any().is::<T>() {
            let rc = self.0.clone();
            let ptr = std::rc::Rc::into_raw(rc) as *const std::cell::RefCell<T>;
            // SAFETY: is::<T>() proved the value is `T`, and handles are always allocated as
            // `Rc::new(RefCell::new(concrete))`, so the layout matches `RcBox<RefCell<T>>`.
            Some(unsafe { std::rc::Rc::from_raw(ptr) })
        } else {
            None
        }
    }

    pub fn as_platform_repository(&self) -> Option<PlatformRepositoryHandle> {
        self.downcast_rc::<PlatformRepository>()
            .map(PlatformRepositoryHandle::from_rc)
    }

    pub fn count(&self) -> anyhow::Result<usize> {
        self.0.borrow().count()
    }

    pub fn get_repo_name(&self) -> String {
        self.0.borrow().get_repo_name()
    }

    pub fn get_packages(&self) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.0.borrow_mut().get_packages()
    }

    pub fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        self.0.borrow().has_package(package)
    }

    pub fn find_package(
        &self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        self.0.borrow_mut().find_package(name, constraint)
    }

    pub fn find_packages(
        &self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        self.0.borrow_mut().find_packages(name, constraint)
    }

    pub fn load_packages(
        &self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        self.0.borrow_mut().load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        )
    }

    pub fn search(
        &self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        self.0.borrow_mut().search(query, mode, r#type)
    }

    pub fn get_providers(
        &self,
        package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>> {
        self.0.borrow_mut().get_providers(package_name)
    }

    // --- InstalledRepositoryInterface helpers (valid only when the wrapped repository is one) ---

    /// PHP `$repository instanceof InstalledRepositoryInterface`.
    pub fn is_installed_repository_interface(&self) -> bool {
        self.0
            .borrow()
            .as_installed_repository_interface()
            .is_some()
    }

    pub fn is_fresh(&self) -> bool {
        self.0
            .borrow()
            .as_installed_repository_interface()
            .is_some_and(|r| r.is_fresh())
    }

    pub fn get_dev_mode(&self) -> Option<bool> {
        self.0
            .borrow()
            .as_installed_repository_interface()
            .and_then(|r| r.get_dev_mode())
    }

    pub fn get_canonical_packages(&self) -> anyhow::Result<Vec<PackageInterfaceHandle>> {
        match self.0.borrow_mut().as_installed_repository_interface_mut() {
            Some(r) => r.get_canonical_packages(),
            None => Ok(Vec::new()),
        }
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
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for RepositoryInterfaceHandle {}

impl std::hash::Hash for RepositoryInterfaceHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ptr_id().hash(state);
    }
}

/// Typed shared handle over `LockArrayRepository`.
#[derive(Debug, Clone)]
pub struct LockArrayRepositoryHandle(std::rc::Rc<std::cell::RefCell<LockArrayRepository>>);

impl LockArrayRepositoryHandle {
    pub fn new(repository: LockArrayRepository) -> Self {
        let rc: std::rc::Rc<std::cell::RefCell<LockArrayRepository>> =
            std::rc::Rc::new(std::cell::RefCell::new(repository));
        let rc_dyn: std::rc::Rc<std::cell::RefCell<dyn RepositoryInterface>> = rc.clone();
        rc.borrow().set_self_handle(std::rc::Rc::downgrade(&rc_dyn));
        Self(rc)
    }

    pub fn from_rc(rc: std::rc::Rc<std::cell::RefCell<LockArrayRepository>>) -> Self {
        Self(rc)
    }

    pub fn as_rc(&self) -> &std::rc::Rc<std::cell::RefCell<LockArrayRepository>> {
        &self.0
    }

    pub fn borrow(&self) -> Ref<'_, LockArrayRepository> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, LockArrayRepository> {
        self.0.borrow_mut()
    }

    pub fn add_package(&self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        self.0.borrow().add_package(package)
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn ptr_id(&self) -> usize {
        std::rc::Rc::as_ptr(&self.0) as *const () as usize
    }
}

impl From<LockArrayRepositoryHandle> for RepositoryInterfaceHandle {
    fn from(h: LockArrayRepositoryHandle) -> Self {
        let rc: std::rc::Rc<std::cell::RefCell<dyn RepositoryInterface>> = h.0;
        RepositoryInterfaceHandle::from_rc(rc)
    }
}

impl PartialEq for LockArrayRepositoryHandle {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for LockArrayRepositoryHandle {}

impl std::hash::Hash for LockArrayRepositoryHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ptr_id().hash(state);
    }
}

/// Typed shared handle over `PlatformRepository`.
#[derive(Debug, Clone)]
pub struct PlatformRepositoryHandle(std::rc::Rc<std::cell::RefCell<PlatformRepository>>);

impl PlatformRepositoryHandle {
    pub fn new(repository: PlatformRepository) -> Self {
        let rc: std::rc::Rc<std::cell::RefCell<PlatformRepository>> =
            std::rc::Rc::new(std::cell::RefCell::new(repository));
        let rc_dyn: std::rc::Rc<std::cell::RefCell<dyn RepositoryInterface>> = rc.clone();
        rc.borrow().set_self_handle(std::rc::Rc::downgrade(&rc_dyn));
        Self(rc)
    }

    pub fn from_rc(rc: std::rc::Rc<std::cell::RefCell<PlatformRepository>>) -> Self {
        Self(rc)
    }

    pub fn as_rc(&self) -> &std::rc::Rc<std::cell::RefCell<PlatformRepository>> {
        &self.0
    }

    pub fn borrow(&self) -> Ref<'_, PlatformRepository> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<'_, PlatformRepository> {
        self.0.borrow_mut()
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn ptr_id(&self) -> usize {
        std::rc::Rc::as_ptr(&self.0) as *const () as usize
    }
}

impl From<PlatformRepositoryHandle> for RepositoryInterfaceHandle {
    fn from(h: PlatformRepositoryHandle) -> Self {
        let rc: std::rc::Rc<std::cell::RefCell<dyn RepositoryInterface>> = h.0;
        RepositoryInterfaceHandle::from_rc(rc)
    }
}

impl PartialEq for PlatformRepositoryHandle {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for PlatformRepositoryHandle {}

impl std::hash::Hash for PlatformRepositoryHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ptr_id().hash(state);
    }
}
