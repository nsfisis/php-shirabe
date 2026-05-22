//! ref: composer/src/Composer/Installer/PluginInstaller.php

use crate::composer::PartialComposerWeakHandle;
use crate::installer::BinaryInstaller;
use crate::installer::InstallerInterface;
use crate::installer::LibraryInstaller;
use crate::io::IOInterface;
use crate::package::PackageInterface;
use crate::plugin::PluginManager;
use crate::repository::InstalledRepositoryInterface;
use crate::util::Filesystem;
use crate::util::Platform;
use anyhow::Result;
use shirabe_php_shim::{LogicException, PhpMixed, UnexpectedValueException, empty};

#[derive(Debug)]
pub struct PluginInstaller {
    inner: LibraryInstaller,
}

impl PluginInstaller {
    pub fn new(
        io: Box<dyn IOInterface>,
        composer: PartialComposerWeakHandle,
        fs: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
        binary_installer: Option<BinaryInstaller>,
    ) -> Self {
        Self {
            inner: LibraryInstaller::new(
                io,
                composer,
                Some("composer-plugin".to_string()),
                fs,
                binary_installer,
            ),
        }
    }

    pub fn disable_plugins(&mut self) {
        // TODO(plugin): disable plugins via plugin manager
        self.get_plugin_manager().borrow_mut().disable_plugins();
    }

    fn rollback_install(
        &mut self,
        e: anyhow::Error,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> Result<()> {
        self.inner.io.write_error(&format!(
            "Plugin initialization failed ({}), uninstalling plugin",
            e
        ));
        self.inner.uninstall(repo, package)?;
        Err(e)
    }

    fn get_plugin_manager(&self) -> std::rc::Rc<std::cell::RefCell<PluginManager>> {
        // TODO(plugin): PartialComposer does not expose PluginManager; revisit when wiring plugin support
        todo!("PartialComposer.get_plugin_manager")
    }
}

impl InstallerInterface for PluginInstaller {
    fn supports(&self, package_type: &str) -> bool {
        package_type == "composer-plugin" || package_type == "composer-installer"
    }

    fn is_installed(
        &self,
        repo: &dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> bool {
        self.inner.is_installed(repo, package)
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        if (r#type == "install" || r#type == "update")
            && !self
                .get_plugin_manager()
                .borrow()
                .are_plugins_disabled("local")
        {
            let plugin_optional = package
                .get_extra()
                .get("plugin-optional")
                .map(|v| matches!(v, PhpMixed::Bool(true)))
                .unwrap_or(false);
            // TODO(plugin): check if plugin is allowed
            // TODO(phase-b): is_plugin_allowed needs &mut PluginManager but prepare is &self.
            let _ = plugin_optional;
        }

        self.inner.prepare(r#type, package, prev_package)
    }

    async fn download(
        &self,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        let extra = package.get_extra();
        let class = extra.get("class").cloned().unwrap_or(PhpMixed::Null);
        if empty(&class) {
            return Err(UnexpectedValueException {
                message: format!(
                    "Error while installing {}, composer-plugin packages should have a class defined in their extra key to be usable.",
                    package.get_pretty_name()
                ),
                code: 0,
            }.into());
        }

        self.inner.download(package, prev_package)
    }

    async fn install(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> Result<Option<PhpMixed>> {
        let promise = self.inner.install(repo, package)?;
        let promise = match promise {
            Some(p) => p,
            None => shirabe_external_packages::react::promise::resolve(None),
        };

        // TODO(plugin): register package in plugin manager after install, rollback on failure
        Ok(Some(promise.then(
            Some(Box::new(move |_v| -> Option<PhpMixed> {
                Platform::workaround_filesystem_issues();
                // self.get_plugin_manager().register_package(package, true)?;
                // On error: self.rollback_install(e, repo, package)?;
                None
            })),
            None,
        )))
    }

    async fn update(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
    ) -> Result<Option<PhpMixed>> {
        let promise = self.inner.update(repo, initial, target)?;
        let promise = match promise {
            Some(p) => p,
            None => shirabe_external_packages::react::promise::resolve(None),
        };

        // TODO(plugin): deactivate initial and register target in plugin manager after update, rollback on failure
        Ok(Some(promise.then(
            Some(Box::new(move |_v| -> Option<PhpMixed> {
                Platform::workaround_filesystem_issues();
                // self.get_plugin_manager().deactivate_package(initial);
                // self.get_plugin_manager().register_package(target, true)?;
                // On error: self.rollback_install(e, repo, target)?;
                None
            })),
            None,
        )))
    }

    async fn uninstall(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        package: &dyn PackageInterface,
    ) -> Result<Option<PhpMixed>> {
        // TODO(plugin): uninstall package from plugin manager
        self.get_plugin_manager()
            .borrow_mut()
            .uninstall_package(package);

        self.inner.uninstall(repo, package)
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        self.inner.cleanup(r#type, package, prev_package)
    }

    fn get_install_path(&self, package: &dyn PackageInterface) -> Option<String> {
        self.inner.get_install_path(package)
    }
}
