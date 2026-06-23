//! ref: composer/src/Composer/Command/UpdateCommand.php

use crate::io::io_interface;
use crate::package::base_package;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::helper::Table;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, array_filter, array_intersect,
    array_keys, array_merge_map, array_search_in_vec, count, empty, in_array, sprintf, strtolower,
};
use shirabe_semver::Intervals;
use shirabe_semver::constraint::MultiConstraint;
use std::cell::RefCell;
use std::rc::Rc;

use crate::advisory::AuditConfig;
use crate::advisory::Auditor;
use crate::command::BumpCommand;
use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::dependency_resolver::request::{self, Request, UpdateAllowTransitiveDeps};
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::installer::Installer;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::loader::RootPackageLoader;
use crate::package::version::VersionParser;
use crate::package::version::VersionSelector;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::repository::CanonicalPackagesTrait;
use crate::repository::CompositeRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositorySet;
use crate::util::HttpDownloader;

#[derive(Debug)]
pub struct UpdateCommand {
    base_command_data: BaseCommandData,
}

impl Default for UpdateCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl UpdateCommand {
    pub fn new() -> Self {
        let command = UpdateCommand {
            base_command_data: BaseCommandData::new(None),
        };
        command
            .configure()
            .expect("UpdateCommand::configure uses static, valid metadata");
        command
    }
}

impl Command for UpdateCommand {
    fn configure(&self) -> anyhow::Result<()> {
        // TODO(cli-completion): suggest_installed_package(false, true) / suggest_prefer_install
        self.set_name("update")?;
        self.set_aliases(vec!["u".to_string(), "upgrade".to_string()])?;
        self.set_description("Updates your dependencies to the latest version according to composer.json, and updates the composer.lock file");
        // TODO(phase-c): populate with InputArgument/InputOption entries (see PHP UpdateCommand);
        // blocked on the symfony InputDefinition entry modeling.
        self.set_definition(&[]);
        self.set_help(
            "The <info>update</info> command reads the composer.json file from the\n\
            current directory, processes it, and updates, removes or installs all the\n\
            dependencies.\n\n\
            <info>shirabe update</info>\n\n\
            To limit the update operation to a few packages, you can list the package(s)\n\
            you want to update as such:\n\n\
            <info>shirabe update vendor/package1 foo/mypackage [...]</info>\n\n\
            You may also use an asterisk (*) pattern to limit the update operation to package(s)\n\
            from a specific vendor:\n\n\
            <info>shirabe update vendor/package1 foo/* [...]</info>\n\n\
            To run an update with more restrictive constraints you can use:\n\n\
            <info>shirabe update --with vendor/package:1.0.*</info>\n\n\
            To run a partial update with more restrictive constraints you can use the shorthand:\n\n\
            <info>shirabe update vendor/package:1.0.*</info>\n\n\
            To select packages names interactively with auto-completion use <info>-i</info>.\n\n\
            Read more at https://getcomposer.org/doc/03-cli.md#update-u-upgrade\n",
        );
        Ok(())
    }

    fn execute(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let io = self.get_io().clone();
        if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
            io.write_error3(
                "<warning>You are using the deprecated option \"--dev\". It has no effect and will break in Composer 3.</warning>",
                true,
                io_interface::NORMAL,
            );
        }
        if input
            .borrow()
            .get_option("no-suggest")?
            .as_bool()
            .unwrap_or(false)
        {
            io.write_error3(
                "<warning>You are using the deprecated option \"--no-suggest\". It has no effect and will break in Composer 3.</warning>",
                true,
                io_interface::NORMAL,
            );
        }

        let composer_handle = self.require_composer(None, None)?;
        let composer = crate::command::composer_full(&composer_handle);

        if !HttpDownloader::is_curl_enabled() {
            io.write_error3(
                "<warning>Composer is operating significantly slower than normal because you do not have the PHP curl extension enabled.</warning>",
                true,
                io_interface::NORMAL,
            );
        }

        let mut packages: Vec<String> = input
            .borrow()
            .get_argument("packages")?
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut reqs: IndexMap<String, String> = self.format_requirements(
            input
                .borrow()
                .get_option("with")?
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
        )?;

