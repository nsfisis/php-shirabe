//! ref: composer/src/Composer/Installer/PluginInstaller.php

use anyhow::Result;
use shirabe_php_shim::{empty, LogicException, PhpMixed, UnexpectedValueException};
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use crate::installer::binary_installer::BinaryInstaller;
use crate::installer::installer_interface::InstallerInterface;
use crate::installer::library_installer::LibraryInstaller;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::partial_composer::PartialComposer;
use crate::plugin::plugin_manager::PluginManager;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;

#[derive(Debug)]
pub struct PluginInstaller {
    inner: LibraryInstaller,
}

impl PluginInstaller {
    pub fn new(
        io: Box<dyn IOInterface>,
        composer: PartialComposer,
        fs: Option<Filesystem>,
        binary_installer: Option<BinaryInstaller>,
    ) -> Self {
        Self {
            inner: LibraryInstaller::new(io, composer, Some("composer-plugin".to_string()), fs, binary_installer),
        }
    }

    pub fn disable_plugins(&self) {
        // TODO(plugin): disable plugins via plugin manager
        self.get_plugin_manager().disable_plugins();
    }

    fn rollback_install(&self, e: anyhow::Error, repo: &mut dyn InstalledRepositoryInterface, package: &dyn PackageInterface) -> Result<()> {
        self.inner.io.write_error(&format!("Plugin initialization failed ({}), uninstalling plugin", e));
        self.inner.uninstall(repo, package)?;
        Err(e)
    }

    fn get_plugin_manager(&self) -> &PluginManager {
        // TODO(plugin): assert self.inner.composer is fully loaded Composer instance
        assert!(
            self.inner.composer.is_full_composer(),
            "{}",
            LogicException {
                message: "PluginInstaller should be initialized with a fully loaded Composer instance.".to_string(),
                code: 0,
            }
        );
        // TODO(plugin): return plugin manager from composer
        self.inner.composer.get_plugin_manager()
    }
}

impl InstallerInterface for PluginInstaller {
    fn supports(&self, package_type: &str) -> bool {
        package_type == "composer-plugin" || package_type == "composer-installer"
    }

    fn is_installed(&self, repo: &dyn InstalledRepositoryInterface, package: &dyn PackageInterface) -> bool {
        self.inner.is_installed(repo, package)
    }

    fn prepare(&self, r#type: &str, package: &dyn PackageInterface, prev_package: Option<&dyn PackageInterface>) -> Result<Option<Box<dyn PromiseInterface>>> {
        if (r#type == "install" || r#type == "update") && !self.get_plugin_manager().are_plugins_disabled("local") {
            let plugin_optional = package.get_extra()
                .get("plugin-optional")
                .map(|v| matches!(v, PhpMixed::Bool(true)))
                .unwrap_or(false);
            // TODO(plugin): check if plugin is allowed
            self.get_plugin_manager().is_plugin_allowed(package.get_name(), false, plugin_optional);
        }

        self.inner.prepare(r#type, package, prev_package)
    }

    fn download(&self, package: &dyn PackageInterface, prev_package: Option<&dyn PackageInterface>) -> Result<Option<Box<dyn PromiseInterface>>> {
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

    fn install(&self, repo: &mut dyn InstalledRepositoryInterface, package: &dyn PackageInterface) -> Result<Option<Box<dyn PromiseInterface>>> {
        let promise = self.inner.install(repo, package)?;
        let promise = match promise {
            Some(p) => p,
            None => shirabe_external_packages::react::promise::resolve(None),
        };

        // TODO(plugin): register package in plugin manager after install, rollback on failure
        Ok(Some(promise.then(Box::new(move || -> Result<()> {
            Platform::workaround_filesystem_issues();
            // self.get_plugin_manager().register_package(package, true)?;
            // On error: self.rollback_install(e, repo, package)?;
            Ok(())
        }))))
    }

    fn update(&self, repo: &mut dyn InstalledRepositoryInterface, initial: &dyn PackageInterface, target: &dyn PackageInterface) -> Result<Option<Box<dyn PromiseInterface>>> {
        let promise = self.inner.update(repo, initial, target)?;
        let promise = match promise {
            Some(p) => p,
            None => shirabe_external_packages::react::promise::resolve(None),
        };

        // TODO(plugin): deactivate initial and register target in plugin manager after update, rollback on failure
        Ok(Some(promise.then(Box::new(move || -> Result<()> {
            Platform::workaround_filesystem_issues();
            // self.get_plugin_manager().deactivate_package(initial);
            // self.get_plugin_manager().register_package(target, true)?;
            // On error: self.rollback_install(e, repo, target)?;
            Ok(())
        }))))
    }

    fn uninstall(&self, repo: &mut dyn InstalledRepositoryInterface, package: &dyn PackageInterface) -> Result<Option<Box<dyn PromiseInterface>>> {
        // TODO(plugin): uninstall package from plugin manager
        self.get_plugin_manager().uninstall_package(package);

        self.inner.uninstall(repo, package)
    }

    fn cleanup(&self, r#type: &str, package: &dyn PackageInterface, prev_package: Option<&dyn PackageInterface>) -> Result<Option<Box<dyn PromiseInterface>>> {
        self.inner.cleanup(r#type, package, prev_package)
    }

    fn get_install_path(&self, package: &dyn PackageInterface) -> Option<String> {
        self.inner.get_install_path(package)
    }
}
