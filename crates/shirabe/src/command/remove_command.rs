//! ref: composer/src/Composer/Command/RemoveCommand.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::exception::invalid_argument_exception::InvalidArgumentException;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{array_map, strtolower, PhpMixed, UnexpectedValueException};

use crate::advisory::auditor::Auditor;
use crate::command::base_command::BaseCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::config::json_config_source::JsonConfigSource;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::dependency_resolver::request::Request;
use crate::factory::Factory;
use crate::installer::Installer;
use crate::json::json_file::JsonFile;
use crate::package::base_package::BasePackage;

#[derive(Debug)]
pub struct RemoveCommand {
    inner: BaseCommand,
}

impl RemoveCommand {
    pub fn configure(&mut self) {
        let suggest_root_requirement = self.suggest_root_requirement();
        self.inner
            .set_name("remove")
            .set_aliases(vec!["rm".to_string(), "uninstall".to_string()])
            .set_description("Removes a package from the require or require-dev")
            .set_definition(vec![
                InputArgument::new(
                    "packages",
                    Some(InputArgument::IS_ARRAY),
                    "Packages that should be removed.",
                    None,
                    suggest_root_requirement,
                ),
                InputOption::new(
                    "dev",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Removes a package from the require-dev section.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "dry-run",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Outputs the operations but will not execute anything (implicitly enables --verbose).",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "no-progress",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Do not output download progress.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "no-update",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Disables the automatic update of the dependencies (implies --no-install).",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "no-install",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Skip the install step after updating the composer.lock file.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "no-audit",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Skip the audit step after updating the composer.lock file (can also be set via the COMPOSER_NO_AUDIT=1 env var).",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "audit-format",
                    None,
                    Some(InputOption::VALUE_REQUIRED),
                    "Audit output format. Must be \"table\", \"plain\", \"json\", or \"summary\".",
                    Some(PhpMixed::String(Auditor::FORMAT_SUMMARY.to_string())),
                    Auditor::FORMATS.to_vec(),
                ),
                InputOption::new(
                    "no-security-blocking",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Allows installing packages with security advisories or that are abandoned (can also be set via the COMPOSER_NO_SECURITY_BLOCKING=1 env var).",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "update-no-dev",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Run the dependency update with the --no-dev option.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "update-with-dependencies",
                    Some(PhpMixed::String("w".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Allows inherited dependencies to be updated with explicit dependencies (can also be set via the COMPOSER_WITH_DEPENDENCIES=1 env var). (Deprecated, is now default behavior)",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "update-with-all-dependencies",
                    Some(PhpMixed::String("W".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Allows all inherited dependencies to be updated, including those that are root requirements (can also be set via the COMPOSER_WITH_ALL_DEPENDENCIES=1 env var).",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "with-all-dependencies",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Alias for --update-with-all-dependencies",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "no-update-with-dependencies",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Does not allow inherited dependencies to be updated with explicit dependencies.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "minimal-changes",
                    Some(PhpMixed::String("m".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "During an update with -w/-W, only perform absolutely necessary changes to transitive dependencies (can also be set via the COMPOSER_MINIMAL_CHANGES=1 env var).",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "unused",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Remove all packages which are locked but not required by any other package.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "ignore-platform-req",
                    None,
                    Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY),
                    "Ignore a specific platform requirement (php & ext- packages).",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "ignore-platform-reqs",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Ignore all platform requirements (php & ext- packages).",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "optimize-autoloader",
                    Some(PhpMixed::String("o".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Optimize autoloader during autoloader dump",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "classmap-authoritative",
                    Some(PhpMixed::String("a".to_string())),
                    Some(InputOption::VALUE_NONE),
                    "Autoload classes from the classmap only. Implicitly enables `--optimize-autoloader`.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "apcu-autoloader",
                    None,
                    Some(InputOption::VALUE_NONE),
                    "Use APCu to cache found/not-found classes.",
                    None,
                    vec![],
                ),
                InputOption::new(
                    "apcu-autoloader-prefix",
                    None,
                    Some(InputOption::VALUE_REQUIRED),
                    "Use a custom prefix for the APCu autoloader cache. Implicitly enables --apcu-autoloader",
                    None,
                    vec![],
                ),
            ])
            .set_help(
                "The <info>remove</info> command removes a package from the current\n\
                list of installed packages\n\n\
                <info>php composer.phar remove</info>\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#remove-rm"
            );
    }

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        if input.get_argument("packages").as_list().map(|l| l.is_empty()).unwrap_or(true)
            && !input.get_option("unused").as_bool().unwrap_or(false)
        {
            return Err(anyhow::anyhow!(InvalidArgumentException {
                message: "Not enough arguments (missing: \"packages\").".to_string(),
                code: 0,
            }));
        }

        let mut packages: Vec<String> = input
            .get_argument("packages")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| strtolower(s)))
                    .collect()
            })
            .unwrap_or_default();

        if input.get_option("unused").as_bool().unwrap_or(false) {
            let composer = self.require_composer()?;
            let locker = composer.get_locker();
            if !locker.is_locked() {
                return Err(anyhow::anyhow!(UnexpectedValueException {
                    message: "A valid composer.lock file is required to run this command with --unused".to_string(),
                    code: 0,
                }));
            }

            let locked_packages = locker.get_locked_repository()?.get_packages();

            let mut required: IndexMap<String, bool> = IndexMap::new();
            for link in composer
                .get_package()
                .get_requires()
                .values()
                .chain(composer.get_package().get_dev_requires().values())
            {
                required.insert(link.get_target().to_string(), true);
            }

            let mut locked_packages = locked_packages;
            loop {
                let mut found = false;
                let mut to_remove = vec![];
                for (index, package) in locked_packages.iter().enumerate() {
                    for name in package.get_names() {
                        if required.contains_key(name.as_str()) {
                            for link in package.get_requires().values() {
                                required.insert(link.get_target().to_string(), true);
                            }
                            found = true;
                            to_remove.push(index);
                            break;
                        }
                    }
                }
                for index in to_remove.into_iter().rev() {
                    locked_packages.remove(index);
                }
                if !found {
                    break;
                }
            }

            let unused: Vec<String> = locked_packages
                .iter()
                .map(|p| p.get_name().to_string())
                .collect();
            packages.extend(unused);

            if packages.is_empty() {
                self.get_io().write_error("<info>No unused packages to remove</info>");
                return Ok(0);
            }
        }

        let file = Factory::get_composer_file();

        let json_file = JsonFile::new(&file, None, None);
        let composer_data = json_file.read()?;
        let composer_backup = std::fs::read_to_string(json_file.get_path())?;

        let json = JsonConfigSource::new(&json_file);

        let r#type = if input.get_option("dev").as_bool().unwrap_or(false) {
            "require-dev"
        } else {
            "require"
        };
        let alt_type = if !input.get_option("dev").as_bool().unwrap_or(false) {
            "require-dev"
        } else {
            "require"
        };
        let io = self.get_io();

        if input
            .get_option("update-with-dependencies")
            .as_bool()
            .unwrap_or(false)
        {
            io.write_error("<warning>You are using the deprecated option \"update-with-dependencies\". This is now default behaviour. The --no-update-with-dependencies option can be used to remove a package without its dependencies.</warning>");
        }

        // make sure name checks are done case insensitively
        let mut composer_data = composer_data;
        for link_type in ["require", "require-dev"] {
            if let Some(section) = composer_data
                .as_array_mut()
                .and_then(|m| m.get_mut(link_type))
                .and_then(|v| v.as_array_mut())
            {
                let entries: Vec<(String, String)> = section
                    .iter()
                    .filter_map(|(k, v)| v.as_string().map(|_| (k.clone(), k.clone())))
                    .collect();
                for (name, canonical) in entries {
                    section.insert(
                        strtolower(&name),
                        Box::new(PhpMixed::String(canonical)),
                    );
                }
            }
        }

        let dry_run = input.get_option("dry-run").as_bool().unwrap_or(false);
        let mut to_remove: IndexMap<String, Vec<String>> = IndexMap::new();
        for package in &packages {
            let in_type = composer_data
                .as_array()
                .and_then(|m| m.get(r#type))
                .and_then(|v| v.as_array())
                .and_then(|m| m.get(package.as_str()))
                .and_then(|v| v.as_string())
                .map(|s| s.to_string());

            let in_alt_type = composer_data
                .as_array()
                .and_then(|m| m.get(alt_type))
                .and_then(|v| v.as_array())
                .and_then(|m| m.get(package.as_str()))
                .and_then(|v| v.as_string())
                .map(|s| s.to_string());

            if let Some(canonical_name) = in_type {
                if dry_run {
                    to_remove
                        .entry(r#type.to_string())
                        .or_default()
                        .push(canonical_name.clone());
                } else {
                    json.remove_link(r#type, &canonical_name);
                }
            } else if let Some(canonical_name) = in_alt_type {
                io.write_error(&format!(
                    "<warning>{} could not be found in {} but it is present in {}</warning>",
                    canonical_name, r#type, alt_type
                ));
                if io.is_interactive() {
                    if io.ask_confirmation(&format!(
                        "Do you want to remove it from {} [<comment>yes</comment>]? ",
                        alt_type
                    ), true) {
                        if dry_run {
                            to_remove
                                .entry(alt_type.to_string())
                                .or_default()
                                .push(canonical_name.clone());
                        } else {
                            json.remove_link(alt_type, &canonical_name);
                        }
                    }
                }
            } else {
                let type_keys: Vec<String> = composer_data
                    .as_array()
                    .and_then(|m| m.get(r#type))
                    .and_then(|v| v.as_array())
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                let matches_in_type = Preg::grep(
                    &BasePackage::package_name_to_regexp(package),
                    &type_keys,
                )
                .unwrap_or_default();

                let alt_type_keys: Vec<String> = composer_data
                    .as_array()
                    .and_then(|m| m.get(alt_type))
                    .and_then(|v| v.as_array())
                    .map(|m| m.keys().cloned().collect())
                    .unwrap_or_default();
                let matches_in_alt_type = Preg::grep(
                    &BasePackage::package_name_to_regexp(package),
                    &alt_type_keys,
                )
                .unwrap_or_default();

                if !type_keys.is_empty() && !matches_in_type.is_empty() {
                    for matched_package in &matches_in_type {
                        if dry_run {
                            to_remove
                                .entry(r#type.to_string())
                                .or_default()
                                .push(matched_package.clone());
                        } else {
                            json.remove_link(r#type, matched_package);
                        }
                    }
                } else if !alt_type_keys.is_empty() && !matches_in_alt_type.is_empty() {
                    for matched_package in &matches_in_alt_type {
                        io.write_error(&format!(
                            "<warning>{} could not be found in {} but it is present in {}</warning>",
                            matched_package, r#type, alt_type
                        ));
                        if io.is_interactive() {
                            if io.ask_confirmation(&format!(
                                "Do you want to remove it from {} [<comment>yes</comment>]? ",
                                alt_type
                            ), true) {
                                if dry_run {
                                    to_remove
                                        .entry(alt_type.to_string())
                                        .or_default()
                                        .push(matched_package.clone());
                                } else {
                                    json.remove_link(alt_type, matched_package);
                                }
                            }
                        }
                    }
                } else {
                    io.write_error(&format!(
                        "<warning>{} is not required in your composer.json and has not been removed</warning>",
                        package
                    ));
                }
            }
        }

        io.write_error(&format!("<info>{} has been updated</info>", file));

        if input.get_option("no-update").as_bool().unwrap_or(false) {
            return Ok(0);
        }

        // TODO(plugin): deactivate installed plugins
        if let Some(composer_opt) = self.try_composer() {
            composer_opt.get_plugin_manager().deactivate_installed_plugins();
        }

        self.reset_composer();
        let composer = self.require_composer()?;

        if dry_run {
            let root_package = composer.get_package();
            let mut links: IndexMap<String, IndexMap<String, _>> = IndexMap::new();
            links.insert("require".to_string(), root_package.get_requires().clone());
            links.insert(
                "require-dev".to_string(),
                root_package.get_dev_requires().clone(),
            );
            for (link_type, names) in &to_remove {
                for name in names {
                    if let Some(section) = links.get_mut(link_type.as_str()) {
                        section.remove(name.as_str());
                    }
                }
            }
            root_package.set_requires(links.remove("require").unwrap_or_default());
            root_package.set_dev_requires(links.remove("require-dev").unwrap_or_default());
        }

        // TODO(plugin): dispatch CommandEvent(PluginEvents::COMMAND, 'remove', input, output)
        let command_event = crate::plugin::command_event::CommandEvent::new(
            crate::plugin::plugin_events::PluginEvents::COMMAND,
            "remove",
            input,
            output,
        );
        composer
            .get_event_dispatcher()
            .dispatch(command_event.get_name(), command_event);

        let allow_plugins = composer.get_config().get("allow-plugins");
        let removed_plugins: Vec<String> = if let Some(allow_map) =
            allow_plugins.as_ref().and_then(|v| v.as_array())
        {
            packages
                .iter()
                .filter(|p| allow_map.contains_key(p.as_str()))
                .cloned()
                .collect()
        } else {
            vec![]
        };

        if !dry_run
            && allow_plugins.as_ref().and_then(|v| v.as_array()).is_some()
            && !removed_plugins.is_empty()
        {
            let allow_map_len = allow_plugins
                .as_ref()
                .and_then(|v| v.as_array())
                .map(|m| m.len())
                .unwrap_or(0);
            if allow_map_len == removed_plugins.len() {
                json.remove_config_setting("allow-plugins");
            } else {
                for plugin in &removed_plugins {
                    json.remove_config_setting(&format!("allow-plugins.{}", plugin));
                }
            }
        }

        composer
            .get_installation_manager()
            .set_output_progress(!input.get_option("no-progress").as_bool().unwrap_or(false));

        let mut install = Installer::create(io, &composer);

        let update_dev_mode = !input.get_option("update-no-dev").as_bool().unwrap_or(false);
        let optimize = input.get_option("optimize-autoloader").as_bool().unwrap_or(false)
            || composer.get_config().get("optimize-autoloader").and_then(|v| v.as_bool()).unwrap_or(false);
        let authoritative = input.get_option("classmap-authoritative").as_bool().unwrap_or(false)
            || composer.get_config().get("classmap-authoritative").and_then(|v| v.as_bool()).unwrap_or(false);
        let apcu_prefix = input.get_option("apcu-autoloader-prefix").as_string().map(|s| s.to_string());
        let apcu = apcu_prefix.is_some()
            || input.get_option("apcu-autoloader").as_bool().unwrap_or(false)
            || composer.get_config().get("apcu-autoloader").and_then(|v| v.as_bool()).unwrap_or(false);
        let minimal_changes = input.get_option("minimal-changes").as_bool().unwrap_or(false)
            || composer.get_config().get("update-with-minimal-changes").and_then(|v| v.as_bool()).unwrap_or(false);

        let mut update_allow_transitive_dependencies =
            Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS_NO_ROOT_REQUIRE;
        let mut flags = String::new();
        if input.get_option("update-with-all-dependencies").as_bool().unwrap_or(false)
            || input.get_option("with-all-dependencies").as_bool().unwrap_or(false)
        {
            update_allow_transitive_dependencies = Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS;
            flags += " --with-all-dependencies";
        } else if input.get_option("no-update-with-dependencies").as_bool().unwrap_or(false) {
            update_allow_transitive_dependencies = Request::UPDATE_ONLY_LISTED;
            flags += " --with-dependencies";
        }

        io.write_error(&format!(
            "<info>Running composer update {}{}</info>",
            packages.join(" "),
            flags
        ));

        install.set_verbose(input.get_option("verbose").as_bool().unwrap_or(false));
        install.set_dev_mode(update_dev_mode);
        install.set_optimize_autoloader(optimize);
        install.set_class_map_authoritative(authoritative);
        install.set_apcu_autoloader(apcu, apcu_prefix);
        install.set_update(true);
        install.set_install(!input.get_option("no-install").as_bool().unwrap_or(false));
        install.set_update_allow_transitive_dependencies(update_allow_transitive_dependencies);
        install.set_platform_requirement_filter(
            self.get_platform_requirement_filter(input),
        );
        install.set_dry_run(dry_run);
        install.set_audit_config(self.create_audit_config(composer.get_config(), input));
        install.set_minimal_update(minimal_changes);

        // if no lock is present, we do not do a partial update as
        // this is not supported by the Installer
        if composer.get_locker().is_locked() {
            install.set_update_allow_list(packages.clone());
        }

        let status = install.run()?;
        if status != 0 {
            io.write_error(&format!(
                "\n<error>Removal failed, reverting {} to its original content.</error>",
                file
            ));
            std::fs::write(json_file.get_path(), &composer_backup)?;
        }

        if !dry_run {
            for package in &packages {
                if !composer
                    .get_repository_manager()
                    .get_local_repository()
                    .find_packages(package)
                    .is_empty()
                {
                    io.write_error(&format!(
                        "<error>Removal failed, {} is still present, it may be required by another package. See `composer why {}`.</error>",
                        package, package
                    ));
                    return Ok(2);
                }
            }
        }

        Ok(status)
    }
}
