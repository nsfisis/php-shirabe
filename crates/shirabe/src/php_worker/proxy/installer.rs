//! `InstallerInterface` adapter for an installer entity living in the PHP child process.

use crate::installer::InstallerInterface;
use crate::package::PackageInterfaceHandle;
use crate::php_worker::call::PhpCall;
use crate::php_worker::proxy::PhpProxy;
use crate::php_worker::rpc::entity::entity_value;
use crate::php_worker::throwable::throw_into_error;
use crate::repository::InstalledRepositoryInterfaceHandle;
use bstr::BStr;
use indexmap::IndexMap;
use shirabe_php_rpc::{PhpEntityHandle, RpcValue, ToRpcValue};

/// `InstallerInterface` adapter for an installer entity living in the PHP child process: the
/// installer a plugin hands to `InstallationManager::addInstaller`, or the class a legacy
/// `composer-installer` package names. Every call is forwarded as a `CallMethod` RPC, by the
/// `InstallerInterface` impl `#[php_binding]` generates.
#[derive(Debug)]
pub struct PhpInstallerProxy {
    handle: PhpEntityHandle,
}

impl PhpInstallerProxy {
    fn new(handle: PhpEntityHandle) -> Self {
        Self { handle }
    }
}

impl PhpProxy for PhpInstallerProxy {
    fn php_handle(&self) -> &PhpEntityHandle {
        &self.handle
    }
}

// `supports()` and `isInstalled()` declare no return type in PHP, so they may answer with any
// value; only a bool decodes.
pub(crate) fn installer_supports(
    proxy: &PhpInstallerProxy,
    package_type: &BStr,
) -> anyhow::Result<bool> {
    PhpCall::Method(&proxy.handle, BStr::new("supports")).run(vec![RpcValue::string(package_type)])
}

pub(crate) fn installer_is_installed(
    proxy: &PhpInstallerProxy,
    repo: &InstalledRepositoryInterfaceHandle,
    package: PackageInterfaceHandle,
) -> anyhow::Result<bool> {
    let repo = entity_value(&repo.as_repository_handle()).map_err(throw_into_error)?;
    PhpCall::Method(&proxy.handle, BStr::new("isInstalled")).run(vec![repo, package.to_rpc_value()])
}

thread_local! {
    /// The installer adapters handed to `InstallationManager::addInstaller` over RPC, keyed by
    /// the entity. `removeInstaller` arrives carrying the same entity, and the manager compares
    /// installers by identity, so the adapter it was given has to be found again rather than
    /// rebuilt.
    static PHP_INSTALLER_PROXIES: std::cell::RefCell<IndexMap<PhpEntityHandle, std::rc::Rc<dyn InstallerInterface>>> =
        std::cell::RefCell::new(IndexMap::new());
}

/// The adapter for an installer entity, building it on first sight.
pub(crate) fn php_installer_proxy(handle: PhpEntityHandle) -> std::rc::Rc<dyn InstallerInterface> {
    PHP_INSTALLER_PROXIES.with(|proxies| {
        let mut proxies = proxies.borrow_mut();
        if let Some(existing) = proxies.get(&handle) {
            return existing.clone();
        }
        let proxy: std::rc::Rc<dyn InstallerInterface> =
            std::rc::Rc::new(PhpInstallerProxy::new(handle.clone()));
        proxies.insert(handle, proxy.clone());
        proxy
    })
}

/// Drops the adapter bookkeeping for an installer entity that left the manager.
pub(crate) fn forget_php_installer_proxy(handle: &PhpEntityHandle) {
    PHP_INSTALLER_PROXIES.with(|proxies| {
        proxies.borrow_mut().shift_remove(handle);
    });
}
