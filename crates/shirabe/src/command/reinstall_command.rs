//! ref: composer/src/Composer/Command/ReinstallCommand.php

use std::any::Any;

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::InvalidArgumentException;

use crate::command::base_command::BaseCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::dependency_resolver::operation::uninstall_operation::UninstallOperation;
use crate::dependency_resolver::transaction::Transaction;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::BasePackage;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::script::script_events::ScriptEvents;
use crate::util::platform::Platform;

#[derive(Debug)]
pub struct ReinstallCommand {
    inner: BaseCommand,
}

impl CompletionTrait for ReinstallCommand {}

impl ReinstallCommand {
    pub fn configure(&mut self) {
        let suggest_prefer_install = self.suggest_prefer_install();
        let suggest_installed_package_types = self.suggest_installed_package_types(false);
        let suggest_installed_package = self.suggest_installed_package(false);
        self.inner
            .set_name("reinstall")
            .set_description("Uninstalls and reinstalls the given package names")
            .set_definition(vec![
                InputOption::new("prefer-source", None, Some(InputOption::VALUE_NONE), "Forces installation from package sources when possible, including VCS information.", None, vec![]),
                InputOption::new("prefer-dist", None, Some(InputOption::VALUE_NONE), "Forces installation from package dist (default behavior).", None, vec![]),
                InputOption::new("prefer-install", None, Some(InputOption::VALUE_REQUIRED), "Forces installation from package dist|source|auto (auto chooses source for dev versions, dist for the rest).", None, suggest_prefer_install),
                InputOption::new("no-autoloader", None, Some(InputOption::VALUE_NONE), "Skips autoloader generation", None, vec![]),
                InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None, vec![]),
                InputOption::new("optimize-autoloader", Some(shirabe_php_shim::PhpMixed::String("o".to_string())), Some(InputOption::VALUE_NONE), "Optimize autoloader during autoloader dump", None, vec![]),
                InputOption::new("classmap-authoritative", Some(shirabe_php_shim::PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Autoload classes from the classmap only. Implicitly enables `--optimize-autoloader`.", None, vec![]),
                InputOption::new("apcu-autoloader", None, Some(InputOption::VALUE_NONE), "Use APCu to cache found/not-found classes.", None, vec![]),
                InputOption::new("apcu-autoloader-prefix", None, Some(InputOption::VALUE_REQUIRED), "Use a custom prefix for the APCu autoloader cache. Implicitly enables --apcu-autoloader", None, vec![]),
                InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None, vec![]),
                InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None, vec![]),
                InputOption::new("type", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Filter packages to reinstall by type(s)", None, suggest_installed_package_types),
                InputArgument::new("packages", Some(InputArgument::IS_ARRAY), "List of package names to reinstall, can include a wildcard (*) to match any substring.", None, suggest_installed_package),
            ])
            .set_help(
                "The <info>reinstall</info> command looks up installed packages by name,\n\
                uninstalls them and reinstalls them. This lets you do a clean install\n\
                of a package if you messed with its files, or if you wish to change\n\
                the installation type using --prefer-install.\n\n\
                <info>php composer.phar reinstall acme/foo \"acme/bar-*\"</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#reinstall"
            );
    }

