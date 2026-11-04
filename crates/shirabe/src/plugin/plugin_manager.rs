//! ref: composer/src/Composer/Plugin/PluginManager.php

use crate::composer::{ComposerHandle, ComposerWeakHandle, PartialComposerHandle};
use crate::event_dispatcher::EventDispatcher;
use crate::factory::DisablePlugins;
use crate::installer::InstallerInterface;
use crate::io::{IOInterface, IOInterfaceImmutable, IOInterfaceWrite};
use crate::package::version::VersionParser;
use crate::package::{
    LockerInterface, PackageInterfaceHandle, RootPackageInterfaceHandle, base_package,
};
use crate::plugin::PluginBlockedException;
use crate::plugin::capability::Capability;
use crate::plugin::php_plugin_proxy::{
    PhpCapabilityProxy, PhpCommandProviderProxy, PhpPluginProxy, PluginRpcDispatcher, php_is_a,
    plugin_handle_value,
};
use crate::plugin::plugin_interface::{self, PluginInterface};
use crate::plugin::throwable_bridge::unwrap_php_result;
use crate::repository::{
    InstalledRepository, RepositoryInterfaceHandle, RepositoryUtils, RootPackageRepository,
};
use crate::util::PackageSorter;
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{PluginValue, call_function_with_dispatcher};
use shirabe_php_shim::{
    CmpOp, E_USER_DEPRECATED, PhpMixed, RuntimeException, UnexpectedValueException, bconcat,
    dirname, display_to_bytes, empty, file_get_contents, implode, ksort, path_from_bytes,
    php_regex, preg_is_match, preg_quote, preg_replace2, strrpos, strtr_array, substr,
    trigger_error, trim2, var_export, var_export_str, version_compare,
};
use shirabe_semver::constraint::SimpleConstraint;

