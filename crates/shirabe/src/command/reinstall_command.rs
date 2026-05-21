//! ref: composer/src/Composer/Command/ReinstallCommand.php

use std::any::Any;

use anyhow::Result;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::InvalidArgumentException;

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputDefinitionItem;
use crate::console::input::InputOption;
use crate::dependency_resolver::Transaction;
use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::io::IOInterface;
use crate::package::AliasPackage;
use crate::package::BasePackage;
use crate::package::PackageInterface;
use crate::package::base_package;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::script::ScriptEvents;
use crate::util::Platform;

#[derive(Debug)]
pub struct ReinstallCommand {
    base_command_data: BaseCommandData,
}

impl ReinstallCommand {
    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_prefer_install / suggest_installed_package_types / suggest_installed_package
        self
            .set_name("reinstall")
            .set_description("Uninstalls and reinstalls the given package names")
            .set_definition(&[
                InputOption::new("prefer-source", None, Some(InputOption::VALUE_NONE), "Forces installation from package sources when possible, including VCS information.", None).unwrap().into(),
                InputOption::new("prefer-dist", None, Some(InputOption::VALUE_NONE), "Forces installation from package dist (default behavior).", None).unwrap().into(),
                InputOption::new("prefer-install", None, Some(InputOption::VALUE_REQUIRED), "Forces installation from package dist|source|auto (auto chooses source for dev versions, dist for the rest).", None).unwrap().into(),
                InputOption::new("no-autoloader", None, Some(InputOption::VALUE_NONE), "Skips autoloader generation", None).unwrap().into(),
                InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None).unwrap().into(),
                InputOption::new("optimize-autoloader", Some(shirabe_php_shim::PhpMixed::String("o".to_string())), Some(InputOption::VALUE_NONE), "Optimize autoloader during autoloader dump", None).unwrap().into(),
                InputOption::new("classmap-authoritative", Some(shirabe_php_shim::PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Autoload classes from the classmap only. Implicitly enables `--optimize-autoloader`.", None).unwrap().into(),
                InputOption::new("apcu-autoloader", None, Some(InputOption::VALUE_NONE), "Use APCu to cache found/not-found classes.", None).unwrap().into(),
                InputOption::new("apcu-autoloader-prefix", None, Some(InputOption::VALUE_REQUIRED), "Use a custom prefix for the APCu autoloader cache. Implicitly enables --apcu-autoloader", None).unwrap().into(),
                InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None).unwrap().into(),
                InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None).unwrap().into(),
                InputOption::new("type", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Filter packages to reinstall by type(s)", None).unwrap().into(),
                InputArgument::new("packages", Some(InputArgument::IS_ARRAY), "List of package names to reinstall, can include a wildcard (*) to match any substring.", None).unwrap().into(),
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

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> Result<i64> {
        let composer = self.require_composer(None, None)?;
        let composer = crate::command::composer_full(&composer);
        let io = self.get_io();

        let repository_manager = composer.get_repository_manager().clone();
        let repository_manager = repository_manager.borrow();
        let local_repo = repository_manager.get_local_repository();
        let mut packages_to_reinstall: Vec<Box<dyn crate::package::PackageInterface>> = vec![];
        let mut package_names_to_reinstall: Vec<String> = vec![];

        let type_option = input.get_option("type");
        let type_count = type_option.as_list().map_or(0, |l| l.len());
        let packages_arg = input.get_argument("packages");
        let packages_count = packages_arg.as_list().map_or(0, |l| l.len());

        if type_count > 0 {
            if packages_count > 0 {
                return Err(InvalidArgumentException {
                    message:
                        "You cannot specify package names and filter by type at the same time."
                            .to_string(),
                    code: 0,
                }
                .into());
            }
            let filter_types: Vec<String> = type_option
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
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
                    message: "You must pass one or more package names to be reinstalled."
                        .to_string(),
                    code: 0,
                }
                .into());
            }
            let patterns: Vec<String> = packages_arg
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            for pattern in &patterns {
                let pattern_regexp = base_package::package_name_to_regexp(pattern);
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
        let result_packages: Vec<Box<dyn PackageInterface>> = present_packages
            .iter()
            .map(|p| p.clone_package_box())
            .collect();
        let present_packages: Vec<Box<dyn PackageInterface>> = present_packages
            .into_iter()
            .filter(|package| !package_names_to_reinstall.contains(&package.get_name().to_string()))
            .map(|p| p.clone_package_box())
            .collect();

        let transaction = Transaction::new(present_packages, result_packages);
        let install_operations = transaction.get_operations();

        let mut install_order = indexmap::IndexMap::new();
        for (index, op) in install_operations.iter().enumerate() {
            if let Some(install_op) = op.as_any().downcast_ref::<InstallOperation>() {
                if install_op
                    .get_package()
                    .as_any()
                    .downcast_ref::<AliasPackage>()
                    .is_none()
                {
                    install_order.insert(install_op.get_package().get_name().to_string(), index);
                }
            }
        }

        uninstall_operations.sort_by(|a, b| {
            let a_order = install_order
                .get(a.get_package().get_name())
                .copied()
                .unwrap_or(0);
            let b_order = install_order
                .get(b.get_package().get_name())
                .copied()
                .unwrap_or(0);
            b_order.cmp(&a_order)
        });

        // TODO(plugin): dispatch CommandEvent
        let command_event = CommandEvent::new(PluginEvents::COMMAND, "reinstall", input, output);
        let event_dispatcher = composer.get_event_dispatcher();
        event_dispatcher
            .borrow_mut()
            .dispatch(Some(command_event.get_name()), None);

        let config = composer.get_config();
        let (prefer_source, prefer_dist) =
            self.get_preferred_install_options(&*config.borrow(), input, false)?;

        let installation_manager = composer.get_installation_manager().clone();
        let download_manager = composer.get_download_manager();
        let package = composer.get_package();

        // TODO(phase-b): InstallationManager setters need &mut self; conflicts with the &installation_manager / &local_repo / &package borrows held below; needs shared-ownership refactor
        let _no_progress = !input.get_option("no-progress").as_bool().unwrap_or(false);
        let _no_plugins = input.get_option("no-plugins").as_bool().unwrap_or(false);

        download_manager
            .borrow_mut()
            .set_prefer_source(prefer_source);
        download_manager.borrow_mut().set_prefer_dist(prefer_dist);

        let dev_mode = local_repo.get_dev_mode().unwrap_or(true);

        Platform::put_env("COMPOSER_DEV_MODE", if dev_mode { "1" } else { "0" });
        event_dispatcher.borrow_mut().dispatch_script(
            ScriptEvents::PRE_INSTALL_CMD,
            dev_mode,
            vec![],
            indexmap::IndexMap::new(),
        );

        // TODO(phase-b): InstallationManager::execute needs `&mut dyn InstalledRepositoryInterface`;
        // local_repo is borrowed shared from RepositoryManager. Needs Rc<RefCell<dyn ...>> migration.
        let _ = (
            uninstall_operations,
            install_operations,
            dev_mode,
            local_repo,
            &installation_manager,
        );
        // installation_manager.execute(local_repo_mut, uninstall_ops_boxed, dev_mode, true, false);
        // installation_manager.execute(local_repo_mut, install_ops_boxed, dev_mode, true, false);

        if !input.get_option("no-autoloader").as_bool().unwrap_or(false) {
            let optimize = input
                .get_option("optimize-autoloader")
                .as_bool()
                .unwrap_or(false)
                || config
                    .borrow_mut()
                    .get("optimize-autoloader")
                    .as_bool()
                    .unwrap_or(false);
            let authoritative = input
                .get_option("classmap-authoritative")
                .as_bool()
                .unwrap_or(false)
                || config
                    .borrow_mut()
                    .get("classmap-authoritative")
                    .as_bool()
                    .unwrap_or(false);
            let apcu_prefix = input
                .get_option("apcu-autoloader-prefix")
                .as_string_opt()
                .map(|s| s.to_string());
            let apcu = apcu_prefix.is_some()
                || input
                    .get_option("apcu-autoloader")
                    .as_bool()
                    .unwrap_or(false)
                || config
                    .borrow_mut()
                    .get("apcu-autoloader")
                    .as_bool()
                    .unwrap_or(false);

            // TODO(phase-b): AutoloadGenerator setters/dump need &mut self; conflicts with concurrent borrows of composer subsystems; needs shared-ownership refactor
            let _ = (
                authoritative,
                apcu,
                apcu_prefix.clone(),
                self.get_platform_requirement_filter(input)?,
                optimize,
                &*config.borrow(),
                local_repo,
                package,
                installation_manager,
            );
            // composer.get_autoload_generator_mut().set_class_map_authoritative(authoritative);
            // composer.get_autoload_generator_mut().set_apcu(apcu, apcu_prefix.clone());
            // composer.get_autoload_generator_mut().set_platform_requirement_filter(...);
            // composer.get_autoload_generator_mut().dump(...);
        }

        event_dispatcher.borrow_mut().dispatch_script(
            ScriptEvents::POST_INSTALL_CMD,
            dev_mode,
            vec![],
            indexmap::IndexMap::new(),
        );

        Ok(0)
    }
}

impl HasBaseCommandData for ReinstallCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
