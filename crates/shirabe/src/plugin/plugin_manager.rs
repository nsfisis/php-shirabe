//! ref: composer/src/Composer/Plugin/PluginManager.php
//!
//! TODO(plugin): the entire plugin manager subsystem is part of the Plugin API
//! and is not implemented in Phase A. The structure is mirrored verbatim so
//! future plugin support can fill in the runtime hooks.

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    E_USER_DEPRECATED, PhpMixed, RuntimeException, UnexpectedValueException, array_key_exists,
    array_reverse, array_search, clone, get_class, implode, in_array, is_a, is_array, is_string,
    ksort, preg_quote, str_replace, strrpos, strtr, substr, trigger_error, trim, var_export,
    version_compare,
};
use shirabe_semver::constraint::constraint::Constraint;

use crate::composer::Composer;
use crate::event_dispatcher::event_subscriber_interface::EventSubscriberInterface;
use crate::installer::installer_interface::InstallerInterface;
use crate::io::io_interface::IOInterface;
use crate::package::base_package::BasePackage;
use crate::package::complete_package::CompletePackage;
use crate::package::link::Link;
use crate::package::locker::Locker;
use crate::package::package_interface::PackageInterface;
use crate::package::root_package_interface::RootPackageInterface;
use crate::package::version::version_parser::VersionParser;
use crate::partial_composer::PartialComposer;
use crate::plugin::capability::capability::Capability;
use crate::plugin::capable::Capable;
use crate::plugin::plugin_blocked_exception::PluginBlockedException;
use crate::plugin::plugin_interface::PluginInterface;
use crate::repository::installed_repository::InstalledRepository;
use crate::repository::repository_interface::RepositoryInterface;
use crate::repository::repository_utils::RepositoryUtils;
use crate::repository::root_package_repository::RootPackageRepository;
use crate::util::package_sorter::PackageSorter;

/// Marker for the disablePlugins variant: false | "local" | "global" | true.
#[derive(Debug, Clone, PartialEq)]
pub enum DisablePlugins {
    False,
    Local,
    Global,
    True,
}