#[derive(Debug)]
pub struct PluginManager {
    composer: ComposerWeakHandle,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    global_composer: Option<PartialComposerHandle>,
    version_parser: VersionParser,
    disable_plugins: DisablePlugins,
    plugins: Vec<std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>>,
    registered_plugins: IndexMap<BString, Vec<PluginOrInstaller>>,
    allow_plugin_rules: Option<IndexMap<BString, bool>>,
    allow_global_plugin_rules: Option<IndexMap<BString, bool>>,
    running_in_global_dir: bool,
    plugin_api_version_override: Option<BString>,
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
    pub fn __set_plugin_api_version(&mut self, version: &BStr) {
        self.plugin_api_version_override = Some(version.to_owned());
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
        if !self.are_plugins_disabled(BStr::new("local")) {
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

        if self.global_composer.is_some() && !self.are_plugins_disabled(BStr::new("global")) {
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
        if !self.are_plugins_disabled(BStr::new("local")) {
            let repo = self
                .composer_full()
                .borrow()
                .get_repository_manager()
                .borrow()
                .get_local_repository();
            self.deactivate_repository(&repo, false)?;
        }

        if self.global_composer.is_some() && !self.are_plugins_disabled(BStr::new("global")) {
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
    pub fn get_plugins(&self) -> &[std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>] {
        &self.plugins
    }

    /// Gets all currently active plugin instances
    ///
    /// internal — Plugin package names which are currently active
    pub fn get_registered_plugins(&self) -> Vec<BString> {
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
        if self.are_plugins_disabled(if is_global_plugin {
            BStr::new("global")
        } else {
            BStr::new("local")
        }) {
            self.io.write_error(bconcat!(
                "<warning>The \"",
                package.get_name(),
                "\" plugin was not loaded as plugins are disabled.</warning>"
            ));

            return Ok(());
        }

        if package.get_type() == "composer-plugin" {
            let requires_map = package.get_requires();
            let mut requires_composer: Option<&shirabe_semver::constraint::AnyConstraint> = None;
            for (_k, link) in requires_map.iter() {
                if "composer-plugin-api" == link.get_target() {
                    requires_composer = Some(link.get_constraint());
                    break;
                }
            }

            let requires_composer = match requires_composer {
                Some(r) => r,
                None => {
                    return Err(RuntimeException::new(bconcat!("Plugin ", package.get_name(), " is missing a require statement for a version of the composer-plugin-api package.")).into());
                }
            };

            let current_plugin_api_version = self.get_plugin_api_version();
            let current_plugin_api_constraint = SimpleConstraint::new(
                "==",
                self.version_parser
                    .normalize(&current_plugin_api_version, None)?,
                None,
            );

            if requires_composer.get_pretty_string() == self.get_plugin_api_version() {
                self.io.write_error(bconcat!(
                    "<warning>The \"",
                    package.get_name(),
                    "\" plugin requires composer-plugin-api ",
                    self.get_plugin_api_version(),
                    ", this *WILL* break in the future and it should be fixed ASAP (require ^",
                    self.get_plugin_api_version(),
                    " instead for example).</warning>"
                ));
            } else if !requires_composer.matches(&current_plugin_api_constraint.into()) {
                self.io.write_error(bconcat!("<warning>The \"", package.get_name(), "\" plugin ", if is_global_plugin || self.running_in_global_dir { "(installed globally) " } else { "" }, "was skipped because it requires a Plugin API version (\"", requires_composer.get_pretty_string(), "\") that does not match your Shirabe installation (\"", current_plugin_api_version, "\"). You may need to run shirabe update with the \"--no-plugins\" option.</warning>"));
                return Ok(());
            }

            if package.get_name() == "symfony/flex"
                && preg_is_match(php_regex!("{^[0-9.]+$}"), package.get_version())
                && version_compare(package.get_version(), "1.9.8", CmpOp::Lt)
            {
                self.io.write_error(bconcat!("<warning>The \"", package.get_name(), "\" plugin ", if is_global_plugin || self.running_in_global_dir { "(installed globally) " } else { "" }, "was skipped because it is not compatible with Composer 2+. Make sure to update it to version 1.9.8 or greater.</warning>"));
                return Ok(());
            }
        }

        let plugin_optional = package
            .get_extra()
            .get(B("plugin-optional"))
            .map(|v| v.as_bool() == Some(true))
            .unwrap_or(false);
        if !self.is_plugin_allowed(package.get_name(), is_global_plugin, plugin_optional, true)? {
            self.io.write_error3(
                bconcat!(
                    "Skipped loading \"",
                    package.get_name(),
                    "\" ",
                    if is_global_plugin || self.running_in_global_dir {
                        "(installed globally) "
                    } else {
                        ""
                    },
                    "as it is not in config.allow-plugins"
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
        let class_value = extra.get(B("class"));
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
            return Err(UnexpectedValueException::new(bconcat!("Error while installing ", package.get_pretty_name(), ", composer-plugin packages should have a class defined in their extra key to be usable.")).into());
        }
        // PHP: is_array($extra['class']) ? $extra['class'] : [$extra['class']] — an associative
        // array iterates its values too, and a non-string entry reaches class_exists() where it
        // raises a TypeError (an Error, not caught by the plugin installer's rollback).
        let expect_class_name = |value: &PhpMixed| -> BString {
            value.as_string().map(BString::from).unwrap_or_else(|| {
                panic!("extra.class entries must be strings (PHP raises a TypeError): {value:?}")
            })
        };
        let classes: Vec<BString> = match class_value {
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
        root_package_autoloads.insert("files".into(), PhpMixed::List(vec![]));
        root_package.set_autoload(root_package_autoloads);
        let mut root_package_autoloads = root_package.get_dev_autoload();
        root_package_autoloads.insert("files".into(), PhpMixed::List(vec![]));
        root_package.set_dev_autoload(root_package_autoloads);

        let root_package_repo =
            RepositoryInterfaceHandle::new(RootPackageRepository::new(root_package.clone()));
        let mut installed_repo =
            InstalledRepository::new(vec![local_repo.clone(), root_package_repo]);
        if let Some(global_repo) = &global_repo {
            installed_repo.add_repository(global_repo.clone());
        }

        let mut autoload_packages: IndexMap<BString, PackageInterfaceHandle> = IndexMap::new();
        autoload_packages.insert(package.get_name(), package.clone());
        let autoload_packages =
            self.collect_dependencies(&installed_repo, autoload_packages, package.clone())?;

        let generator = composer.borrow().get_autoload_generator();
        let root_package_as_package: PackageInterfaceHandle = root_package.clone().into();
        let mut autoloads: Vec<(PackageInterfaceHandle, Option<std::path::PathBuf>)> = vec![(
            root_package_as_package.clone(),
            Some(std::path::PathBuf::new()),
        )];
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
            .map(|s| s.to_owned());
        let mut class_loader = generator.borrow().create_loader(&map, vendor_dir)?;
        class_loader.register(false);

        // The plugin code runs in the PHP worker: load the Composer PHP runtime (contracts like
        // PluginInterface) and the reverse-RPC autoloader before touching plugin classes there.
        let cache_dir = composer
            .borrow()
            .get_config()
            .borrow()
            .get_str("cache-dir")?;
        EventDispatcher::ensure_composer_php_runtime(path_from_bytes(&cache_dir))?;
        EventDispatcher::ensure_script_autoloader()?;

        if let Some(files) = map.get(B("files")).and_then(|v| v.as_array()) {
            for (file_identifier, file) in files {
                // exclude laminas/laminas-zendframework-bridge:src/autoload.php as it breaks Composer in some conditions
                // see https://github.com/composer/composer/issues/10349 and https://github.com/composer/composer/issues/10401
                // this hack can be removed once this deprecated package stop being installed
                if file_identifier == "7e9bd612cc444b3eed788ebbe46263a0" {
                    continue;
                }
                let file = file.as_string().unwrap_or_else(|| {
                    panic!("autoload files entry {file_identifier:?} is not a string path")
                });
                self.php_runtime_composer_require(file_identifier.as_bstr(), file)?;
            }
        }

        for class in classes {
            let mut class = class;
            if self.php_runtime_class_exists(class.as_bstr(), false)? {
                class = trim2(&class, "\\");
                let path = class_loader.find_file(&class).unwrap_or_else(|| {
                    panic!(
                        "plugin class `{}` is already defined but has no autoloadable file",
                        class.display_lossy()
                    )
                });
                let code = file_get_contents(path_from_bytes(&path)).unwrap_or_else(|_| {
                    panic!(
                        "unable to read the plugin class file `{}`",
                        path.display_lossy()
                    )
                });
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
                    bconcat!(
                        "{^((?:(?:final|readonly)\\s+)*(?:\\s*))class\\s+(",
                        preg_quote(&class_name, None),
                        ")}mi"
                    ),
                    bconcat!("$1class $2_composer_tmp", class_counter),
                    &code,
                    1,
                    None,
                );
                let mut replacements: IndexMap<BString, BString> = IndexMap::new();
                replacements.insert(BString::from("__FILE__"), var_export_str(&path, true));
                replacements.insert(
                    BString::from("__DIR__"),
                    var_export_str(dirname(&path), true),
                );
                replacements.insert(BString::from("__CLASS__"), var_export_str(&class, true));
                let code = strtr_array(&code, &replacements);
                let code = preg_replace2(r"/^\s*<\?(php)?/i", "", &code, 1, None);
                self.php_runtime_eval(code)?;
                class = bconcat!(class, "_composer_tmp", class_counter);
                CLASS_COUNTER.store(class_counter + 1, std::sync::atomic::Ordering::Relaxed);
            }

            if old_installer_plugin {
                if !self.php_runtime_is_a(
                    class.as_bstr(),
                    BStr::new("Composer\\Installer\\InstallerInterface"),
                )? {
                    return Err(RuntimeException::new(bconcat!(
                        "Could not activate plugin \"",
                        package.get_name(),
                        "\" as \"",
                        class,
                        "\" does not implement Composer\\Installer\\InstallerInterface"
                    ))
                    .into());
                }
                self.io.write_error(bconcat!("<warning>Loading \"", package.get_name(), "\" ", if is_global_plugin || self.running_in_global_dir {
                        "(installed globally) "
                    } else {
                        ""
                    }, "which is a legacy composer-installer built for Composer 1.x, it is likely to cause issues as you are running Composer 2.x.</warning>"));
                let composer = self.composer_full();
                let handle = self.php_runtime_new_object_with_args(
                    class.as_bstr(),
                    vec![
                        crate::plugin::io_handle_value(&self.io)?,
                        crate::plugin::composer_handle_value(&composer),
                    ],
                )?;
                let installer = crate::plugin::php_installer_proxy(handle);
                composer
                    .borrow()
                    .get_installation_manager()
                    .borrow()
                    .add_installer(installer.clone());
                self.registered_plugins
                    .entry(package.get_name())
                    .or_default()
                    .push(PluginOrInstaller::Installer(installer));
            } else if self.php_runtime_class_exists(class.as_bstr(), true)? {
                if !self.php_runtime_is_a(
                    class.as_bstr(),
                    BStr::new("Composer\\Plugin\\PluginInterface"),
                )? {
                    return Err(RuntimeException::new(bconcat!(
                        "Could not activate plugin \"",
                        package.get_name(),
                        "\" as \"",
                        class,
                        "\" does not implement Composer\\Plugin\\PluginInterface"
                    ))
                    .into());
                }
                let handle = self.php_runtime_new_object(class.as_bstr())?;
                let plugin: std::rc::Rc<std::cell::RefCell<dyn PluginInterface>> =
                    std::rc::Rc::new(std::cell::RefCell::new(PhpPluginProxy::new(
                        handle.phandle,
                        handle.class,
                        handle.implements,
                    )));
                self.add_plugin(plugin.clone(), is_global_plugin, Some(package.clone()))?;
                self.registered_plugins
                    .entry(package.get_name())
                    .or_default()
                    .push(PluginOrInstaller::Plugin(plugin));
            } else if fail_on_missing_classes {
                return Err(UnexpectedValueException::new(bconcat!(
                    "Plugin ",
                    package.get_name(),
                    " could not be initialized, class not found: ",
                    class
                ))
                .into());
            }
        }
        Ok(())
    }

    /// Runs a boolean runtime query in the PHP worker with the plugin dispatcher active.
    fn php_runtime_bool(&self, function: &BStr, args: Vec<PluginValue>) -> anyhow::Result<bool> {
        let value = unwrap_php_result(call_function_with_dispatcher(
            function,
            args,
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        match value {
            PluginValue::Bool(value) => Ok(value),
            other => Err(RuntimeException::new(bconcat!(
                "PHP runtime query `",
                function,
                "` did not return a bool: ",
                display_to_bytes(format_args!("{other:?}"))
            ))
            .into()),
        }
    }

    fn php_runtime_class_exists(&self, class: &BStr, autoload: bool) -> anyhow::Result<bool> {
        self.php_runtime_bool(
            BStr::new("class_exists"),
            vec![PluginValue::string(class), PluginValue::Bool(autoload)],
        )
    }

    fn php_runtime_is_a(&self, class: &BStr, interface: &BStr) -> anyhow::Result<bool> {
        self.php_runtime_bool(
            BStr::new("is_a"),
            vec![
                PluginValue::string(class),
                PluginValue::string(interface),
                PluginValue::Bool(true),
            ],
        )
    }

    fn php_runtime_eval(&self, code: BString) -> anyhow::Result<()> {
        unwrap_php_result(call_function_with_dispatcher(
            "__shirabe_eval",
            vec![PluginValue::string(code)],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        Ok(())
    }

    fn php_runtime_composer_require(
        &self,
        file_identifier: &BStr,
        file: &BStr,
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
    fn php_runtime_new_object(
        &self,
        class: &BStr,
    ) -> anyhow::Result<shirabe_php_rpc::PhpObjHandle> {
        self.php_runtime_new_object_with_args(class, vec![])
    }

    fn php_runtime_new_object_with_args(
        &self,
        class: &BStr,
        args: Vec<PluginValue>,
    ) -> anyhow::Result<shirabe_php_rpc::PhpObjHandle> {
        let value = unwrap_php_result(shirabe_php_rpc::new_object(
            class,
            args,
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        match value {
            PluginValue::PhpHandle(handle) => Ok(handle),
            other => Err(RuntimeException::new(bconcat!(
                "instantiating `",
                class,
                "` did not return a PHP handle: ",
                display_to_bytes(format_args!("{other:?}"))
            ))
            .into()),
        }
    }

    /// Deactivates a plugin package
    pub fn deactivate_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        if !self.registered_plugins.contains_key(&package.get_name()) {
            return Ok(());
        }

        // PHP unsets registeredPlugins only after the loop; a deactivate() throw must leave the
        // entry observable, so removal happens last here too.
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
    fn get_plugin_api_version(&self) -> BString {
        match &self.plugin_api_version_override {
            Some(version) => version.clone(),
            None => plugin_interface::PLUGIN_API_VERSION.to_owned(),
        }
    }

    /// Adds a plugin, activates it and registers it with the event dispatcher
    pub fn add_plugin(
        &mut self,
        plugin: std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
        is_global_plugin: bool,
        source_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        if self.are_plugins_disabled(if is_global_plugin {
            BStr::new("global")
        } else {
            BStr::new("local")
        }) {
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
                    .get(B("plugin-optional"))
                    .map(|v| v.as_bool() == Some(true))
                    .unwrap_or(false);
                if !self.is_plugin_allowed(
                    sp.get_name(),
                    is_global_plugin,
                    plugin_optional,
                    true,
                )? {
                    self.io.write_error3(
                        bconcat!(
                            "Skipped loading \"",
                            plugin.borrow().get_class_name(),
                            " from ",
                            sp.get_name(),
                            "\" ",
                            if is_global_plugin || self.running_in_global_dir {
                                "(installed globally) "
                            } else {
                                ""
                            },
                            " as it is not in config.allow-plugins"
                        ),
                        true,
                        crate::io::DEBUG,
                    );
                    return Ok(());
                }
            }
        }

        let mut details: Vec<BString> = vec![];
        if let Some(sp) = source_package.as_ref() {
            details.push(bconcat!("from ", sp.get_name()));
        }
        if is_global_plugin || self.running_in_global_dir {
            details.push("installed globally".into());
        }
        self.io.write_error3(
            bconcat!(
                "Loading plugin ",
                plugin.borrow().get_class_name(),
                if !details.is_empty() {
                    bconcat!(" (", implode(", ", &details), ")")
                } else {
                    BString::default()
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
            bconcat!("Unloading plugin ", plugin.borrow().get_class_name()),
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
            bconcat!("Uninstalling plugin ", plugin.borrow().get_class_name()),
            true,
            crate::io::DEBUG,
        );
        plugin
            .borrow_mut()
            .uninstall(self.composer_full(), self.io.clone())?;
        Ok(())
    }

    fn load_repository(
        &mut self,
        repo: &RepositoryInterfaceHandle,
        is_global_repo: bool,
        root_package: Option<RootPackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        let packages = repo.get_packages()?;

        let mut weights: IndexMap<BString, i64> = IndexMap::new();
        for package in &packages {
            if package.get_type() == "composer-plugin" {
                let extra = package.get_extra();
                if package.get_name() == "composer/installers"
                    || extra
                        .get(B("plugin-modifies-install-path"))
                        .map(|v| v.as_bool() == Some(true))
                        .unwrap_or(false)
                {
                    weights.insert(package.get_name(), -10000);
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
                && !self.is_plugin_allowed(package.get_name(), false, true, false)?
            {
                self.io.write_error(bconcat!("<warning>The \"", package.get_name(), "\" plugin was not loaded as it is not listed in allow-plugins and is not required by the root package anymore.</warning>"));
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
        mut collected: IndexMap<BString, PackageInterfaceHandle>,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<IndexMap<BString, PackageInterfaceHandle>> {
        // TODO(plugin): used by registerPackage to assemble plugin dependency autoload map
        for (_k, require_link) in package.get_requires().iter() {
            for required_package in installed_repo.find_packages_with_replacers_and_providers(
                bstr::BStr::new(require_link.get_target()),
                None,
            )? {
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
    ) -> Option<std::path::PathBuf> {
        if !global {
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
        capability: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        let capable = match plugin.as_capable() {
            Some(c) => c,
            None => return Ok(None),
        };

        let capabilities = capable.get_capabilities()?;

        // PHP: !empty($capabilities[$capability]) && is_string($capabilities[$capability]) && trim($capabilities[$capability])
        if let Some(value) = capabilities.get(B(capability))
            && !empty(value)
            && let PhpMixed::String(s) = value
        {
            let trimmed = trim2(s, " \t\n\r\0\u{0B}");
            // PHP evaluates trim(...) in boolean context: "" and "0" are falsy.
            if !trimmed.is_empty() && trimmed != "0" {
                return Ok(Some(trimmed));
            }
        }

        // PHP: array_key_exists($capability, $capabilities) && (empty(...) || !is_string(...)
        // || !trim(...)). Once the first branch has declined, a present key always fails one
        // of the three disjuncts, so a present key unconditionally throws here.
        if let Some(value) = capabilities.get(B(capability)) {
            return Err(UnexpectedValueException::new(bconcat!(
                "Plugin ",
                plugin.get_class_name(),
                " provided invalid capability class name(s), got ",
                var_export(value, true)
            ))
            .into());
        }

        Ok(None)
    }

    pub fn get_plugin_capability(
        &self,
        plugin: &std::rc::Rc<std::cell::RefCell<dyn PluginInterface>>,
        capability_class_name: &BStr,
        ctor_args: IndexMap<BString, PluginValue>,
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
            return Err(RuntimeException::new(bconcat!(
                "Cannot instantiate Capability, as class ",
                capability_class,
                " from plugin ",
                plugin.borrow().get_class_name(),
                " does not exist."
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
        ctor_args.insert(BString::from("plugin"), plugin_value);
        let args_value = PluginValue::Array(ctor_args.into_iter().collect());

        // PHP: $capabilityObj = new $capabilityClass($ctorArgs);
        let capability_obj = unwrap_php_result(shirabe_php_rpc::new_object(
            capability_class.as_bstr(),
            vec![args_value],
            Some(&mut PluginRpcDispatcher::default()),
        ))?;
        let handle = match capability_obj {
            PluginValue::PhpHandle(handle) => handle,
            other => {
                return Err(RuntimeException::new(bconcat!(
                    "worker returned a non-object for new ",
                    capability_class,
                    "(...): ",
                    display_to_bytes(format_args!("{other:?}"))
                ))
                .into());
            }
        };

        // PHP: if (!$capabilityObj instanceof Capability || !$capabilityObj instanceof $capabilityClassName)
        if !php_is_a(
            &handle,
            BStr::new("Composer\\Plugin\\Capability\\Capability"),
        )? || !php_is_a(&handle, capability_class_name)?
        {
            return Err(RuntimeException::new(bconcat!(
                "Class ",
                capability_class,
                " must implement both Composer\\Plugin\\Capability\\Capability and ",
                capability_class_name,
                "."
            ))
            .into());
        }

        match capability_class_name.as_bytes() {
            b"Composer\\Plugin\\Capability\\CommandProvider" => {
                Ok(Some(Box::new(PhpCommandProviderProxy::new(handle))))
            }
            b"Composer\\Plugin\\Capability\\Capability" => {
                Ok(Some(Box::new(PhpCapabilityProxy::new(handle))))
            }
            // A capability interface outside composer-plugin-api that the instanceof checks
            // accepted (the plugin ships its own): no Rust adapter exists for its methods.
            other => Err(RuntimeException::new(bconcat!(
                "capability ",
                other,
                " is recognized but not yet adapted"
            ))
            .into()),
        }
    }

    pub fn get_plugin_capabilities(
        &self,
        capability_class_name: &BStr,
        ctor_args: IndexMap<BString, PluginValue>,
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
    ) -> Option<IndexMap<BString, bool>> {
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
            let mut m: IndexMap<BString, bool> = IndexMap::new();
            m.insert(BString::from("{}"), true);
            return Some(m);
        }

        if allow_plugins_config.as_bool() == Some(false) {
            let mut m: IndexMap<BString, bool> = IndexMap::new();
            m.insert(BString::from("{}"), false);
            return Some(m);
        }

        let mut rules: IndexMap<BString, bool> = IndexMap::new();
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

    pub fn are_plugins_disabled(&self, r#type: &BStr) -> bool {
        matches!(
            (&self.disable_plugins, r#type.as_bytes()),
            (DisablePlugins::All, _)
                | (DisablePlugins::Local, b"local")
                | (DisablePlugins::Global, b"global")
        )
    }

    pub fn disable_plugins(&mut self) {
        self.disable_plugins = DisablePlugins::All;
    }

    pub fn is_plugin_allowed(
        &mut self,
        package: impl AsRef<[u8]>,
        is_global_plugin: bool,
        optional: bool,
        prompt: bool,
    ) -> anyhow::Result<bool> {
        let package = BStr::new(package.as_ref());
        // TODO(plugin): allow-plugins authorization flow with interactive prompt
        let rules: &mut Option<IndexMap<BString, bool>> = if is_global_plugin {
            &mut self.allow_global_plugin_rules
        } else {
            &mut self.allow_plugin_rules
        };

        // This is a BC mode for lock files created pre-Composer-2.2 where the expectation of
        // an allow-plugins config being present cannot be made.
        if rules.is_none() {
            if !self.io.is_interactive() {
                self.io.write_error("<warning>For additional security you should declare the allow-plugins config with a list of packages names that are allowed to run code. See https://getcomposer.org/allow-plugins</warning>");
                self.io.write_error("<warning>This warning will become an exception once you run shirabe update!</warning>");

                let mut m: IndexMap<BString, bool> = IndexMap::new();
                m.insert(BString::from("{}"), true);
                *rules = Some(m);

                // if no config is defined we allow all plugins for BC
                return Ok(true);
            }

            // keep going and prompt the user
            *rules = Some(IndexMap::new());
        }

        let rules_snapshot: Vec<(BString, bool)> = rules
            .as_ref()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        for (pattern, allow) in &rules_snapshot {
            if preg_is_match(pattern, package) {
                return Ok(*allow);
            }
        }

        if package == "composer/package-versions-deprecated" {
            return Ok(false);
        }

        if self.io.is_interactive() && prompt {
            // TODO(plugin): interactive consent flow — preserved as a stub. PHP picks
            // $this->globalComposer's config when is_global_plugin; the stub uses the
            // local composer's config in both cases.
            let config = self
                .composer
                .upgrade()
                .expect("PluginManager must not outlive Composer")
                .borrow()
                .get_config();

            self.io.write_error(bconcat!("<warning>", package, if is_global_plugin || self.running_in_global_dir { " (installed globally)" } else { "" }, " contains a Composer plugin which is currently not in your allow-plugins config. See https://getcomposer.org/allow-plugins</warning>"));
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
                    bconcat!("Do you trust \"<fg=green;options=bold>", package, "</>\" to execute code and wish to enable it now? (writes \"allow-plugins\" to composer.json) [<comment>y,n,d,?</comment>] "),
                    PhpMixed::string(default),
                )?;
                let answer_str: &[u8] = answer.as_string().unwrap_or_default();
                match answer_str {
                    b"y" | b"n" | b"d" => {
                        let allow = answer_str == b"y";

                        // persist answer in current rules to avoid prompting again if the package gets reloaded
                        rules
                            .as_mut()
                            .unwrap()
                            .insert(base_package::package_name_to_regexp(package), allow);

                        // persist answer in composer.json if it wasn't simply discarded
                        if answer_str == b"y" || answer_str == b"n" {
                            let allow_plugins_value = config.borrow_mut().get("allow-plugins");
                            if let Some(arr) = allow_plugins_value.as_array() {
                                let mut allow_plugins = arr.clone();
                                allow_plugins.insert(BString::from(package), PhpMixed::Bool(allow));
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
                                        bstr::BStr::new("allow-plugins"),
                                        PhpMixed::Array(allow_plugins.clone()),
                                    )?;
                                let mut inner = IndexMap::new();
                                inner
                                    .insert("allow-plugins".into(), PhpMixed::Array(allow_plugins));
                                let mut config_section = IndexMap::new();
                                config_section.insert("config".into(), PhpMixed::Array(inner));
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
                            BString::from(
                                "y - add package to allow-plugins in composer.json and let it run immediately",
                            ),
                            BString::from(
                                "n - add package (as disallowed) to allow-plugins in composer.json to suppress further prompts",
                            ),
                            BString::from(
                                "d - discard this, do not change composer.json and do not allow the plugin to run",
                            ),
                            BString::from("? - print help"),
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

        Err(PluginBlockedException::new(bconcat!(package, if is_global_plugin || self.running_in_global_dir { " (installed globally)" } else { "" }, " contains a Composer plugin which is blocked by your allow-plugins config. You may add it to the list if you consider it safe.\nYou can run \"shirabe ", if is_global_plugin || self.running_in_global_dir { "global " } else { "" }, "config --no-plugins allow-plugins.", package, " [true|false]\" to enable it (true) or disable it explicitly and suppress this exception (false)\nSee https://getcomposer.org/allow-plugins")).into())
    }
}
