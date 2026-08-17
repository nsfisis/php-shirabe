//! ref: composer/src/Composer/Plugin/PluginManager.php

use crate::composer::PartialComposerHandle;
use crate::composer::{ComposerHandle, ComposerWeakHandle};
use crate::event_dispatcher::{EventDispatcher, unwrap_php_result};
use crate::factory::DisablePlugins;
use crate::installer::InstallerInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::LockerInterface;
use crate::package::PackageInterfaceHandle;
use crate::package::RootPackageInterfaceHandle;
use crate::package::base_package::{self};
use crate::package::version::VersionParser;
use crate::plugin::PluginBlockedException;
use crate::plugin::capability::Capability;
use crate::plugin::php_plugin_proxy::{
    PhpCapabilityProxy, PhpCommandProviderProxy, PhpPluginProxy, PluginRpcDispatcher, php_is_a,
    plugin_handle_value,
};
use crate::plugin::plugin_interface::{self, PluginInterface};
use crate::repository::InstalledRepository;
use crate::repository::RepositoryInterfaceHandle;
use crate::repository::RepositoryUtils;
use crate::repository::RootPackageRepository;
use crate::util::PackageSorter;
use indexmap::IndexMap;
use shirabe_php_rpc::{PluginValue, call_function_with_dispatcher};
use shirabe_php_shim::{
    CmpOp, E_USER_DEPRECATED, PhpMixed, RuntimeException, UnexpectedValueException, dirname, empty,
    file_get_contents, implode, ksort, php_regex, preg_match2, preg_quote, preg_replace2, strrpos,
    strtr_array, substr, trigger_error, trim, var_export, var_export_str, version_compare,
};
use shirabe_semver::constraint::SimpleConstraint;

#[derive(Debug)]
pub struct PluginManager {
    composer: ComposerWeakHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    global_composer: Option<PartialComposerHandle>,
    version_parser: VersionParser,
    disable_plugins: DisablePlugins,
    // PHP stores the same plugin instance in both $plugins and $registeredPlugins (reference
    // semantics); shared handles preserve the identity comparisons that relies on.
    plugins: Vec<std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>>,
    registered_plugins: IndexMap<String, Vec<PluginOrInstaller>>,
    allow_plugin_rules: Option<IndexMap<String, bool>>,
    allow_global_plugin_rules: Option<IndexMap<String, bool>>,
    running_in_global_dir: bool,
    plugin_api_version_override: Option<String>,
}

#[derive(Debug)]
pub enum PluginOrInstaller {
    Plugin(std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>),
    Installer(std::rc::Rc<dyn InstallerInterface>),
}

