//! `InstallerInterface` adapter for an installer entity living in the PHP child process.

use crate::installer::InstallerInterface;
use crate::package::PackageInterfaceHandle;
use crate::php_worker::call::PhpCall;
use crate::php_worker::job;
use crate::php_worker::rpc::entity::entity_value;
use crate::repository::InstalledRepositoryInterfaceHandle;
use bstr::BStr;
use indexmap::IndexMap;
use shirabe_php_rpc::{FromRpcValue, PhpEntityHandle, RpcValue, ToRpcValue, codec};
use shirabe_php_shim::PhpMixed;

/// `InstallerInterface` adapter for an installer entity living in the PHP child process: the
/// installer a plugin hands to `InstallationManager::addInstaller`, or the class a legacy
/// `composer-installer` package names. Every call is forwarded as a `CallMethod` RPC.
#[derive(Debug)]
pub struct PhpInstallerProxy {
    handle: PhpEntityHandle,
}

impl PhpInstallerProxy {
    fn new(handle: PhpEntityHandle) -> Self {
        Self { handle }
    }

    fn call<R: FromRpcValue>(&self, method: &BStr, args: Vec<RpcValue>) -> anyhow::Result<R> {
        PhpCall::Method(&self.handle, method).run(args)
    }

    fn package_arg(package: &PackageInterfaceHandle) -> RpcValue {
        package.clone().to_rpc_value()
    }

    fn optional_package_arg(package: &Option<PackageInterfaceHandle>) -> RpcValue {
        match package {
            Some(package) => Self::package_arg(package),
            None => RpcValue::Null,
        }
    }

    fn repo_arg(repo: &InstalledRepositoryInterfaceHandle) -> anyhow::Result<RpcValue> {
        Ok(entity_value(&repo.as_repository_handle())?)
    }

    /// The `?PromiseInterface` half of the installer contract. The Rust callers await the
    /// installer's effects rather than chaining continuations, so a returned promise is awaited
    /// here as the PHP-owned promise it crossed the boundary as.
    async fn promise_result(
        &self,
        method: &BStr,
        value: RpcValue,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let call = PhpCall::Method(&self.handle, method);
        let id = match value {
            RpcValue::Null => return Ok(None),
            RpcValue::PromiseHandle(id) => id,
            other => {
                return Err(call.unsupported_shape(codec::unexpected("a promise or null", &other)));
            }
        };
        let result = job::await_php_promise(id).await?;
        Option::<PhpMixed>::from_rpc_value(&result).map_err(|throw| call.unsupported_shape(throw))
    }
}

#[async_trait::async_trait(?Send)]
impl InstallerInterface for PhpInstallerProxy {
    fn supports(&self, package_type: &bstr::BStr) -> anyhow::Result<bool> {
        self.call(BStr::new("supports"), vec![RpcValue::string(package_type)])
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
            RpcValue::string(r#type),
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
            RpcValue::string(r#type),
            Self::package_arg(&package),
            Self::optional_package_arg(&prev_package),
        ];
        let value = self.call(BStr::new("cleanup"), args)?;
        self.promise_result(BStr::new("cleanup"), value).await
    }

    fn get_install_path(
        &self,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<Option<std::path::PathBuf>> {
        let args = vec![Self::package_arg(&package)];
        self.call(BStr::new("getInstallPath"), args)
    }
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