        // extract --with shorthands from the allowlist
        if !packages.is_empty() {
            let allowlist_packages_with_requirements: Vec<String> =
                array_filter(&packages, |pkg: &String| -> bool {
                    Preg::is_match(r"{\S+[ =:]\S+}", pkg)
                });
            for (package, constraint) in
                self.format_requirements(allowlist_packages_with_requirements.clone())?
            {
                reqs.insert(package, constraint);
            }

            // replace the foo/bar:req by foo/bar in the allowlist
            for package in &allowlist_packages_with_requirements {
                let package_name = Preg::replace(r"{^([^ =:]+)[ =:].*$}", "$1", package);
                if let Some(idx) = array_search_in_vec(package, &packages) {
                    packages[idx] = package_name;
                }
            }
        }

        let root_package = composer.get_package();
        root_package.set_references(RootPackageLoader::extract_references(
            &reqs,
            root_package.get_references().clone(),
        ));
        root_package.set_stability_flags(RootPackageLoader::extract_stability_flags(
            &reqs,
            &root_package.get_minimum_stability(),
            root_package.get_stability_flags().clone(),
        ));

        let parser = VersionParser::new();
        let mut temporary_constraints: IndexMap<String, _> = IndexMap::new();
        let root_requirements =
            array_merge_map(root_package.get_requires(), root_package.get_dev_requires());
        for (package, constraint) in &reqs {
            let package = strtolower(package);
            let parsed_constraint = parser.parse_constraints(constraint)?;
            temporary_constraints.insert(package.clone(), parsed_constraint.clone());
            if let Some(root_req) = root_requirements.get(&package)
                && !Intervals::have_intersections(&parsed_constraint, root_req.get_constraint())?
            {
                io.write_error3(
                        &format!(
                            "<error>The temporary constraint \"{}\" for \"{}\" must be a subset of the constraint in your composer.json ({})</error>",
                            constraint,
                            package,
                            root_req.get_pretty_constraint(),
                        ),
                        true,
                        io_interface::NORMAL,
                    );
                io.write(&format!(
                        "<info>Run `composer require {}` or `composer require {}:{}` instead to replace the constraint</info>",
                        package, package, constraint,
                    ));

                return Ok(crate::command::FAILURE);
            }
        }

        if input
            .borrow()
            .get_option("patch-only")?
            .as_bool()
            .unwrap_or(false)
        {
            if !composer.get_locker().borrow_mut().is_locked() {
                return Err(InvalidArgumentException {
                    message: "patch-only can only be used with a lock file present".to_string(),
                    code: 0,
                }
                .into());
            }
            for package in composer
                .get_locker()
                .borrow_mut()
                .get_locked_repository(true)?
                .borrow_mut()
                .get_canonical_packages()?
            {
                if package.is_dev() {
                    continue;
                }
                let matches = Preg::is_match_with_indexed_captures(
                    r"{^(\d+\.\d+\.\d+)}",
                    &package.get_version(),
                );
                let Some(matches) = matches else {
                    continue;
                };
                let constraint = parser.parse_constraints(&format!(
                    "~{}",
                    matches.get(1).cloned().unwrap_or_default()
                ))?;
                if let Some(existing) = temporary_constraints.get(&package.get_name()) {
                    temporary_constraints.insert(
                        package.get_name(),
                        MultiConstraint::create(vec![existing.clone(), constraint], true, None)?,
                    );
                } else {
                    temporary_constraints.insert(package.get_name(), constraint);
                }
            }
        }

        if input
            .borrow()
            .get_option("interactive")?
            .as_bool()
            .unwrap_or(false)
        {
            packages = self.get_packages_interactively(
                io.clone(),
                input.clone(),
                output.clone(),
                &composer_handle,
                packages,
            )?;
        }

        if input
            .borrow()
            .get_option("root-reqs")?
            .as_bool()
            .unwrap_or(false)
        {
            let mut requires: Vec<String> = array_keys(&root_package.get_requires());
            if !input
                .borrow()
                .get_option("no-dev")?
                .as_bool()
                .unwrap_or(false)
            {
                requires.extend(array_keys(&root_package.get_dev_requires()));
            }

            if !packages.is_empty() {
                packages = array_intersect(&packages, &requires);
            } else {
                packages = requires;
            }
        }