#[derive(Debug)]
pub struct PluginManager {
    pub(crate) composer: Composer,
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) global_composer: Option<PartialComposer>,
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
        io: Box<dyn IOInterface>,
        composer: Composer,
        global_composer: Option<PartialComposer>,
        disable_plugins: DisablePlugins,
    ) -> Self {
        let allow_plugin_rules = Self::parse_allowed_plugins(
            composer.get_config().get("allow-plugins").clone(),
            Some(composer.get_locker()),
        );
        let allow_global_plugin_rules = Self::parse_allowed_plugins(
            global_composer
                .as_ref()
                .map(|gc| gc.get_config().get("allow-plugins").clone())
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

    /// Loads all plugins from currently installed plugin packages
    pub fn load_installed_plugins(&mut self) -> anyhow::Result<()> {
        // TODO(plugin): plugin loading is part of the plugin API
        if !self.are_plugins_disabled("local") {
            let repo = self
                .composer
                .get_repository_manager()
                .get_local_repository();
            self.load_repository(&*repo, false, Some(self.composer.get_package()))?;
        }

        if self.global_composer.is_some() && !self.are_plugins_disabled("global") {
            let repo = self
                .global_composer
                .as_ref()
                .unwrap()
                .get_repository_manager()
                .get_local_repository();
            self.load_repository(&*repo, true, None)?;
        }
        Ok(())
    }

    /// Deactivate all plugins from currently installed plugin packages
    pub fn deactivate_installed_plugins(&mut self) {
        // TODO(plugin): deactivation is part of the plugin API
        if !self.are_plugins_disabled("local") {
            let repo = self
                .composer
                .get_repository_manager()
                .get_local_repository();
            self.deactivate_repository(&*repo, false);
        }

        if self.global_composer.is_some() && !self.are_plugins_disabled("global") {
            let repo = self
                .global_composer
                .as_ref()
                .unwrap()
                .get_repository_manager()
                .get_local_repository();
            self.deactivate_repository(&*repo, true);
        }
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
    pub fn get_global_composer(&self) -> Option<&PartialComposer> {
        self.global_composer.as_ref()
    }

    /// Register a plugin package, activate it etc.
    pub fn register_package(
        &mut self,
        package: &dyn PackageInterface,
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
            let mut requires_composer: Option<
                Box<dyn shirabe_semver::constraint::constraint_interface::ConstraintInterface>,
            > = None;
            for (_k, link) in &package.get_requires() {
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
            let current_plugin_api_constraint = Constraint::new(
                "==",
                self.version_parser
                    .normalize(&current_plugin_api_version, None)?,
            );

            if requires_composer.get_pretty_string() == self.get_plugin_api_version() {
                self.io.write_error(&format!("<warning>The \"{}\" plugin requires composer-plugin-api {}, this *WILL* break in the future and it should be fixed ASAP (require ^{} instead for example).</warning>", package.get_name(), self.get_plugin_api_version(), self.get_plugin_api_version()));
            } else if !requires_composer.matches(&current_plugin_api_constraint) {
                self.io.write_error(&format!("<warning>The \"{}\" plugin {}was skipped because it requires a Plugin API version (\"{}\") that does not match your Composer installation (\"{}\"). You may need to run composer update with the \"--no-plugins\" option.</warning>",
                    package.get_name(),
                    if is_global_plugin || self.running_in_global_dir { "(installed globally) " } else { "" },
                    requires_composer.get_pretty_string(),
                    current_plugin_api_version
                ));
                return Ok(());
            }

            if package.get_name() == "symfony/flex"
                && Preg::is_match("{^[0-9.]+$}", package.get_version(), None).unwrap_or(false)
                && version_compare(package.get_version(), "1.9.8", "<")
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
        if !self.is_plugin_allowed(package.get_name(), is_global_plugin, plugin_optional, true)? {
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

        if self.registered_plugins.contains_key(package.get_name()) {
            return Ok(());
        }

        let extra = package.get_extra();
        let class_value = extra.get("class");
        if class_value.is_none() || class_value.map(|v| v.is_empty()).unwrap_or(true) {
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
    pub fn deactivate_package(&mut self, package: &dyn PackageInterface) {
        // TODO(plugin): deactivation flow
        if !self.registered_plugins.contains_key(package.get_name()) {
            return;
        }

        let plugins = self
            .registered_plugins
            .shift_remove(package.get_name())
            .unwrap_or_default();
        for plugin in plugins {
            match plugin {
                PluginOrInstaller::Installer(inst) => {
                    self.composer
                        .get_installation_manager()
                        .remove_installer(&*inst);
                }
                PluginOrInstaller::Plugin(p) => {
                    self.remove_plugin(&*p);
                }
            }
        }
    }

    /// Uninstall a plugin package
    pub fn uninstall_package(&mut self, package: &dyn PackageInterface) {
        // TODO(plugin): uninstall flow
        if !self.registered_plugins.contains_key(package.get_name()) {
            return;
        }

        let plugins = self
            .registered_plugins
            .shift_remove(package.get_name())
            .unwrap_or_default();
        for plugin in plugins {
            match plugin {
                PluginOrInstaller::Installer(inst) => {
                    self.composer
                        .get_installation_manager()
                        .remove_installer(&*inst);
                }
                PluginOrInstaller::Plugin(p) => {
                    self.remove_plugin(&*p);
                    self.uninstall_plugin(&*p);
                }
            }
        }
    }

    /// Returns the version of the internal composer-plugin-api package.
    pub(crate) fn get_plugin_api_version(&self) -> String {
        PluginInterface::PLUGIN_API_VERSION.to_string()
    }

    /// Adds a plugin, activates it and registers it with the event dispatcher
    pub fn add_plugin(
        &mut self,
        plugin: Box<dyn PluginInterface>,
        is_global_plugin: bool,
        source_package: Option<&dyn PackageInterface>,
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
            let sp = source_package.unwrap();
            let plugin_optional = sp
                .get_extra()
                .get("plugin-optional")
                .map(|v| v.as_bool() == Some(true))
                .unwrap_or(false);
            if !self.is_plugin_allowed(sp.get_name(), is_global_plugin, plugin_optional, true)? {
                self.io.write_error(&format!(
                    "Skipped loading \"{} from {}\" {} as it is not in config.allow-plugins",
                    get_class(&*plugin),
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
        if let Some(sp) = source_package {
            details.push(format!("from {}", sp.get_name()));
        }
        if is_global_plugin || self.running_in_global_dir {
            details.push("installed globally".to_string());
        }
        self.io.write_error(&format!(
            "Loading plugin {}{}",
            get_class(&*plugin),
            if !details.is_empty() {
                format!(" ({})", implode(", ", &details))
            } else {
                String::new()
            }
        ));
        plugin.activate(&self.composer, &*self.io);

        // TODO(plugin): if plugin is EventSubscriberInterface, hook into the event dispatcher
        let plugin_dyn: &dyn PluginInterface = &*plugin;
        if let Some(sub) = plugin_dyn.as_event_subscriber_interface() {
            self.composer.get_event_dispatcher().add_subscriber(sub);
        }
        self.plugins.push(plugin);
        Ok(())
    }

    /// Removes a plugin, deactivates it and removes any listener the plugin has set on the plugin instance
    pub fn remove_plugin(&mut self, plugin: &dyn PluginInterface) {
        // TODO(plugin): plugin removal
        let index = array_search(plugin, &self.plugins, true);
        let index = match index {
            Some(i) => i,
            None => return,
        };

        self.io
            .write_error(&format!("Unloading plugin {}", get_class(plugin)));
        self.plugins.remove(index as usize);
        plugin.deactivate(&self.composer, &*self.io);

        self.composer.get_event_dispatcher().remove_listener(plugin);
    }

    /// Notifies a plugin it is being uninstalled and should clean up
    pub fn uninstall_plugin(&self, plugin: &dyn PluginInterface) {
        // TODO(plugin): plugin uninstall hook
        self.io
            .write_error(&format!("Uninstalling plugin {}", get_class(plugin)));
        plugin.uninstall(&self.composer, &*self.io);
    }

    fn load_repository(
        &mut self,
        repo: &dyn RepositoryInterface,
        is_global_repo: bool,
        root_package: Option<&dyn RootPackageInterface>,
    ) -> anyhow::Result<()> {
        // TODO(plugin): repository scan for plugin packages
        let packages = repo.get_packages();

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

        let sorted_packages =
            PackageSorter::sort_packages(packages.iter().map(|p| p.clone_box()).collect(), weights);
        let required_packages: Vec<Box<dyn PackageInterface>> = if !is_global_repo {
            RepositoryUtils::filter_required_packages(
                packages.iter().map(|p| p.as_ref()).collect(),
                root_package.unwrap(),
                true,
            )
            .iter()
            .map(|p| p.clone_box())
            .collect()
        } else {
            vec![]
        };

        for package in &sorted_packages {
            let cp = match package.as_complete_package() {
                Some(cp) => cp,
                None => continue,
            };

            if !in_array(
                package.get_type(),
                &vec![
                    "composer-plugin".to_string(),
                    "composer-installer".to_string(),
                ],
                true,
            ) {
                continue;
            }

            if !is_global_repo
                && !in_array(
                    &**package as &dyn PackageInterface,
                    &required_packages.iter().map(|p| &**p).collect::<Vec<_>>(),
                    true,
                )
                && !self.is_plugin_allowed(package.get_name(), false, true, false)?
            {
                self.io.write_error(&format!("<warning>The \"{}\" plugin was not loaded as it is not listed in allow-plugins and is not required by the root package anymore.</warning>", package.get_name()));
                continue;
            }

            if "composer-plugin" == package.get_type() {
                self.register_package(&**package, false, is_global_repo)?;
            // Backward compatibility
            } else if "composer-installer" == package.get_type() {
                self.register_package(&**package, false, is_global_repo)?;
            }
            let _ = cp;
        }
        Ok(())
    }

    fn deactivate_repository(&mut self, repo: &dyn RepositoryInterface, _is_global_repo: bool) {
        // TODO(plugin): deactivate plugins from a repository
        let packages = repo.get_packages();
        let sorted_packages = array_reverse(PackageSorter::sort_packages(
            packages.iter().map(|p| p.clone_box()).collect(),
            IndexMap::new(),
        ));

        for package in &sorted_packages {
            if package.as_complete_package().is_none() {
                continue;
            }
            if "composer-plugin" == package.get_type() {
                self.deactivate_package(&**package);
            // Backward compatibility
            } else if "composer-installer" == package.get_type() {
                self.deactivate_package(&**package);
            }
        }
    }

    fn collect_dependencies(
        &self,
        installed_repo: &InstalledRepository,
        mut collected: IndexMap<String, Box<dyn PackageInterface>>,
        package: &dyn PackageInterface,
    ) -> IndexMap<String, Box<dyn PackageInterface>> {
        // TODO(plugin): used by registerPackage to assemble plugin dependency autoload map
        for (_k, require_link) in &package.get_requires() {
            for required_package in
                installed_repo.find_packages_with_replacers_and_providers(require_link.get_target())
            {
                if !collected.contains_key(required_package.get_name()) {
                    collected.insert(
                        required_package.get_name().to_string(),
                        required_package.clone_box(),
                    );
                    collected =
                        self.collect_dependencies(installed_repo, collected, &*required_package);
                }
            }
        }

        collected
    }

    /// Retrieves the path a package is installed to.
    fn get_install_path(&self, package: &dyn PackageInterface, global: bool) -> Option<String> {
        if !global {
            return self
                .composer
                .get_installation_manager()
                .get_install_path(package);
        }

        // PHP: assert(null !== $this->globalComposer);
        self.global_composer
            .as_ref()
            .unwrap()
            .get_installation_manager()
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

        if let Some(cap_value) = capabilities.get(capability) {
            if let Some(s) = cap_value.as_string() {
                if !trim(s, " \t\n\r\0\u{0B}").is_empty() {
                    return Ok(Some(trim(s, " \t\n\r\0\u{0B}")));
                }
            }
        }

        if array_key_exists(capability, &capabilities)
            && (capabilities
                .get(capability)
                .map(|v| v.is_empty())
                .unwrap_or(true)
                || !is_string(capabilities.get(capability).unwrap())
                || trim(
                    capabilities
                        .get(capability)
                        .and_then(|v| v.as_string())
                        .unwrap_or(""),
                    " \t\n\r\0\u{0B}",
                )
                .is_empty())
        {
            return Err(UnexpectedValueException {
                message: format!(
                    "Plugin {} provided invalid capability class name(s), got {}",
                    get_class(plugin),
                    var_export(capabilities.get(capability).unwrap(), true)
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
        locker: Option<&Locker>,
    ) -> Option<IndexMap<String, bool>> {
        // PHP: [] === $allowPluginsConfig && $locker !== null && $locker->isLocked() && version_compare($locker->getPluginApi(), '2.2.0', '<')
        let is_empty_array = allow_plugins_config
            .as_array()
            .map(|a| a.is_empty())
            .unwrap_or(false);
        if is_empty_array
            && locker.is_some()
            && locker.unwrap().is_locked()
            && version_compare(&locker.unwrap().get_plugin_api(), "2.2.0", "<")
        {
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
                    BasePackage::package_name_to_regexp(pattern),
                    allow.as_bool().unwrap_or(false),
                );
            }
        }

        Some(rules)
    }

    pub fn are_plugins_disabled(&self, r#type: &str) -> bool {
        match (&self.disable_plugins, r#type) {
            (DisablePlugins::True, _) => true,
            (DisablePlugins::Local, "local") => true,
            (DisablePlugins::Global, "global") => true,
            _ => false,
        }
    }

    pub fn disable_plugins(&mut self) {
        self.disable_plugins = DisablePlugins::True;
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
            if Preg::is_match(pattern, package, None).unwrap_or(false) {
                return Ok(*allow);
            }
        }

        if package == "composer/package-versions-deprecated" {
            return Ok(false);
        }

        if self.io.is_interactive() && prompt {
            // TODO(plugin): interactive consent flow — preserved as a stub
            let composer_ref: &Composer = if is_global_plugin && self.global_composer.is_some() {
                // PHP allows PartialComposer here; treat as the same dispatch surface in the stub.
                &self.composer
            } else {
                &self.composer
            };

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
                            .insert(BasePackage::package_name_to_regexp(package), allow);

                        // persist answer in composer.json if it wasn't simply discarded
                        if answer_str == "y" || answer_str == "n" {
                            let allow_plugins_value =
                                composer_ref.get_config().get("allow-plugins").clone();
                            if let Some(arr) = allow_plugins_value.as_array() {
                                let mut allow_plugins = arr.clone();
                                allow_plugins
                                    .insert(package.to_string(), Box::new(PhpMixed::Bool(allow)));
                                if composer_ref
                                    .get_config()
                                    .get("sort-packages")
                                    .as_bool()
                                    .unwrap_or(false)
                                {
                                    ksort(&mut allow_plugins);
                                }
                                composer_ref
                                    .get_config()
                                    .get_config_source()
                                    .add_config_setting(
                                        "allow-plugins",
                                        PhpMixed::Array(allow_plugins.clone()),
                                    );
                                let mut wrap: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                                let mut inner: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                                inner.insert(
                                    "allow-plugins".to_string(),
                                    Box::new(PhpMixed::Array(allow_plugins)),
                                );
                                wrap.insert("config".to_string(), Box::new(PhpMixed::Array(inner)));
                                composer_ref.get_config().merge(PhpMixed::Array(wrap), "");
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