    pub fn execute(&self, input: &dyn InputInterface, output: &dyn OutputInterface) -> Result<i64> {
        let io = self.inner.get_io();

        let composer = self.inner.require_composer()?;

        let local_repo = composer.get_repository_manager().get_local_repository();
        let mut packages_to_reinstall: Vec<Box<dyn crate::package::package_interface::PackageInterface>> = vec![];
        let mut package_names_to_reinstall: Vec<String> = vec![];

        let type_option = input.get_option("type");
        let type_count = type_option.as_list().map_or(0, |l| l.len());
        let packages_arg = input.get_argument("packages");
        let packages_count = packages_arg.as_list().map_or(0, |l| l.len());

        if type_count > 0 {
            if packages_count > 0 {
                return Err(InvalidArgumentException {
                    message: "You cannot specify package names and filter by type at the same time.".to_string(),
                    code: 0,
                }.into());
            }
            let filter_types: Vec<String> = type_option.as_list()
                .map(|l| l.iter().filter_map(|v| v.as_string().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            for package in local_repo.get_canonical_packages() {
                if filter_types.contains(&package.get_type().to_string()) {
                    package_names_to_reinstall.push(package.get_name().to_string());
                    packages_to_reinstall.push(package);
                }
            }
        } else {
            if packages_count == 0 {
                return Err(InvalidArgumentException {
                    message: "You must pass one or more package names to be reinstalled.".to_string(),
                    code: 0,
                }.into());
            }
            let patterns: Vec<String> = packages_arg.as_list()
                .map(|l| l.iter().filter_map(|v| v.as_string().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            for pattern in &patterns {
                let pattern_regexp = BasePackage::package_name_to_regexp(pattern);
                let mut matched = false;
                for package in local_repo.get_canonical_packages() {
                    if Preg::is_match(&pattern_regexp, package.get_name()).unwrap_or(false) {
                        matched = true;
                        package_names_to_reinstall.push(package.get_name().to_string());
                        packages_to_reinstall.push(package);
                    }
                }
                if !matched {
                    io.write_error(&format!("<warning>Pattern \"{}\" does not match any currently installed packages.</warning>", pattern));
                }
            }
        }

        if packages_to_reinstall.is_empty() {
            io.write_error("<warning>Found no packages to reinstall, aborting.</warning>");
            return Ok(1);
        }

        let mut uninstall_operations: Vec<UninstallOperation> = vec![];
        for package in packages_to_reinstall {
            uninstall_operations.push(UninstallOperation::new(package));
        }

        let present_packages = local_repo.get_packages();
        let result_packages = present_packages.clone();
        let present_packages: Vec<_> = present_packages.into_iter()
            .filter(|package| !package_names_to_reinstall.contains(&package.get_name().to_string()))
            .collect();

        let transaction = Transaction::new(present_packages, result_packages);
        let install_operations = transaction.get_operations();

        let mut install_order = indexmap::IndexMap::new();
        for (index, op) in install_operations.iter().enumerate() {
            if let Some(install_op) = (op.as_any() as &dyn Any).downcast_ref::<InstallOperation>() {
                if (install_op.get_package().as_any() as &dyn Any).downcast_ref::<AliasPackage>().is_none() {
                    install_order.insert(install_op.get_package().get_name().to_string(), index);
                }
            }
        }

        uninstall_operations.sort_by(|a, b| {
            let a_order = install_order.get(a.get_package().get_name()).copied().unwrap_or(0);
            let b_order = install_order.get(b.get_package().get_name()).copied().unwrap_or(0);
            b_order.cmp(&a_order)
        });

        // TODO(plugin): dispatch CommandEvent
        let command_event = CommandEvent::new(
            PluginEvents::COMMAND.to_string(),
            "reinstall".to_string(),
            Box::new(input),
            Box::new(output),
            vec![],
            vec![],
        );
        let event_dispatcher = composer.get_event_dispatcher();
        event_dispatcher.dispatch(command_event.get_name(), &command_event);

        let config = composer.get_config();
        let (prefer_source, prefer_dist) = self.inner.get_preferred_install_options(config, input)?;

        let installation_manager = composer.get_installation_manager();
        let download_manager = composer.get_download_manager();
        let package = composer.get_package();

        installation_manager.set_output_progress(!input.get_option("no-progress").as_bool().unwrap_or(false));
        if input.get_option("no-plugins").as_bool().unwrap_or(false) {
            installation_manager.disable_plugins();
        }

        download_manager.set_prefer_source(prefer_source);
        download_manager.set_prefer_dist(prefer_dist);

        let dev_mode = local_repo.get_dev_mode().unwrap_or(true);

        Platform::put_env("COMPOSER_DEV_MODE", if dev_mode { "1" } else { "0" });
        event_dispatcher.dispatch_script(ScriptEvents::PRE_INSTALL_CMD, dev_mode);

        installation_manager.execute(local_repo, uninstall_operations, dev_mode);
        installation_manager.execute(local_repo, install_operations, dev_mode);

        if !input.get_option("no-autoloader").as_bool().unwrap_or(false) {
            let optimize = input.get_option("optimize-autoloader").as_bool().unwrap_or(false)
                || config.get("optimize-autoloader").as_bool().unwrap_or(false);
            let authoritative = input.get_option("classmap-authoritative").as_bool().unwrap_or(false)
                || config.get("classmap-authoritative").as_bool().unwrap_or(false);
            let apcu_prefix = input.get_option("apcu-autoloader-prefix").as_string_opt().map(|s| s.to_string());
            let apcu = apcu_prefix.is_some()
                || input.get_option("apcu-autoloader").as_bool().unwrap_or(false)
                || config.get("apcu-autoloader").as_bool().unwrap_or(false);

            let generator = composer.get_autoload_generator();
            generator.set_class_map_authoritative(authoritative);
            generator.set_apcu(apcu, apcu_prefix.as_deref());
            generator.set_platform_requirement_filter(self.inner.get_platform_requirement_filter(input)?);
            generator.dump(
                config,
                local_repo,
                package,
                installation_manager,
                "composer",
                optimize,
                None,
                composer.get_locker(),
            );
        }

        event_dispatcher.dispatch_script(ScriptEvents::POST_INSTALL_CMD, dev_mode);

        Ok(0)
    }
}
