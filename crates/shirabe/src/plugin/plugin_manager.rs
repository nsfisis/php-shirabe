//! ref: composer/src/Composer/Plugin/PluginManager.php
//!
//! TODO(plugin): the entire plugin manager subsystem is part of the Plugin API
//! and is not implemented in Phase A. The structure is mirrored verbatim so
//! future plugin support can fill in the runtime hooks.

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    E_USER_DEPRECATED, PhpMixed, RuntimeException, UnexpectedValueException, array_key_exists,
    array_reverse, array_search, clone, get_class, get_class_obj, implode, in_array, is_a,
    is_array, is_string, ksort, preg_quote, str_replace, strrpos, strtr, substr, trigger_error,
    trim, var_export, var_export_str, version_compare,
};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;

use crate::composer::PartialComposerHandle;
use crate::composer::{ComposerHandle, ComposerWeakHandle};
use crate::event_dispatcher::EventSubscriberInterface;
use crate::factory::DisablePlugins;
use crate::installer::InstallerInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::CompletePackage;
use crate::package::Link;
use crate::package::Locker;
use crate::package::PackageInterfaceHandle;
use crate::package::RootPackageInterfaceHandle;
use crate::package::base_package::{self, BasePackage};
use crate::package::version::VersionParser;
use crate::plugin::Capable;
use crate::plugin::PluginBlockedException;
use crate::plugin::capability::Capability;
use crate::plugin::plugin_interface::{self, PluginInterface};
use crate::repository::InstalledRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryUtils;
use crate::repository::RootPackageRepository;
use crate::util::PackageSorter;

#[derive(Debug)]
pub struct PluginManager {
    pub(crate) composer: ComposerWeakHandle,
    pub(crate) io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    pub(crate) global_composer: Option<PartialComposerHandle>,
    pub(crate) version_parser: VersionParser,
    pub(crate) disable_plugins: DisablePlugins,
    pub(crate) plugins: Vec<Box<dyn PluginInterface>>,
    pub(crate) registered_plugins: IndexMap<String, Vec<PluginOrInstaller>>,
    allow_plugin_rules: Option<IndexMap<String, bool>>,
    allow_global_plugin_rules: Option<IndexMap<String, bool>>,
    running_in_global_dir: bool,
}

#[derive(Debug)]
pub enum PluginOrInstaller {
    Plugin(Box<dyn PluginInterface>),
    Installer(Box<dyn InstallerInterface>),
}

static mut CLASS_COUNTER: i64 = 0;

