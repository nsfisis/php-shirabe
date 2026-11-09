//! `InstallerInterface` adapter for an installer entity living in the PHP child process.

use crate::installer::InstallerInterface;
use crate::package::PackageInterfaceHandle;
use crate::php_worker::call::PhpCall;
use crate::php_worker::job;
use crate::php_worker::rpc::table::{package_handle_value, repository_handle_value};
use crate::repository::InstalledRepositoryInterfaceHandle;
use bstr::BStr;
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpObjHandle, PluginValue, wire};
use shirabe_php_shim::PhpMixed;

/// `InstallerInterface` adapter for an installer entity living in the PHP child process: the
/// installer a plugin hands to `InstallationManager::addInstaller`, or the class a legacy
/// `composer-installer` package names. Every call is forwarded as a `CallPhpMethod` RPC.
#[derive(Debug)]
pub struct PhpInstallerProxy {
    handle: PhpObjHandle,
}

impl PhpInstallerProxy {
    fn new(handle: PhpObjHandle) -> Self {
        Self { handle }
    }

    fn call<R: FromWire>(&self, method: &BStr, args: Vec<PluginValue>) -> anyhow::Result<R> {
        PhpCall::Method(&self.handle, method).run(args)
    }

    fn package_arg(package: &PackageInterfaceHandle) -> PluginValue {
        package_handle_value(package.as_rc())
    }

    fn optional_package_arg(package: &Option<PackageInterfaceHandle>) -> PluginValue {
        match package {
            Some(package) => Self::package_arg(package),
            None => PluginValue::Null,
        }
    }

    fn repo_arg(repo: &InstalledRepositoryInterfaceHandle) -> anyhow::Result<PluginValue> {
        Ok(repository_handle_value(&repo.as_repository_handle())?)
    }

    /// The `?PromiseInterface` half of the installer contract. The Rust callers await the
    /// installer's effects rather than chaining continuations, so a returned promise is awaited
    /// here as the PHP-owned job it crossed the boundary as.
    async fn promise_result(
        &self,
        method: &BStr,
        value: PluginValue,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let call = PhpCall::Method(&self.handle, method);
        let id = match value {
            PluginValue::Null => return Ok(None),
            PluginValue::Job(id) => id,
            other => {
                return Err(call.unsupported_shape(wire::unexpected("a promise or null", &other)));
            }
        };
        let result = job::await_php_job(id).await?;
        Option::<PhpMixed>::from_wire(&result).map_err(|throw| call.unsupported_shape(throw))
    }
}

#[async_trait::async_trait(?Send)]
impl InstallerInterface for PhpInstallerProxy {
    fn supports(&self, package_type: &bstr::BStr) -> anyhow::Result<bool> {
        self.call(
            BStr::new("supports"),
            vec![PluginValue::string(package_type)],
        )
    }

    fn is_installed(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)];
        self.call(BStr::new("isInstalled"), args)
    }

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            Self::package_arg(&package),
            Self::optional_package_arg(&prev_package),
        ];
        let value = self.call(BStr::new("download"), args)?;
        self.promise_result(BStr::new("download"), value).await
    }

    async fn prepare(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            PluginValue::string(r#type),
            Self::package_arg(&package),
            Self::optional_package_arg(&prev_package),
        ];
        let value = self.call(BStr::new("prepare"), args)?;
        self.promise_result(BStr::new("prepare"), value).await
    }

    async fn install(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)];
        let value = self.call(BStr::new("install"), args)?;
        self.promise_result(BStr::new("install"), value).await
    }

    async fn update(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            Self::repo_arg(repo)?,
            Self::package_arg(&initial),
            Self::package_arg(&target),
        ];
        let value = self.call(BStr::new("update"), args)?;
        self.promise_result(BStr::new("update"), value).await
    }

    async fn uninstall(
        &self,
        repo: &InstalledRepositoryInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![Self::repo_arg(repo)?, Self::package_arg(&package)];
        let value = self.call(BStr::new("uninstall"), args)?;
        self.promise_result(BStr::new("uninstall"), value).await
    }

    async fn cleanup(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let args = vec![
            PluginValue::string(r#type),
            Self::package_arg(&package),
            Self::optional_package_arg(&prev_package),
        ];
        let value = self.call(BStr::new("cleanup"), args)?;
        self.promise_result(BStr::new("cleanup"), value).await
    }

    fn get_install_path(&self, package: PackageInterfaceHandle) -> Option<std::path::PathBuf> {
        // PHP declares `getInstallPath(): string`; a failure here is a plugin error the
        // infallible signature cannot carry, so it aborts rather than answering a path that
        // would silently install the package in the wrong place.
        let args = vec![Self::package_arg(&package)];
        self.call(BStr::new("getInstallPath"), args)
            .unwrap_or_else(|error| {
                panic!(
                    "{}::getInstallPath failed over RPC: {error:#}",
                    self.handle.class().display_lossy()
                )
            })
    }
}

thread_local! {
    /// The installer adapters handed to `InstallationManager::addInstaller` over RPC, keyed by
    /// the entity. `removeInstaller` arrives carrying the same entity, and the manager compares
    /// installers by identity, so the adapter it was given has to be found again rather than
    /// rebuilt.
    static PHP_INSTALLER_PROXIES: std::cell::RefCell<IndexMap<PhpObjHandle, std::rc::Rc<dyn InstallerInterface>>> =
        std::cell::RefCell::new(IndexMap::new());
}

/// The adapter for an installer entity, building it on first sight.
pub(crate) fn php_installer_proxy(handle: PhpObjHandle) -> std::rc::Rc<dyn InstallerInterface> {
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
pub(crate) fn forget_php_installer_proxy(handle: &PhpObjHandle) {
    PHP_INSTALLER_PROXIES.with(|proxies| {
        proxies.borrow_mut().shift_remove(handle);
    });
}