        // the arguments lock/nothing/mirrors are not package names but trigger a mirror update instead
        // they are further mutually exclusive with listing actual package names
        let filtered_packages: Vec<String> = array_filter(&packages, |package: &String| -> bool {
            !in_array(
                PhpMixed::String(package.clone()),
                &PhpMixed::List(vec![
                    PhpMixed::String("lock".to_string()),
                    PhpMixed::String("nothing".to_string()),
                    PhpMixed::String("mirrors".to_string()),
                ]),
                true,
            )
        });
        let update_mirrors = input
            .borrow()
            .get_option("lock")?
            .as_bool()
            .unwrap_or(false)
            || filtered_packages.len() != packages.len();
        packages = filtered_packages;

        if update_mirrors && !packages.is_empty() {
            io.write_error3("<error>You cannot simultaneously update only a selection of packages and regenerate the lock file metadata.</error>", true, io_interface::NORMAL);

            return Ok(-1);
        }

        let mut command_event =
            CommandEvent::new(PluginEvents::COMMAND, "update", input.clone(), output);
        let command_event_name = command_event.get_name().to_string();
        composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch(Some(&command_event_name), Some(&mut command_event))?;

        composer
            .get_installation_manager()
            .borrow_mut()
            .set_output_progress(
                !input
                    .borrow()
                    .get_option("no-progress")?
                    .as_bool()
                    .unwrap_or(false),
            );

        let mut install = Installer::create(io.clone(), &composer_handle);

        let config = composer.get_config();
        let (prefer_source, prefer_dist) =
            self.get_preferred_install_options(&config.borrow(), input.clone(), false)?;

        let optimize = input
            .borrow()
            .get_option("optimize-autoloader")?
            .as_bool()
            .unwrap_or(false)
            || config
                .borrow_mut()
                .get("optimize-autoloader")
                .as_bool()
                .unwrap_or(false);
        let authoritative = input
            .borrow()
            .get_option("classmap-authoritative")?
            .as_bool()
            .unwrap_or(false)
            || config
                .borrow_mut()
                .get("classmap-authoritative")
                .as_bool()
                .unwrap_or(false);
        let apcu_prefix: Option<String> = input
            .borrow()
            .get_option("apcu-autoloader-prefix")?
            .as_string()
            .map(|s| s.to_string());
        let apcu = apcu_prefix.is_some()
            || input
                .borrow()
                .get_option("apcu-autoloader")?
                .as_bool()
                .unwrap_or(false)
            || config
                .borrow_mut()
                .get("apcu-autoloader")
                .as_bool()
                .unwrap_or(false);
        let minimal_changes = input
            .borrow()
            .get_option("minimal-changes")?
            .as_bool()
            .unwrap_or(false)
            || config
                .borrow_mut()
                .get("update-with-minimal-changes")
                .as_bool()
                .unwrap_or(false);

        let mut update_allow_transitive_dependencies = UpdateAllowTransitiveDeps::UpdateOnlyListed;
        if input
            .borrow()
            .get_option("with-all-dependencies")?
            .as_bool()
            .unwrap_or(false)
        {
            update_allow_transitive_dependencies =
                UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDeps;
        } else if input
            .borrow()
            .get_option("with-dependencies")?
            .as_bool()
            .unwrap_or(false)
        {
            update_allow_transitive_dependencies =
                UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDepsNoRootRequire;
        }