/// PHP `private static $classCounter = 0;`.
static CLASS_COUNTER: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

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
            .get("allow-plugins");
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
            plugin_api_version_override: None,
        }
    }

    /// For testing only: makes `get_plugin_api_version` report `version` instead of the
    /// compiled-in constant, the seam PHPUnit obtains from
    /// `getMockBuilder(PluginManager::class)->onlyMethods(['getPluginApiVersion'])`.
    pub fn __set_plugin_api_version(&mut self, version: &str) {
        self.plugin_api_version_override = Some(version.to_string());
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
                &repo,
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
            self.load_repository(&repo, true, None)?;
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
            self.deactivate_repository(&repo, false)?;
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
            self.deactivate_repository(&repo, true)?;
        }

        Ok(())
    }

    /// Gets all currently active plugin instances
    ///
    /// PHP returns `$this->plugins` directly; the plugin objects are shared by reference, so this
    /// borrows the stored instances rather than cloning them.
    pub fn get_plugins(&self) -> &[std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>] {
        &self.plugins
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
                    return Err(RuntimeException::new(format!("Plugin {} is missing a require statement for a version of the composer-plugin-api package.", package.get_name())).into());
                }
            };

            let current_plugin_api_version = self.get_plugin_api_version();
            let current_plugin_api_constraint = SimpleConstraint::new(
                "==".to_string(),
                self.version_parser
                    .normalize(&current_plugin_api_version, None)?,
                None,
            );

            if requires_composer.get_pretty_string() == self.get_plugin_api_version() {
                self.io.write_error(&format!("<warning>The \"{}\" plugin requires composer-plugin-api {}, this *WILL* break in the future and it should be fixed ASAP (require ^{} instead for example).</warning>", package.get_name(), self.get_plugin_api_version(), self.get_plugin_api_version()));
            } else if !requires_composer.matches(&current_plugin_api_constraint.into()) {
                self.io.write_error(&format!("<warning>The \"{}\" plugin {}was skipped because it requires a Plugin API version (\"{}\") that does not match your Shirabe installation (\"{}\"). You may need to run shirabe update with the \"--no-plugins\" option.</warning>",
                    package.get_name(),
                    if is_global_plugin || self.running_in_global_dir { "(installed globally) " } else { "" },
                    requires_composer.get_pretty_string(),
                    current_plugin_api_version
                ));
                return Ok(());
            }

            if package.get_name() == "symfony/flex"
                && preg_match2(php_regex!("{^[0-9.]+$}"), &package.get_version(), 0).is_some()
                && version_compare(&package.get_version(), "1.9.8", CmpOp::Lt)
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
            self.io.write_error3(
                &format!(
                    "Skipped loading \"{}\" {}as it is not in config.allow-plugins",
                    package.get_name(),
                    if is_global_plugin || self.running_in_global_dir {
                        "(installed globally) "
                    } else {
                        ""
                    }
                ),
                true,
                crate::io::DEBUG,
            );
            return Ok(());
        }

        let old_installer_plugin = package.get_type() == "composer-installer";

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
            Some(PhpMixed::Float(f)) if *f == 0.0 => true,
            Some(PhpMixed::String(s)) if s.is_empty() || s == "0" => true,
            Some(PhpMixed::Array(a)) => a.is_empty(),
            Some(PhpMixed::List(l)) => l.is_empty(),
            _ => false,
        };
        if class_is_empty {
            return Err(UnexpectedValueException::new(format!("Error while installing {}, composer-plugin packages should have a class defined in their extra key to be usable.", package.get_pretty_name())).into());
        }
        // PHP: is_array($extra['class']) ? $extra['class'] : [$extra['class']] — an associative
        // array iterates its values too, and a non-string entry reaches class_exists() where it
        // raises a TypeError (an Error, not caught by the plugin installer's rollback).
        let expect_class_name = |value: &PhpMixed| -> String {
            value.as_string().map(|s| s.to_string()).unwrap_or_else(|| {
                panic!("extra.class entries must be strings (PHP raises a TypeError): {value:?}")
            })
        };
        let classes: Vec<String> = match class_value {
            Some(PhpMixed::List(items)) => items.iter().map(expect_class_name).collect(),
            Some(PhpMixed::Array(map)) => map.values().map(expect_class_name).collect(),
            Some(other) => vec![expect_class_name(other)],
            None => unreachable!("empty() above rejected a missing extra.class"),
        };

        let composer = self.composer_full();
        let local_repo = composer
            .borrow()
            .get_repository_manager()
            .borrow()
            .get_local_repository();
        let global_repo = self.global_composer.as_ref().map(|gc| {
            gc.borrow_partial()
                .get_repository_manager()
                .borrow()
                .get_local_repository()
        });

        let root_package = RootPackageInterfaceHandle::dup(composer.borrow().get_package());

        // clear files autoload rules from the root package as the root dependencies are not
        // necessarily all present yet when booting this runtime autoloader
        let mut root_package_autoloads = root_package.get_autoload();
        root_package_autoloads.insert("files".to_string(), PhpMixed::List(vec![]));
        root_package.set_autoload(root_package_autoloads);
        let mut root_package_autoloads = root_package.get_dev_autoload();
        root_package_autoloads.insert("files".to_string(), PhpMixed::List(vec![]));
        root_package.set_dev_autoload(root_package_autoloads);

        let root_package_repo =
            RepositoryInterfaceHandle::new(RootPackageRepository::new(root_package.clone()));
        let mut installed_repo =
            InstalledRepository::new(vec![local_repo.clone(), root_package_repo]);
        if let Some(global_repo) = &global_repo {
            installed_repo.add_repository(global_repo.clone());
        }

        let mut autoload_packages: IndexMap<String, PackageInterfaceHandle> = IndexMap::new();
        autoload_packages.insert(package.get_name().to_string(), package.clone());
        let autoload_packages =
            self.collect_dependencies(&installed_repo, autoload_packages, package.clone())?;

        let generator = composer.borrow().get_autoload_generator();
        let root_package_as_package: PackageInterfaceHandle = root_package.clone().into();
        let mut autoloads: Vec<(PackageInterfaceHandle, Option<String>)> =
            vec![(root_package_as_package.clone(), Some(String::new()))];
        for (_name, autoload_package) in &autoload_packages {
            if autoload_package.ptr_eq(&root_package_as_package) {
                continue;
            }

            let is_global_package = match &global_repo {
                Some(gr) => gr.borrow_mut().has_package(autoload_package.clone())?,
                None => false,
            };
            let install_path = self.get_install_path(autoload_package.clone(), is_global_package);
            let install_path = match install_path {
                Some(p) => p,
                None => continue,
            };
            autoloads.push((autoload_package.clone(), Some(install_path)));
        }

        let map =
            generator
                .borrow()
                .parse_autoloads(autoloads, root_package, PhpMixed::Bool(false));
        let vendor_dir = composer
            .borrow()
            .get_config()
            .borrow()
            .get("vendor-dir")
            .as_string()
            .map(|s| s.to_string());
        let mut class_loader = generator.borrow().create_loader(&map, vendor_dir);
        class_loader.register(false);

        // The plugin code runs in the PHP worker: load the Composer PHP runtime (contracts like
        // PluginInterface) and the reverse-RPC autoloader before touching plugin classes there.
        let cache_dir = composer
            .borrow()
            .get_config()
            .borrow()
            .get_str("cache-dir")?;
        EventDispatcher::ensure_composer_php_runtime(std::path::Path::new(&cache_dir))?;
        EventDispatcher::ensure_script_autoloader()?;

        if let Some(files) = map.get("files").and_then(|v| v.as_array()) {
            for (file_identifier, file) in files {
                // exclude laminas/laminas-zendframework-bridge:src/autoload.php as it breaks Composer in some conditions
                // see https://github.com/composer/composer/issues/10349 and https://github.com/composer/composer/issues/10401
                // this hack can be removed once this deprecated package stop being installed
                if file_identifier == "7e9bd612cc444b3eed788ebbe46263a0" {
                    continue;
                }
                let file = file.as_string().unwrap_or_else(|| {
                    panic!("autoload files entry `{file_identifier}` is not a string path")
                });
                self.php_runtime_composer_require(file_identifier, file)?;
            }
        }

        for class in classes {
            let mut class = class;
            if self.php_runtime_class_exists(&class, false)? {
                class = trim(&class, Some("\\")).to_string();
                let path = class_loader.find_file(&class).unwrap_or_else(|| {
                    panic!("plugin class `{class}` is already defined but has no autoloadable file")
                });
                // TODO(bytes): file_get_contents is lossy UTF-8; the eval'd plugin source
                // should be carried as bytes.
                let code = file_get_contents(&path)
                    .unwrap_or_else(|| panic!("unable to read the plugin class file `{path}`"));
                let class_counter = CLASS_COUNTER.load(std::sync::atomic::Ordering::Relaxed);
                let separator_pos = strrpos(&class, "\\");
                let mut class_name = class.clone();
                // PHP: `if ($separatorPos)` — position 0 is falsy and keeps the full name.
                if let Some(separator_pos) = separator_pos
                    && separator_pos != 0
                {
                    class_name = substr(&class, (separator_pos + 1) as i64, None);
                }
                let code = preg_replace2(
                    format!(
                        "{{^((?:(?:final|readonly)\\s+)*(?:\\s*))class\\s+({})}}mi",
                        preg_quote(&class_name, None)
                    ),
                    &format!("$1class $2_composer_tmp{}", class_counter),
                    &code,
                    1,
                    None,
                );
                let mut replacements: IndexMap<String, String> = IndexMap::new();
                replacements.insert("__FILE__".to_string(), var_export_str(&path, true));
                replacements.insert("__DIR__".to_string(), var_export_str(&dirname(&path), true));
                replacements.insert("__CLASS__".to_string(), var_export_str(&class, true));
                let code = strtr_array(&code, &replacements);
                let code = preg_replace2(r"/^\s*<\?(php)?/i", "", &code, 1, None);
                self.php_runtime_eval(&code)?;
                class = format!("{}_composer_tmp{}", class, class_counter);
                CLASS_COUNTER.store(class_counter + 1, std::sync::atomic::Ordering::Relaxed);
            }

            if old_installer_plugin {
                if !self.php_runtime_is_a(&class, "Composer\\Installer\\InstallerInterface")? {
                    return Err(RuntimeException::new(format!(
                        "Could not activate plugin \"{}\" as \"{}\" does not implement Composer\\Installer\\InstallerInterface",
                        package.get_name(),
                        class
                    ))
                    .into());
                }
                self.io.write_error(&format!(
                    "<warning>Loading \"{}\" {}which is a legacy composer-installer built for Composer 1.x, it is likely to cause issues as you are running Composer 2.x.</warning>",
                    package.get_name(),
                    if is_global_plugin || self.running_in_global_dir {
                        "(installed globally) "
                    } else {
                        ""
                    }
                ));
                let composer = self.composer_full();
                let handle = self.php_runtime_new_object_with_args(
                    &class,
                    vec![
                        crate::plugin::io_handle_value(&self.io)?,
                        crate::plugin::composer_handle_value(&composer),
                    ],
                )?;
                let installer = crate::plugin::php_installer_proxy(handle);
                // A shared borrow: this runs inside `InstallationManager::execute`, which holds
                // one of its own for the whole run.
                composer
                    .borrow()
                    .get_installation_manager()
                    .borrow()
                    .add_installer(installer.clone());
                self.registered_plugins
                    .entry(package.get_name().to_string())
                    .or_default()
                    .push(PluginOrInstaller::Installer(installer));
            } else if self.php_runtime_class_exists(&class, true)? {
                if !self.php_runtime_is_a(&class, "Composer\\Plugin\\PluginInterface")? {
                    return Err(RuntimeException::new(format!(
                        "Could not activate plugin \"{}\" as \"{}\" does not implement Composer\\Plugin\\PluginInterface",
                        package.get_name(),
                        class
                    ))
                    .into());
                }
                let handle = self.php_runtime_new_object(&class)?;
                let plugin: std::rc::Rc<std::cell::RefCell<dyn PluginInterface>> =
                    std::rc::Rc::new(std::cell::RefCell::new(PhpPluginProxy::new(
                        handle.phandle,
                        handle.class,
                        handle.implements,
                    )));
                self.add_plugin(plugin.clone(), is_global_plugin, Some(package.clone()))?;
                self.registered_plugins
                    .entry(package.get_name().to_string())
                    .or_default()
                    .push(PluginOrInstaller::Plugin(plugin));
            } else if fail_on_missing_classes {
                return Err(UnexpectedValueException::new(format!(
                    "Plugin {} could not be initialized, class not found: {}",
                    package.get_name(),
                    class
                ))
                .into());
            }
        }
        Ok(())
    }

    /// Runs a boolean runtime query in the PHP worker with the plugin dispatcher active.
    fn php_runtime_bool(&self, function: &str, args: Vec<PluginValue>) -> anyhow::Result<bool> {
        let value = unwrap_php_result(call_function_with_dispatcher(
            function,
            args,
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        match value {
            PluginValue::Bool(value) => Ok(value),
            other => Err(anyhow::anyhow!(
                "PHP runtime query `{function}` did not return a bool: {other:?}"
            )),
        }
    }

    fn php_runtime_class_exists(&self, class: &str, autoload: bool) -> anyhow::Result<bool> {
        self.php_runtime_bool(
            "class_exists",
            vec![PluginValue::string(class), PluginValue::Bool(autoload)],
        )
    }

    fn php_runtime_is_a(&self, class: &str, interface: &str) -> anyhow::Result<bool> {
        self.php_runtime_bool(
            "is_a",
            vec![
                PluginValue::string(class),
                PluginValue::string(interface),
                PluginValue::Bool(true),
            ],
        )
    }

    fn php_runtime_eval(&self, code: &str) -> anyhow::Result<()> {
        unwrap_php_result(call_function_with_dispatcher(
            "__shirabe_eval",
            vec![PluginValue::string(code)],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        Ok(())
    }

    fn php_runtime_composer_require(
        &self,
        file_identifier: &str,
        file: &str,
    ) -> anyhow::Result<()> {
        unwrap_php_result(call_function_with_dispatcher(
            "__shirabe_composer_require",
            vec![
                PluginValue::string(file_identifier),
                PluginValue::string(file),
            ],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        Ok(())
    }

    /// PHP `new $class()` in the worker, returning the P-table handle of the new entity.
    fn php_runtime_new_object(&self, class: &str) -> anyhow::Result<shirabe_php_rpc::PhpObjHandle> {
        self.php_runtime_new_object_with_args(class, vec![])
    }

    fn php_runtime_new_object_with_args(
        &self,
        class: &str,
        args: Vec<PluginValue>,
    ) -> anyhow::Result<shirabe_php_rpc::PhpObjHandle> {
        let value = unwrap_php_result(shirabe_php_rpc::new_object(
            class,
            args,
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        match value {
            PluginValue::PhpHandle(handle) => Ok(handle),
            other => Err(anyhow::anyhow!(
                "instantiating `{class}` did not return a PHP handle: {other:?}"
            )),
        }
    }

    /// Deactivates a plugin package
    pub fn deactivate_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        if !self.registered_plugins.contains_key(&package.get_name()) {
            return Ok(());
        }

        // PHP unsets registeredPlugins only after the loop; a deactivate() throw must leave the
        // entry observable, so removal happens last here too. Plugins are cloned out per index
        // (shared handles); installer entries are only referenced while calling removeInstaller.
        let name = package.get_name();
        let count = self
            .registered_plugins
            .get(&name)
            .map(|entries| entries.len())
            .unwrap_or(0);
        for index in 0..count {
            let plugin = match &self.registered_plugins.get(&name).unwrap()[index] {
                PluginOrInstaller::Plugin(p) => Some(p.clone()),
                PluginOrInstaller::Installer(_) => None,
            };
            match plugin {
                Some(p) => self.remove_plugin(&p)?,
                None => {
                    let composer = self.composer_full();
                    let installation_manager = composer.borrow().get_installation_manager();
                    if let PluginOrInstaller::Installer(inst) =
                        &self.registered_plugins.get(&name).unwrap()[index]
                    {
                        installation_manager.borrow().remove_installer(&**inst);
                    }
                }
            }
        }
        self.registered_plugins.shift_remove(&name);
        Ok(())
    }

    /// Uninstall a plugin package
    pub fn uninstall_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        if !self.registered_plugins.contains_key(&package.get_name()) {
            return Ok(());
        }

        // PHP unsets registeredPlugins only after the loop, as in deactivate_package.
        let name = package.get_name();
        let count = self
            .registered_plugins
            .get(&name)
            .map(|entries| entries.len())
            .unwrap_or(0);
        for index in 0..count {
            let plugin = match &self.registered_plugins.get(&name).unwrap()[index] {
                PluginOrInstaller::Plugin(p) => Some(p.clone()),
                PluginOrInstaller::Installer(_) => None,
            };
            match plugin {
                Some(p) => {
                    self.remove_plugin(&p)?;
                    self.uninstall_plugin(&p)?;
                }
                None => {
                    let composer = self.composer_full();
                    let installation_manager = composer.borrow().get_installation_manager();
                    if let PluginOrInstaller::Installer(inst) =
                        &self.registered_plugins.get(&name).unwrap()[index]
                    {
                        installation_manager.borrow().remove_installer(&**inst);
                    }
                }
            }
        }
        self.registered_plugins.shift_remove(&name);
        Ok(())
    }

    /// Returns the version of the internal composer-plugin-api package.
    fn get_plugin_api_version(&self) -> String {
        match &self.plugin_api_version_override {
            Some(version) => version.clone(),
            None => plugin_interface::PLUGIN_API_VERSION.to_string(),
        }
    }

    /// Adds a plugin, activates it and registers it with the event dispatcher
    pub fn add_plugin(
        &mut self,
        plugin: std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
        is_global_plugin: bool,
        source_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        if self.are_plugins_disabled(if is_global_plugin { "global" } else { "local" }) {
            return Ok(());
        }

        match &source_package {
            None => {
                trigger_error(
                    "Calling PluginManager::addPlugin without $sourcePackage is deprecated, if you are using this please get in touch with us to explain the use case",
                    E_USER_DEPRECATED,
                );
            }
            Some(sp) => {
                let plugin_optional = sp
                    .get_extra()
                    .get("plugin-optional")
                    .map(|v| v.as_bool() == Some(true))
                    .unwrap_or(false);
                if !self.is_plugin_allowed(
                    &sp.get_name(),
                    is_global_plugin,
                    plugin_optional,
                    true,
                )? {
                    self.io.write_error3(
                        &format!(
                            "Skipped loading \"{} from {}\" {} as it is not in config.allow-plugins",
                            plugin.borrow().get_class_name(),
                            sp.get_name(),
                            if is_global_plugin || self.running_in_global_dir {
                                "(installed globally) "
                            } else {
                                ""
                            }
                        ),
                        true,
                        crate::io::DEBUG,
                    );
                    return Ok(());
                }
            }
        }

        let mut details: Vec<String> = vec![];
        if let Some(sp) = source_package.as_ref() {
            details.push(format!("from {}", sp.get_name()));
        }
        if is_global_plugin || self.running_in_global_dir {
            details.push("installed globally".to_string());
        }
        self.io.write_error3(
            &format!(
                "Loading plugin {}{}",
                plugin.borrow().get_class_name(),
                if !details.is_empty() {
                    format!(" ({})", implode(", ", &details))
                } else {
                    String::new()
                }
            ),
            true,
            crate::io::DEBUG,
        );
        self.plugins.push(plugin.clone());
        plugin
            .borrow_mut()
            .activate(self.composer_full(), self.io.clone())?;

        let plugin_ref = plugin.borrow();
        if let Some(subscriber) = plugin_ref.as_event_subscriber() {
            let event_dispatcher = self.composer_full().borrow().get_event_dispatcher();
            let result = event_dispatcher.borrow_mut().add_subscriber(subscriber);
            result?;
        }
        Ok(())
    }

    /// Removes a plugin, deactivates it and removes any listener the plugin has set on the plugin instance
    pub fn remove_plugin(
        &mut self,
        plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
    ) -> anyhow::Result<()> {
        // PHP uses identity (`===`) comparison via array_search($plugin, $this->plugins, true).
        let index = self
            .plugins
            .iter()
            .position(|p| std::rc::Rc::ptr_eq(p, plugin));
        let index = match index {
            Some(i) => i,
            None => return Ok(()),
        };

        self.io.write_error3(
            &format!("Unloading plugin {}", plugin.borrow().get_class_name()),
            true,
            crate::io::DEBUG,
        );
        let removed = self.plugins.remove(index);
        removed
            .borrow_mut()
            .deactivate(self.composer_full(), self.io.clone())?;

        // PHP passes the plugin object itself; its cross-RPC identity (the P-table handle) is
        // carried by the subscriber accessor, and `addSubscriber` is the only source of
        // listeners capturing a plugin object today.
        // TODO(plugin): a plugin registering `[$this, 'method']` listeners directly through an
        // EventDispatcher proxy would have no removal path here; no such RPC surface exists yet.
        let subscriber_handle = removed
            .borrow()
            .as_event_subscriber()
            .map(|s| s.subscriber_handle());
        if let Some(handle) = subscriber_handle {
            let event_dispatcher = self.composer_full().borrow().get_event_dispatcher();
            event_dispatcher.borrow_mut().remove_listener(&handle);
        }
        Ok(())
    }

    /// Notifies a plugin it is being uninstalled and should clean up
    pub fn uninstall_plugin(
        &self,
        plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
    ) -> anyhow::Result<()> {
        self.io.write_error3(
            &format!("Uninstalling plugin {}", plugin.borrow().get_class_name()),
            true,
            crate::io::DEBUG,
        );
        plugin
            .borrow_mut()
            .uninstall(self.composer_full(), self.io.clone())?;
        Ok(())
    }

    // The repository stays behind its shared handle (borrowed only transiently) because
    // register_package re-enters the same local repository through the RepositoryManager.
    fn load_repository(
        &mut self,
        repo: &RepositoryInterfaceHandle,
        is_global_repo: bool,
        root_package: Option<RootPackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
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

        let sorted_packages = PackageSorter::sort_packages(packages.to_vec(), weights);
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
        repo: &RepositoryInterfaceHandle,
        _is_global_repo: bool,
    ) -> anyhow::Result<()> {
        let packages = repo.get_packages()?;
        // PHP: $sortedPackages = array_reverse(PackageSorter::sortPackages($packages));
        let mut sorted_packages = PackageSorter::sort_packages(packages.to_vec(), IndexMap::new());
        sorted_packages.reverse();

        for package in &sorted_packages {
            if package.as_complete_package().is_none() {
                continue;
            }
            if "composer-plugin" == package.get_type() {
                self.deactivate_package(package.clone())?;
            // Backward compatibility
            } else if "composer-installer" == package.get_type() {
                self.deactivate_package(package.clone())?;
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
                    collected.insert(required_package.get_name(), required_package.clone());
                    collected = self.collect_dependencies(
                        installed_repo,
                        collected,
                        required_package.clone(),
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
            // Shared borrow: this runs re-entrantly while InstallationManager::execute holds a
            // shared borrow of the same manager handle.
            return self
                .composer_full()
                .borrow()
                .get_installation_manager()
                .borrow()
                .get_install_path(package);
        }

        // PHP: assert(null !== $this->globalComposer);
        self.global_composer
            .as_ref()
            .unwrap()
            .borrow_partial()
            .get_installation_manager()
            .borrow()
            .get_install_path(package)
    }

    fn get_capability_implementation_class_name(
        &self,
        plugin: &dyn PluginInterface,
        capability: &str,
    ) -> anyhow::Result<Option<String>> {
        let capable = match plugin.as_capable() {
            Some(c) => c,
            None => return Ok(None),
        };

        let capabilities = capable.get_capabilities()?;

        // PHP: !empty($capabilities[$capability]) && is_string($capabilities[$capability]) && trim($capabilities[$capability])
        if let Some(value) = capabilities.get(capability)
            && !empty(value)
            && let PhpMixed::String(s) = value
        {
            let trimmed = trim(s, Some(" \t\n\r\0\u{0B}"));
            // PHP evaluates trim(...) in boolean context: "" and "0" are falsy.
            if !trimmed.is_empty() && trimmed != "0" {
                return Ok(Some(trimmed));
            }
        }

        // PHP: array_key_exists($capability, $capabilities) && (empty(...) || !is_string(...)
        // || !trim(...)). Once the first branch has declined, a present key always fails one
        // of the three disjuncts, so a present key unconditionally throws here.
        if let Some(value) = capabilities.get(capability) {
            return Err(UnexpectedValueException::new(format!(
                "Plugin {} provided invalid capability class name(s), got {}",
                plugin.get_class_name(),
                var_export(value, true)
            ))
            .into());
        }

        Ok(None)
    }

    pub fn get_plugin_capability(
        &self,
        plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
        capability_class_name: &str,
        ctor_args: IndexMap<String, PluginValue>,
    ) -> anyhow::Result<Option<Box<dyn Capability>>> {
        let capability_class = match self
            .get_capability_implementation_class_name(&*plugin.borrow(), capability_class_name)?
        {
            Some(c) => c,
            None => return Ok(None),
        };

        // PHP: if (!class_exists($capabilityClass))
        let exists = unwrap_php_result(call_function_with_dispatcher(
            "class_exists",
            vec![PluginValue::string(capability_class.clone())],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        if !matches!(exists, PluginValue::Bool(true)) {
            return Err(RuntimeException::new(format!(
                "Cannot instantiate Capability, as class {} from plugin {} does not exist.",
                capability_class,
                plugin.borrow().get_class_name()
            ))
            .into());
        }

        // PHP: $ctorArgs['plugin'] = $plugin; the capability constructor receives the plugin
        // instance itself. A PHP-implemented plugin already has an entity in the child's P
        // table; a Rust-implemented one crosses as a handle to its R-table entity.
        let plugin_value = match plugin.borrow().__as_php_plugin_proxy() {
            Some(proxy) => PluginValue::PhpHandle(shirabe_php_rpc::PhpObjHandle {
                phandle: proxy.phandle,
                class: proxy.class.clone(),
                implements: proxy.implements.clone(),
            }),
            None => plugin_handle_value(plugin),
        };
        let mut ctor_args = ctor_args;
        ctor_args.insert("plugin".to_string(), plugin_value);
        let args_value = PluginValue::Array(
            ctor_args
                .into_iter()
                .map(|(k, v)| (k.into_bytes(), v))
                .collect(),
        );

        // PHP: $capabilityObj = new $capabilityClass($ctorArgs);
        let capability_obj = unwrap_php_result(shirabe_php_rpc::new_object(
            &capability_class,
            vec![args_value],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        let handle = match capability_obj {
            PluginValue::PhpHandle(handle) => handle,
            other => anyhow::bail!(
                "worker returned a non-object for new {capability_class}(...): {other:?}"
            ),
        };

        // PHP: if (!$capabilityObj instanceof Capability || !$capabilityObj instanceof $capabilityClassName)
        if !php_is_a(&handle, "Composer\\Plugin\\Capability\\Capability")?
            || !php_is_a(&handle, capability_class_name)?
        {
            return Err(RuntimeException::new(format!(
                "Class {capability_class} must implement both Composer\\Plugin\\Capability\\Capability and {capability_class_name}."
            ))
            .into());
        }

        match capability_class_name {
            "Composer\\Plugin\\Capability\\CommandProvider" => {
                Ok(Some(Box::new(PhpCommandProviderProxy::new(handle))))
            }
            "Composer\\Plugin\\Capability\\Capability" => {
                Ok(Some(Box::new(PhpCapabilityProxy::new(handle))))
            }
            // A capability interface outside composer-plugin-api that the instanceof checks
            // accepted (the plugin ships its own): no Rust adapter exists for its methods.
            other => anyhow::bail!("capability {other} is recognized but not yet adapted"),
        }
    }

    pub fn get_plugin_capabilities(
        &self,
        capability_class_name: &str,
        ctor_args: IndexMap<String, PluginValue>,
    ) -> anyhow::Result<Vec<Box<dyn Capability>>> {
        let mut capabilities: Vec<Box<dyn Capability>> = vec![];
        for plugin in self.get_plugins() {
            if let Some(capability) =
                self.get_plugin_capability(plugin, capability_class_name, ctor_args.clone())?
            {
                capabilities.push(capability);
            }
        }

        Ok(capabilities)
    }

    fn parse_allowed_plugins(
        allow_plugins_config: PhpMixed,
        mut locker: Option<&mut dyn LockerInterface>,
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
                        version_compare(&api, "2.2.0", CmpOp::Lt)
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
        matches!(
            (&self.disable_plugins, r#type),
            (DisablePlugins::All, _)
                | (DisablePlugins::Local, "local")
                | (DisablePlugins::Global, "global")
        )
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
            if preg_match2(pattern, package, 0).is_some() {
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
                .get_config();

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
                )?;
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
                            let allow_plugins_value = config.borrow_mut().get("allow-plugins");
                            if let Some(arr) = allow_plugins_value.as_array() {
                                let mut allow_plugins = arr.clone();
                                allow_plugins.insert(package.to_string(), PhpMixed::Bool(allow));
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
                                let mut inner = IndexMap::new();
                                inner.insert(
                                    "allow-plugins".to_string(),
                                    PhpMixed::Array(allow_plugins),
                                );
                                let mut config_section = IndexMap::new();
                                config_section.insert("config".to_string(), PhpMixed::Array(inner));
                                config
                                    .borrow_mut()
                                    .merge(&config_section, crate::config::Config::SOURCE_UNKNOWN);
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
            "{}{} contains a Composer plugin which is blocked by your allow-plugins config. You may add it to the list if you consider it safe.\nYou can run \"shirabe {}config --no-plugins allow-plugins.{} [true|false]\" to enable it (true) or disable it explicitly and suppress this exception (false)\nSee https://getcomposer.org/allow-plugins",
            package,
            if is_global_plugin || self.running_in_global_dir { " (installed globally)" } else { "" },
            if is_global_plugin || self.running_in_global_dir { "global " } else { "" },
            package
        )).into())
    }
}