impl PluginManager {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        composer: ComposerWeakHandle,
        global_composer: Option<PartialComposerHandle>,
        disable_plugins: DisablePlugins,
    ) -> Self {
        let composer_rc = composer
            .upgrade()
            .expect("PluginManager must not outlive Composer");
        let allow_plugins_config = composer_rc
            .borrow()
            .get_config()
            .borrow()
            .get("allow-plugins")
            .clone();
        let locker = composer_rc.borrow().get_locker().clone();
        let mut locker = locker.borrow_mut();
        let allow_plugin_rules =
            Self::parse_allowed_plugins(allow_plugins_config, Some(&mut *locker));
        drop(locker);
        let allow_global_plugin_rules = Self::parse_allowed_plugins(
            global_composer
                .as_ref()
                .map(|gc| {
                    gc.borrow_partial()
                        .get_config()
                        .borrow_mut()
                        .get("allow-plugins")
                        .clone()
                })
                .unwrap_or(PhpMixed::Bool(false)),
            None,
        );
        Self {
            io,
            composer,
            global_composer,
            version_parser: VersionParser::new(),
            disable_plugins,
            plugins: vec![],
            registered_plugins: IndexMap::new(),
            allow_plugin_rules,
            allow_global_plugin_rules,
            running_in_global_dir: false,
        }
    }

    pub fn set_running_in_global_dir(&mut self, running_in_global_dir: bool) {
        self.running_in_global_dir = running_in_global_dir;
    }

    /// Upgrades the weak Composer back-reference to a full handle. PHP holds a strong
    /// `Composer`; the Rust port keeps it weak to break the Composer/PluginManager cycle.
    fn composer_full(&self) -> ComposerHandle {
        self.composer
            .upgrade()
            .expect("PluginManager must not outlive Composer")
    }

    /// Loads all plugins from currently installed plugin packages
    pub fn load_installed_plugins(&mut self) -> anyhow::Result<()> {
        // TODO(plugin): plugin loading is part of the plugin API
        if !self.are_plugins_disabled("local") {
            let repo = self
                .composer_full()
                .borrow()
                .get_repository_manager()
                .borrow()
                .get_local_repository();
            self.load_repository(
                &mut *repo.borrow_mut(),
                false,
                Some(self.composer_full().borrow().get_package().clone()),
            )?;
        }

        if self.global_composer.is_some() && !self.are_plugins_disabled("global") {
            let repo = self
                .global_composer
                .as_ref()
                .unwrap()
                .borrow_partial()
                .get_repository_manager()
                .borrow()
                .get_local_repository();
            self.load_repository(&mut *repo.borrow_mut(), true, None)?;
        }
        Ok(())
    }

    /// Deactivate all plugins from currently installed plugin packages
    pub fn deactivate_installed_plugins(&mut self) -> anyhow::Result<()> {
        // TODO(plugin): deactivation is part of the plugin API
        if !self.are_plugins_disabled("local") {
            let repo = self
                .composer_full()
                .borrow()
                .get_repository_manager()
                .borrow()
                .get_local_repository();
            self.deactivate_repository(&mut *repo.borrow_mut(), false)?;
        }

        if self.global_composer.is_some() && !self.are_plugins_disabled("global") {
            let repo = self
                .global_composer
                .as_ref()
                .unwrap()
                .borrow_partial()
                .get_repository_manager()
                .borrow()
                .get_local_repository();
            self.deactivate_repository(&mut *repo.borrow_mut(), true)?;
        }

        Ok(())
    }

    /// Gets all currently active plugin instances
    pub fn get_plugins(&self) -> Vec<Box<dyn PluginInterface>> {
        self.plugins.iter().map(|p| p.clone_box()).collect()
    }

    /// Gets all currently active plugin instances
    ///
    /// internal — Plugin package names which are currently active
    pub fn get_registered_plugins(&self) -> Vec<String> {
        self.registered_plugins.keys().cloned().collect()
    }

    /// Gets global composer or null when main composer is not fully loaded
    pub fn get_global_composer(&self) -> Option<&PartialComposerHandle> {
        self.global_composer.as_ref()
    }

    /// Register a plugin package, activate it etc.
    pub fn register_package(
        &mut self,
        package: PackageInterfaceHandle,
        fail_on_missing_classes: bool,
        is_global_plugin: bool,
    ) -> anyhow::Result<()> {
        // TODO(plugin): registerPackage drives the actual plugin loading via eval()
        if self.are_plugins_disabled(if is_global_plugin { "global" } else { "local" }) {
            self.io.write_error(&format!(
                "<warning>The \"{}\" plugin was not loaded as plugins are disabled.</warning>",
                package.get_name()
            ));

            return Ok(());
        }

        if package.get_type() == "composer-plugin" {
            let requires_map = package.get_requires();
            let mut requires_composer: Option<&shirabe_semver::constraint::AnyConstraint> = None;
            for (_k, link) in &requires_map {
                if "composer-plugin-api" == link.get_target() {
                    requires_composer = Some(link.get_constraint());
                    break;
                }
            }

            let requires_composer = match requires_composer {
                Some(r) => r,
                None => {
                    return Err(RuntimeException {
                        message: format!("Plugin {} is missing a require statement for a version of the composer-plugin-api package.", package.get_name()),
                        code: 0,
                    }.into());
                }
            };

            let current_plugin_api_version = self.get_plugin_api_version();
            let current_plugin_api_constraint = SimpleConstraint::new(
                "==".to_string(),
                self.version_parser
                    .normalize(&current_plugin_api_version, None)?
                    .to_string(),
                None,
            );

            if requires_composer.get_pretty_string() == self.get_plugin_api_version() {
                self.io.write_error(&format!("<warning>The \"{}\" plugin requires composer-plugin-api {}, this *WILL* break in the future and it should be fixed ASAP (require ^{} instead for example).</warning>", package.get_name(), self.get_plugin_api_version(), self.get_plugin_api_version()));
            } else if !requires_composer.matches(&current_plugin_api_constraint.into()) {
                self.io.write_error(&format!("<warning>The \"{}\" plugin {}was skipped because it requires a Plugin API version (\"{}\") that does not match your Composer installation (\"{}\"). You may need to run composer update with the \"--no-plugins\" option.</warning>",
                    package.get_name(),
                    if is_global_plugin || self.running_in_global_dir { "(installed globally) " } else { "" },
                    requires_composer.get_pretty_string(),
                    current_plugin_api_version
                ));
                return Ok(());
            }

            if package.get_name() == "symfony/flex"
                && Preg::is_match3("{^[0-9.]+$}", &package.get_version(), None).unwrap_or(false)
                && version_compare(&package.get_version(), "1.9.8", "<")
            {
                self.io.write_error(&format!("<warning>The \"{}\" plugin {}was skipped because it is not compatible with Composer 2+. Make sure to update it to version 1.9.8 or greater.</warning>",
                    package.get_name(),
                    if is_global_plugin || self.running_in_global_dir { "(installed globally) " } else { "" }
                ));
                return Ok(());
            }
        }

        let plugin_optional = package
            .get_extra()
            .get("plugin-optional")
            .map(|v| v.as_bool() == Some(true))
            .unwrap_or(false);
        if !self.is_plugin_allowed(&package.get_name(), is_global_plugin, plugin_optional, true)? {
            self.io.write_error(&format!(
                "Skipped loading \"{}\" {}as it is not in config.allow-plugins",
                package.get_name(),
                if is_global_plugin || self.running_in_global_dir {
                    "(installed globally) "
                } else {
                    ""
                }
            ));
            return Ok(());
        }

        // TODO(plugin): the rest of registerPackage performs class-level eval() to load the plugin source.
        // This is a runtime concern that requires PHP semantics; not portable to Rust without a PHP interpreter.
        // The remainder of the function is mirrored as references but performs no actual loading.
        let _old_installer_plugin = package.get_type() == "composer-installer";

        if self.registered_plugins.contains_key(&package.get_name()) {
            return Ok(());
        }

        let extra = package.get_extra();
        let class_value = extra.get("class");
        // PHP: empty($extra['class']) — true for null, false, 0, "", "0", [], or missing key.
        let class_is_empty = match class_value {
            None => true,
            Some(PhpMixed::Null) => true,
            Some(PhpMixed::Bool(false)) => true,
            Some(PhpMixed::Int(0)) => true,
            Some(PhpMixed::String(s)) if s.is_empty() || s == "0" => true,
            Some(PhpMixed::Array(a)) => a.is_empty(),
            Some(PhpMixed::List(l)) => l.is_empty(),
            _ => false,
        };
        if class_is_empty {
            return Err(UnexpectedValueException {
                message: format!("Error while installing {}, composer-plugin packages should have a class defined in their extra key to be usable.", package.get_pretty_name()),
                code: 0,
            }.into());
        }
        let _classes: Vec<String> = if let Some(arr) = class_value.and_then(|v| v.as_list()) {
            arr.iter()
                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                .collect()
        } else {
            vec![
                class_value
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string(),
            ]
        };

        // TODO(plugin): everything below this point in the original PHP would create runtime instances:
        //   - clone root package and clear `files` autoloads
        //   - build a synthetic InstalledRepository for plugin dependencies
        //   - parseAutoloads / createLoader on the autoload generator
        //   - eval the plugin class source under a temporary class name
        //   - call activate(...) and register subscribers
        // None of that is implementable without a PHP runtime, and so the body is intentionally left as a no-op stub.
        let _ = fail_on_missing_classes;
        Ok(())
    }

    /// Deactivates a plugin package
    pub fn deactivate_package(&mut self, package: PackageInterfaceHandle) {
        // TODO(plugin): deactivation flow
        if !self.registered_plugins.contains_key(&package.get_name()) {
            return;
        }

        let plugins = self
            .registered_plugins
            .shift_remove(&package.get_name())
            .unwrap_or_default();
        for plugin in plugins {
            match plugin {
                PluginOrInstaller::Installer(inst) => {
                    self.composer_full()
                        .borrow()
                        .get_installation_manager()
                        .borrow_mut()
                        .remove_installer(&*inst);
                }
                PluginOrInstaller::Plugin(p) => {
                    self.remove_plugin(&*p);
                }
            }
        }
    }

    /// Uninstall a plugin package
    pub fn uninstall_package(&mut self, package: PackageInterfaceHandle) {
        // TODO(plugin): uninstall flow
        if !self.registered_plugins.contains_key(&package.get_name()) {
            return;
        }

        let plugins = self
            .registered_plugins
            .shift_remove(&package.get_name())
            .unwrap_or_default();
        for plugin in plugins {
            match plugin {
                PluginOrInstaller::Installer(inst) => {
                    self.composer_full()
                        .borrow()
                        .get_installation_manager()
                        .borrow_mut()
                        .remove_installer(&*inst);
                }
                PluginOrInstaller::Plugin(mut p) => {
                    self.remove_plugin(&*p);
                    self.uninstall_plugin(&mut *p);
                }
            }
        }
    }

    /// Returns the version of the internal composer-plugin-api package.
    pub(crate) fn get_plugin_api_version(&self) -> String {
        plugin_interface::PLUGIN_API_VERSION.to_string()
    }

    /// Adds a plugin, activates it and registers it with the event dispatcher
    pub fn add_plugin(
        &mut self,
        mut plugin: Box<dyn PluginInterface>,
        is_global_plugin: bool,
        source_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        // TODO(plugin): plugin activation
        if self.are_plugins_disabled(if is_global_plugin { "global" } else { "local" }) {
            return Ok(());
        }

        if source_package.is_none() {
            trigger_error(
                "Calling PluginManager::addPlugin without $sourcePackage is deprecated, if you are using this please get in touch with us to explain the use case",
                E_USER_DEPRECATED,
            );
        } else {
            let sp = source_package.as_ref().unwrap();
            let plugin_optional = sp
                .get_extra()
                .get("plugin-optional")
                .map(|v| v.as_bool() == Some(true))
                .unwrap_or(false);
            if !self.is_plugin_allowed(&sp.get_name(), is_global_plugin, plugin_optional, true)? {
                self.io.write_error(&format!(
                    "Skipped loading \"{} from {}\" {} as it is not in config.allow-plugins",
                    get_class_obj(&*plugin),
                    sp.get_name(),
                    if is_global_plugin || self.running_in_global_dir {
                        "(installed globally) "
                    } else {
                        ""
                    }
                ));
                return Ok(());
            }
        }

        let mut details: Vec<String> = vec![];
        if let Some(sp) = source_package.as_ref() {
            details.push(format!("from {}", sp.get_name()));
        }
        if is_global_plugin || self.running_in_global_dir {
            details.push("installed globally".to_string());
        }
        self.io.write_error(&format!(
            "Loading plugin {}{}",
            get_class_obj(&*plugin),
            if !details.is_empty() {
                format!(" ({})", implode(", ", &details))
            } else {
                String::new()
            }
        ));
        plugin.activate(&self.composer_full(), self.io.clone());

        // TODO(plugin): if plugin is EventSubscriberInterface, hook into the event dispatcher
        // The PHP code calls $this->composer->getEventDispatcher()->addSubscriber($plugin);
        // — add_subscriber here is generic over `S: EventSubscriberInterface` and cannot
        // accept a `&dyn EventSubscriberInterface`. Skipped until subscriber dispatch is
        // implemented dynamically.
        let _ = (&*plugin).is_event_subscriber_interface();
        self.plugins.push(plugin);
        Ok(())
    }

    /// Removes a plugin, deactivates it and removes any listener the plugin has set on the plugin instance
    pub fn remove_plugin(&mut self, plugin: &dyn PluginInterface) {
        // TODO(plugin): plugin removal — PHP uses identity (`===`) comparison via array_search($plugin, $this->plugins, true).
        let plugin_addr = plugin as *const dyn PluginInterface as *const () as usize;
        let index = self.plugins.iter().position(|p| {
            (p.as_ref() as *const dyn PluginInterface as *const () as usize) == plugin_addr
        });
        let index = match index {
            Some(i) => i,
            None => return,
        };

        self.io
            .write_error(&format!("Unloading plugin {}", get_class_obj(plugin)));
        let mut removed = self.plugins.remove(index);
        removed.deactivate(&self.composer_full(), self.io.clone());

        // TODO(plugin): remove_listener accepts any callable/object in PHP; here we have
        // a plugin instance and need to translate to a Callable, which is not portable
        // without runtime reflection.
        let _ = plugin;
    }

    /// Notifies a plugin it is being uninstalled and should clean up
    pub fn uninstall_plugin(&self, plugin: &mut dyn PluginInterface) {
        // TODO(plugin): plugin uninstall hook
        self.io
            .write_error(&format!("Uninstalling plugin {}", get_class_obj(plugin)));
        plugin.uninstall(&self.composer_full(), self.io.clone());
    }

    fn load_repository(
        &mut self,
        repo: &mut dyn RepositoryInterface,
        is_global_repo: bool,
        root_package: Option<RootPackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        // TODO(plugin): repository scan for plugin packages
        let packages = repo.get_packages()?;

        let mut weights: IndexMap<String, i64> = IndexMap::new();
        for package in &packages {
            if package.get_type() == "composer-plugin" {
                let extra = package.get_extra();
                if package.get_name() == "composer/installers"
                    || extra
                        .get("plugin-modifies-install-path")
                        .map(|v| v.as_bool() == Some(true))
                        .unwrap_or(false)
                {
                    weights.insert(package.get_name().to_string(), -10000);
                }
            }
        }

        let sorted_packages = PackageSorter::sort_packages(
            packages.iter().map(|p| p.clone().into()).collect(),
            weights,
        );
        let required_packages: Vec<crate::package::BasePackageHandle> = if !is_global_repo {
            // PHP: $requiredPackages = RepositoryUtils::filterRequiredPackages($packages, $rootPackage, true);
            let bucket: Vec<crate::package::BasePackageHandle> = vec![];
            RepositoryUtils::filter_required_packages(
                packages.as_slice(),
                root_package.unwrap().into(),
                true,
                bucket,
            )
        } else {
            vec![]
        };

        for package in &sorted_packages {
            let cp = match package.as_complete_package() {
                Some(cp) => cp,
                None => continue,
            };

            let pkg_type = package.get_type();
            if pkg_type != "composer-plugin" && pkg_type != "composer-installer" {
                continue;
            }

            // PHP: !in_array($package, $requiredPackages, true) — identity-based comparison.
            // Both `sorted_packages` and `required_packages` are package handles, so compare
            // by shared-Rc pointer identity.
            let package_addr = package.ptr_id();
            let in_required = required_packages
                .iter()
                .any(|rp| rp.ptr_id() == package_addr);
            if !is_global_repo
                && !in_required
                && !self.is_plugin_allowed(&package.get_name(), false, true, false)?
            {
                self.io.write_error(&format!("<warning>The \"{}\" plugin was not loaded as it is not listed in allow-plugins and is not required by the root package anymore.</warning>", package.get_name()));
                continue;
            }

            if "composer-plugin" == package.get_type() {
                self.register_package(package.clone(), false, is_global_repo)?;
            // Backward compatibility
            } else if "composer-installer" == package.get_type() {
                self.register_package(package.clone(), false, is_global_repo)?;
            }
            let _ = cp;
        }
        Ok(())
    }

    fn deactivate_repository(
        &mut self,
        repo: &mut dyn RepositoryInterface,
        _is_global_repo: bool,
    ) -> anyhow::Result<()> {
        // TODO(plugin): deactivate plugins from a repository
        let packages = repo.get_packages()?;
        // PHP: $sortedPackages = array_reverse(PackageSorter::sortPackages($packages));
        let mut sorted_packages = PackageSorter::sort_packages(
            packages.iter().map(|p| p.clone().into()).collect(),
            IndexMap::new(),
        );
        sorted_packages.reverse();

        for package in &sorted_packages {
            if package.as_complete_package().is_none() {
                continue;
            }
            if "composer-plugin" == package.get_type() {
                self.deactivate_package(package.clone());
            // Backward compatibility
            } else if "composer-installer" == package.get_type() {
                self.deactivate_package(package.clone());
            }
        }

        Ok(())
    }

    fn collect_dependencies(
        &self,
        installed_repo: &InstalledRepository,
        mut collected: IndexMap<String, PackageInterfaceHandle>,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<IndexMap<String, PackageInterfaceHandle>> {
        // TODO(plugin): used by registerPackage to assemble plugin dependency autoload map
        for (_k, require_link) in &package.get_requires() {
            for required_package in installed_repo
                .find_packages_with_replacers_and_providers(require_link.get_target(), None)?
            {
                if !collected.contains_key(&required_package.get_name()) {
                    collected.insert(required_package.get_name(), required_package.clone().into());
                    collected = self.collect_dependencies(
                        installed_repo,
                        collected,
                        required_package.clone().into(),
                    )?;
                }
            }
        }

        Ok(collected)
    }

    /// Retrieves the path a package is installed to.
    fn get_install_path(
        &mut self,
        package: PackageInterfaceHandle,
        global: bool,
    ) -> Option<String> {
        if !global {
            return self
                .composer_full()
                .borrow()
                .get_installation_manager()
                .borrow_mut()
                .get_install_path(package);
        }

        // PHP: assert(null !== $this->globalComposer);
        self.global_composer
            .as_ref()
            .unwrap()
            .borrow_partial()
            .get_installation_manager()
            .borrow_mut()
            .get_install_path(package)
    }

    pub(crate) fn get_capability_implementation_class_name(
        &self,
        plugin: &dyn PluginInterface,
        capability: &str,
    ) -> anyhow::Result<Option<String>> {
        // TODO(plugin): capability lookup
        let capable = match plugin.as_capable() {
            Some(c) => c,
            None => return Ok(None),
        };

        let capabilities = capable.get_capabilities();

        // PHP: !empty($capabilities[$capability]) && is_string($capabilities[$capability]) && trim($capabilities[$capability])
        if let Some(s) = capabilities.get(capability) {
            let trimmed = trim(s, Some(" \t\n\r\0\u{0B}"));
            if !s.is_empty() && s != "0" && !trimmed.is_empty() {
                return Ok(Some(trimmed));
            }
        }

        // PHP: empty($capabilities[$capability]) — true for null, false, 0, "", "0", [], or missing key.
        // In Rust the values are typed as String, so we only need to consider "", "0".
        let cap_is_empty = match capabilities.get(capability) {
            None => true,
            Some(s) if s.is_empty() || s == "0" => true,
            _ => false,
        };
        if array_key_exists(capability, &capabilities)
            && (cap_is_empty
                || trim(
                    capabilities
                        .get(capability)
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                    Some(" \t\n\r\0\u{0B}"),
                )
                .is_empty())
        {
            return Err(UnexpectedValueException {
                message: format!(
                    "Plugin {} provided invalid capability class name(s), got {}",
                    get_class_obj(plugin),
                    var_export_str(capabilities.get(capability).unwrap(), true)
                ),
                code: 0,
            }
            .into());
        }

        Ok(None)
    }

    pub fn get_plugin_capability(
        &self,
        plugin: &dyn PluginInterface,
        capability_class_name: &str,
        _ctor_args: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Option<Box<dyn Capability>>> {
        // TODO(plugin): instantiate plugin capability via runtime class lookup
        let _capability_class =
            match self.get_capability_implementation_class_name(plugin, capability_class_name)? {
                Some(c) => c,
                None => return Ok(None),
            };
        // PHP: requires class_exists / new $capabilityClass($ctorArgs); cannot be performed in Rust without a runtime registry.
        Ok(None)
    }

    pub fn get_plugin_capabilities(
        &self,
        capability_class_name: &str,
        ctor_args: IndexMap<String, PhpMixed>,
    ) -> Vec<Box<dyn Capability>> {
        // TODO(plugin): aggregate capabilities across all loaded plugins
        let mut capabilities: Vec<Box<dyn Capability>> = vec![];
        for plugin in &self.get_plugins() {
            if let Ok(Some(capability)) =
                self.get_plugin_capability(&**plugin, capability_class_name, ctor_args.clone())
            {
                capabilities.push(capability);
            }
        }

        capabilities
    }

    fn parse_allowed_plugins(
        allow_plugins_config: PhpMixed,
        mut locker: Option<&mut Locker>,
    ) -> Option<IndexMap<String, bool>> {
        // PHP: [] === $allowPluginsConfig && $locker !== null && $locker->isLocked() && version_compare($locker->getPluginApi(), '2.2.0', '<')
        let is_empty_array = allow_plugins_config
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false);
        let plugin_api_under_2_2_0 = if is_empty_array {
            match locker.as_deref_mut() {
                Some(l) => {
                    if l.is_locked() {
                        let api = l.get_plugin_api().unwrap_or_default();
                        version_compare(&api, "2.2.0", "<")
                    } else {
                        false
                    }
                }
                None => false,
            }
        } else {
            false
        };
        if is_empty_array && locker.is_some() && plugin_api_under_2_2_0 {
            return None;
        }

        if allow_plugins_config.as_bool() == Some(true) {
            let mut m: IndexMap<String, bool> = IndexMap::new();
            m.insert("{}".to_string(), true);
            return Some(m);
        }

        if allow_plugins_config.as_bool() == Some(false) {
            let mut m: IndexMap<String, bool> = IndexMap::new();
            m.insert("{}".to_string(), false);
            return Some(m);
        }

        let mut rules: IndexMap<String, bool> = IndexMap::new();
        if let Some(arr) = allow_plugins_config.as_array() {
            for (pattern, allow) in arr {
                rules.insert(
                    base_package::package_name_to_regexp(pattern),
                    allow.as_bool().unwrap_or(false),
                );
            }
        }

        Some(rules)
    }

    pub fn are_plugins_disabled(&self, r#type: &str) -> bool {
        match (&self.disable_plugins, r#type) {
            (DisablePlugins::All, _) => true,
            (DisablePlugins::Local, "local") => true,
            (DisablePlugins::Global, "global") => true,
            _ => false,
        }
    }

    pub fn disable_plugins(&mut self) {
        self.disable_plugins = DisablePlugins::All;
    }

    pub fn is_plugin_allowed(
        &mut self,
        package: &str,
        is_global_plugin: bool,
        optional: bool,
        prompt: bool,
    ) -> anyhow::Result<bool> {
        // TODO(plugin): allow-plugins authorization flow with interactive prompt
        let rules: &mut Option<IndexMap<String, bool>> = if is_global_plugin {
            &mut self.allow_global_plugin_rules
        } else {
            &mut self.allow_plugin_rules
        };

        // This is a BC mode for lock files created pre-Composer-2.2 where the expectation of
        // an allow-plugins config being present cannot be made.
        if rules.is_none() {
            if !self.io.is_interactive() {
                self.io.write_error("<warning>For additional security you should declare the allow-plugins config with a list of packages names that are allowed to run code. See https://getcomposer.org/allow-plugins</warning>");
                self.io.write_error("<warning>This warning will become an exception once you run composer update!</warning>");

                let mut m: IndexMap<String, bool> = IndexMap::new();
                m.insert("{}".to_string(), true);
                *rules = Some(m);

                // if no config is defined we allow all plugins for BC
                return Ok(true);
            }

            // keep going and prompt the user
            *rules = Some(IndexMap::new());
        }

        let rules_snapshot: Vec<(String, bool)> = rules
            .as_ref()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        for (pattern, allow) in &rules_snapshot {
            if Preg::is_match3(pattern, package, None).unwrap_or(false) {
                return Ok(*allow);
            }
        }

        if package == "composer/package-versions-deprecated" {
            return Ok(false);
        }

        if self.io.is_interactive() && prompt {
            // TODO(plugin): interactive consent flow — preserved as a stub. PHP picks
            // $this->globalComposer's config when is_global_plugin; the stub uses the
            // local composer's config in both cases. Access the `composer` field directly
            // (not via `composer_full()`) so the borrow stays disjoint from `rules`.
            let config = self
                .composer
                .upgrade()
                .expect("PluginManager must not outlive Composer")
                .borrow()
                .get_config()
                .clone();

            self.io.write_error(&format!("<warning>{}{} contains a Composer plugin which is currently not in your allow-plugins config. See https://getcomposer.org/allow-plugins</warning>",
                package,
                if is_global_plugin || self.running_in_global_dir { " (installed globally)" } else { "" }
            ));
            let mut attempts = 0;
            loop {
                // do not allow more than 5 prints of the help message, at some point assume the
                // input is not interactive and bail defaulting to a disabled plugin
                let default = "?";
                if attempts > 5 {
                    self.io.write_error("Too many failed prompts, aborting.");
                    break;
                }

                let answer = self.io.ask(
                    format!("Do you trust \"<fg=green;options=bold>{}</>\" to execute code and wish to enable it now? (writes \"allow-plugins\" to composer.json) [<comment>y,n,d,?</comment>] ", package),
                    PhpMixed::String(default.to_string()),
                );
                let answer_str = answer.as_string().unwrap_or("");
                match answer_str {
                    "y" | "n" | "d" => {
                        let allow = answer_str == "y";

                        // persist answer in current rules to avoid prompting again if the package gets reloaded
                        rules
                            .as_mut()
                            .unwrap()
                            .insert(base_package::package_name_to_regexp(package), allow);

                        // persist answer in composer.json if it wasn't simply discarded
                        if answer_str == "y" || answer_str == "n" {
                            let allow_plugins_value =
                                config.borrow_mut().get("allow-plugins").clone();
                            if let Some(arr) = allow_plugins_value.as_array() {
                                let mut allow_plugins = arr.clone();
                                allow_plugins
                                    .insert(package.to_string(), Box::new(PhpMixed::Bool(allow)));
                                if config
                                    .borrow_mut()
                                    .get("sort-packages")
                                    .as_bool()
                                    .unwrap_or(false)
                                {
                                    ksort(&mut allow_plugins);
                                }
                                config
                                    .borrow_mut()
                                    .get_config_source_mut()
                                    .add_config_setting(
                                        "allow-plugins",
                                        PhpMixed::Array(allow_plugins.clone()),
                                    )?;
                                let mut inner: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                                inner.insert(
                                    "allow-plugins".to_string(),
                                    Box::new(PhpMixed::Array(allow_plugins)),
                                );
                                let mut config_section: IndexMap<String, Box<PhpMixed>> =
                                    IndexMap::new();
                                config_section
                                    .insert("config".to_string(), Box::new(PhpMixed::Array(inner)));
                                let wrap: IndexMap<String, PhpMixed> =
                                    config_section.into_iter().map(|(k, v)| (k, *v)).collect();
                                config
                                    .borrow_mut()
                                    .merge(&wrap, crate::config::Config::SOURCE_UNKNOWN);
                            }
                        }

                        return Ok(allow);
                    }
                    _ => {
                        attempts += 1;
                        let messages = vec![
                            "y - add package to allow-plugins in composer.json and let it run immediately".to_string(),
                            "n - add package (as disallowed) to allow-plugins in composer.json to suppress further prompts".to_string(),
                            "d - discard this, do not change composer.json and do not allow the plugin to run".to_string(),
                            "? - print help".to_string(),
                        ];
                        for m in &messages {
                            self.io.write_error(m);
                        }
                    }
                }
            }
        } else if optional {
            return Ok(false);
        }

        Err(PluginBlockedException::new(format!(
            "{}{} contains a Composer plugin which is blocked by your allow-plugins config. You may add it to the list if you consider it safe.\nYou can run \"composer {}config --no-plugins allow-plugins.{} [true|false]\" to enable it (true) or disable it explicitly and suppress this exception (false)\nSee https://getcomposer.org/allow-plugins",
            package,
            if is_global_plugin || self.running_in_global_dir { " (installed globally)" } else { "" },
            if is_global_plugin || self.running_in_global_dir { "global " } else { "" },
            package
        )).into())
    }
}