        install
            .set_dry_run(
                input
                    .borrow()
                    .get_option("dry-run")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_verbose(
                input
                    .borrow()
                    .get_option("verbose")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_prefer_source(prefer_source)
            .set_prefer_dist(prefer_dist)
            .set_dev_mode(
                !input
                    .borrow()
                    .get_option("no-dev")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_dump_autoloader(
                !input
                    .borrow()
                    .get_option("no-autoloader")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_optimize_autoloader(optimize)
            .set_class_map_authoritative(authoritative)
            .set_apcu_autoloader(apcu, apcu_prefix.clone())
            .set_update(true)
            .set_install(
                !input
                    .borrow()
                    .get_option("no-install")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_update_mirrors(update_mirrors)
            .set_update_allow_list(packages.clone())
            .set_update_allow_transitive_dependencies(update_allow_transitive_dependencies)?
            .set_platform_requirement_filter(self.get_platform_requirement_filter(input.clone())?)
            .set_prefer_stable(
                input
                    .borrow()
                    .get_option("prefer-stable")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_prefer_lowest(
                input
                    .borrow()
                    .get_option("prefer-lowest")?
                    .as_bool()
                    .unwrap_or(false),
            )
            .set_temporary_constraints(temporary_constraints)
            .set_audit_config(
                self.create_audit_config(&mut composer.get_config().borrow_mut(), input.clone())?,
            )
            .set_minimal_update(minimal_changes);

        if input
            .borrow()
            .get_option("no-plugins")?
            .as_bool()
            .unwrap_or(false)
        {
            install.disable_plugins();
        }

        let mut result = install.run()?;

        if result == 0
            && !input
                .borrow()
                .get_option("lock")?
                .as_bool()
                .unwrap_or(false)
        {
            let mut bump_after_update = input.borrow().get_option("bump-after-update")?;
            // PHP: false === $bumpAfterUpdate (strict)
            if matches!(bump_after_update, PhpMixed::Bool(false)) {
                bump_after_update = composer.get_config().borrow().get("bump-after-update");
            }

            if !matches!(bump_after_update, PhpMixed::Bool(false)) {
                io.write_error3(
                    "<info>Bumping dependencies</info>",
                    true,
                    io_interface::NORMAL,
                );
                let mut bump_command = BumpCommand::new();
                bump_command.set_composer(composer_handle.clone());
                result = bump_command.do_bump(
                    io.clone(),
                    bump_after_update.as_string() == Some("dev"),
                    bump_after_update.as_string() == Some("no-dev"),
                    input
                        .borrow()
                        .get_option("dry-run")?
                        .as_bool()
                        .unwrap_or(false),
                    input
                        .borrow()
                        .get_argument("packages")?
                        .as_list()
                        .map(|l| {
                            l.iter()
                                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    "--bump-after-update=dev".to_string(),
                )?;
            }
        }

        Ok(result)
    }

    fn initialize(
        &self,
        input: Rc<RefCell<dyn InputInterface>>,
        output: Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    shirabe_external_packages::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for UpdateCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}

impl UpdateCommand {
    /// @param array<string> $packages
    /// @return array<string>
    fn get_packages_interactively(
        &self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        composer: &PartialComposerHandle,
        packages: Vec<String>,
    ) -> Result<Vec<String>> {
        if !input.borrow().is_interactive() {
            return Err(InvalidArgumentException {
                message: "--interactive cannot be used in non-interactive terminals.".to_string(),
                code: 0,
            }
            .into());
        }

        let composer_ref = crate::command::composer_full(composer);
        let platform_req_filter = self.get_platform_requirement_filter(input);
        let stability_flags = composer_ref.get_package().get_stability_flags();
        let requires = array_merge_map(
            composer_ref.get_package().get_requires(),
            composer_ref.get_package().get_dev_requires(),
        );

        let filter: Option<String> = if !packages.is_empty() {
            Some(base_package::package_names_to_regexp(&packages, "%s"))
        } else {
            None
        };

        io.write_error3(
            "<info>Loading packages that can be updated...</info>",
            true,
            io_interface::NORMAL,
        );
        let mut autocompleter_values: IndexMap<String, String> = IndexMap::new();
        let installed_packages: Vec<crate::package::PackageInterfaceHandle> =
            if composer_ref.get_locker().borrow_mut().is_locked() {
                let locked_repo = composer_ref
                    .get_locker()
                    .borrow_mut()
                    .get_locked_repository(true)?;
                locked_repo.borrow_mut().get_canonical_packages()?
            } else {
                composer_ref
                    .get_repository_manager()
                    .borrow()
                    .get_local_repository()
                    .get_packages()?
            };
        let mut version_selector = self.create_version_selector(composer)?;
        for package in &installed_packages {
            if let Some(filter) = &filter
                && !Preg::is_match(filter, &package.get_name())
            {
                continue;
            }
            let current_version = package.get_pretty_version();
            let constraint = requires
                .get(&package.get_name())
                .map(|link| link.get_pretty_constraint());
            let stability = match stability_flags.get(&package.get_name()) {
                Some(flag) => base_package::STABILITIES
                    .iter()
                    .find(|&(_, v)| v == flag)
                    .map(|(k, _)| k.to_string())
                    .unwrap_or_default(),
                None => composer_ref.get_package().get_minimum_stability(),
            };
            let latest_version = version_selector.find_best_candidate(
                &package.get_name(),
                constraint,
                &stability,
                None,
                0,
                None,
                PhpMixed::Bool(true),
            )?;
            let _ = &platform_req_filter;
            if let Some(latest) = latest_version
                && (package.get_version() != latest.get_version() || latest.is_dev())
            {
                autocompleter_values.insert(
                    package.get_name(),
                    format!(
                        "<comment>{}</comment> => <comment>{}</comment>",
                        current_version,
                        latest.get_pretty_version(),
                    ),
                );
            }
        }
        if installed_packages.is_empty() {
            for (req, _constraint) in &requires {
                if PlatformRepository::is_platform_package(req) {
                    continue;
                }
                autocompleter_values.insert(req.to_string(), String::new());
            }
        }

        if autocompleter_values.is_empty() {
            return Err(RuntimeException {
                message: "Could not find any package with new versions available".to_string(),
                code: 0,
            }
            .into());
        }

        let select_result = io.select(
            "Select packages: (Select more than one value separated by comma) ".to_string(),
            autocompleter_values
                .keys()
                .cloned()
                .collect::<Vec<String>>(),
            PhpMixed::Bool(false),
            PhpMixed::Int(1),
            "No package named \"%s\" is installed.".to_string(),
            true,
        );
        let packages: Vec<String> = match select_result {
            PhpMixed::List(l) => l
                .into_iter()
                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                .collect(),
            _ => Vec::new(),
        };

        let mut table = Table::new(output);
        table.set_headers(vec![PhpMixed::String("Selected packages".to_string())]);
        for package in &packages {
            table.add_row(PhpMixed::List(vec![PhpMixed::String(package.clone())]));
        }
        table.render();

        if io.ask_confirmation(
            format!(
                "Would you like to continue and update the above package{} [<comment>yes</comment>]? ",
                PhpMixed::String(
                    if 1 == packages.len() { "" } else { "s" }.to_string(),
                ),
            ),
            true,
        ) {
            return Ok(packages);
        }

        Err(RuntimeException {
            message: "Installation aborted.".to_string(),
            code: 0,
        }
        .into())
    }

    fn create_version_selector(&self, composer: &PartialComposerHandle) -> Result<VersionSelector> {
        let composer = crate::command::composer_full(composer);
        let root_aliases: Vec<crate::repository::RootAliasInput> = composer
            .get_package()
            .get_aliases()
            .into_iter()
            .map(|alias| crate::repository::RootAliasInput {
                package: alias.get("package").cloned().unwrap_or_default(),
                version: alias.get("version").cloned().unwrap_or_default(),
                alias: alias.get("alias").cloned().unwrap_or_default(),
                alias_normalized: alias.get("alias_normalized").cloned().unwrap_or_default(),
            })
            .collect();
        let mut repository_set = RepositorySet::new(
            &composer.get_package().get_minimum_stability(),
            composer.get_package().get_stability_flags().clone(),
            root_aliases,
            composer.get_package().get_references().clone(),
            IndexMap::new(),
            IndexMap::new(),
        );
        let repositories: Vec<crate::repository::RepositoryInterfaceHandle> = composer
            .get_repository_manager()
            .borrow()
            .get_repositories()
            .iter()
            .filter(|repository| !repository.is::<PlatformRepository>())
            .cloned()
            .collect();
        repository_set.add_repository(crate::repository::RepositoryInterfaceHandle::new(
            CompositeRepository::new(repositories),
        ))?;

        VersionSelector::new(
            std::rc::Rc::new(std::cell::RefCell::new(repository_set)),
            None,
        )
    }
}
