//! ref: composer/src/Composer/Command/UpdateCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::helper::table::Table;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, array_filter, array_intersect,
    array_keys, array_merge, array_search, count, empty, in_array, sprintf, strtolower,
};
use shirabe_semver::constraint::multi_constraint::MultiConstraint;
use shirabe_semver::intervals::Intervals;

use crate::advisory::auditor::Auditor;
use crate::command::base_command::BaseCommand;
use crate::command::bump_command::BumpCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::composer::Composer;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::dependency_resolver::request::{self, Request, UpdateAllowTransitiveDeps};
use crate::installer::Installer;
use crate::io::io_interface::IOInterface;
use crate::package::base_package::BasePackage;
use crate::package::loader::root_package_loader::RootPackageLoader;
use crate::package::version::version_parser::VersionParser;
use crate::package::version::version_selector::VersionSelector;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_interface::RepositoryInterface;
use crate::repository::repository_set::RepositorySet;
use crate::util::http_downloader::HttpDownloader;

#[derive(Debug)]
pub struct UpdateCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,
}

impl CompletionTrait for UpdateCommand {}

impl UpdateCommand {
    pub fn configure(&mut self) {
        let suggest_installed_package = self.suggest_installed_package(false, true);
        let suggest_prefer_install = self.suggest_prefer_install();
        self.inner
            .set_name("update")
            .set_aliases(vec!["u".to_string(), "upgrade".to_string()])
            .set_description("Updates your dependencies to the latest version according to composer.json, and updates the composer.lock file")
            .set_definition(vec![
                // TODO(phase-b): InputArgument/InputOption constructors and types
                todo!("Box<dyn InputDefinitionEntry> for InputArgument::new(\"packages\", IS_ARRAY|OPTIONAL, ..., suggest_installed_package)"),
                todo!("InputOption::new(\"with\", ..., VALUE_IS_ARRAY|VALUE_REQUIRED, ...)"),
                todo!("InputOption::new(\"prefer-source\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"prefer-dist\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"prefer-install\", ..., VALUE_REQUIRED, ..., suggest_prefer_install)"),
                todo!("InputOption::new(\"dry-run\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"dev\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"no-dev\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"lock\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"no-install\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"no-audit\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"audit-format\", ..., VALUE_REQUIRED, ..., Auditor::FORMAT_SUMMARY, Auditor::FORMATS)"),
                todo!("InputOption::new(\"no-security-blocking\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"no-autoloader\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"no-suggest\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"no-progress\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"with-dependencies\", \"w\", VALUE_NONE, ...)"),
                todo!("InputOption::new(\"with-all-dependencies\", \"W\", VALUE_NONE, ...)"),
                todo!("InputOption::new(\"verbose\", \"v|vv|vvv\", VALUE_NONE, ...)"),
                todo!("InputOption::new(\"optimize-autoloader\", \"o\", VALUE_NONE, ...)"),
                todo!("InputOption::new(\"classmap-authoritative\", \"a\", VALUE_NONE, ...)"),
                todo!("InputOption::new(\"apcu-autoloader\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"apcu-autoloader-prefix\", ..., VALUE_REQUIRED, ...)"),
                todo!("InputOption::new(\"ignore-platform-req\", ..., VALUE_REQUIRED|VALUE_IS_ARRAY, ...)"),
                todo!("InputOption::new(\"ignore-platform-reqs\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"prefer-stable\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"prefer-lowest\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"minimal-changes\", \"m\", VALUE_NONE, ...)"),
                todo!("InputOption::new(\"patch-only\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"interactive\", \"i\", VALUE_NONE, ...)"),
                todo!("InputOption::new(\"root-reqs\", ..., VALUE_NONE, ...)"),
                todo!("InputOption::new(\"bump-after-update\", ..., VALUE_OPTIONAL, ..., false, ['dev', 'no-dev', 'all'])"),
            ])
            .set_help(
                "The <info>update</info> command reads the composer.json file from the\n\
                current directory, processes it, and updates, removes or installs all the\n\
                dependencies.\n\n\
                <info>php composer.phar update</info>\n\n\
                To limit the update operation to a few packages, you can list the package(s)\n\
                you want to update as such:\n\n\
                <info>php composer.phar update vendor/package1 foo/mypackage [...]</info>\n\n\
                You may also use an asterisk (*) pattern to limit the update operation to package(s)\n\
                from a specific vendor:\n\n\
                <info>php composer.phar update vendor/package1 foo/* [...]</info>\n\n\
                To run an update with more restrictive constraints you can use:\n\n\
                <info>php composer.phar update --with vendor/package:1.0.*</info>\n\n\
                To run a partial update with more restrictive constraints you can use the shorthand:\n\n\
                <info>php composer.phar update vendor/package:1.0.*</info>\n\n\
                To select packages names interactively with auto-completion use <info>-i</info>.\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#update-u-upgrade\n",
            );
    }

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> Result<i64> {
        let io = self.inner.get_io();
        if input.get_option("dev").as_bool().unwrap_or(false) {
            io.write_error(
                PhpMixed::String(
                    "<warning>You are using the deprecated option \"--dev\". It has no effect and will break in Composer 3.</warning>".to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
        }
        if input.get_option("no-suggest").as_bool().unwrap_or(false) {
            io.write_error(
                PhpMixed::String(
                    "<warning>You are using the deprecated option \"--no-suggest\". It has no effect and will break in Composer 3.</warning>".to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
        }

        let composer = self.require_composer(None, None);

        if !HttpDownloader::is_curl_enabled() {
            io.write_error(
                PhpMixed::String(
                    "<warning>Composer is operating significantly slower than normal because you do not have the PHP curl extension enabled.</warning>".to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
        }

        let mut packages: Vec<String> = input
            .get_argument("packages")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut reqs: IndexMap<String, String> = self.inner.format_requirements(
            input
                .get_option("with")
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
        );

        // extract --with shorthands from the allowlist
        if packages.len() > 0 {
            let allowlist_packages_with_requirements: Vec<String> =
                array_filter(&packages, |pkg: &String| -> bool {
                    Preg::is_match(r"{\S+[ =:]\S+}", pkg)
                });
            for (package, constraint) in self
                .inner
                .format_requirements(allowlist_packages_with_requirements.clone())
            {
                reqs.insert(package, constraint);
            }

            // replace the foo/bar:req by foo/bar in the allowlist
            for package in &allowlist_packages_with_requirements {
                let package_name = Preg::replace(r"{^([^ =:]+)[ =:].*$}", "$1", package);
                let index = array_search(
                    package,
                    // TODO(phase-b): array_search expects IndexMap<String, String>; supply a wrapper
                    todo!("packages as IndexMap<String, String>"),
                );
                if let Some(idx) = index {
                    // TODO(phase-b): mutate packages[idx] — PHP integer-keyed array
                    let _ = idx;
                    let _ = package_name;
                }
            }
        }

        let root_package = composer.get_package();
        root_package.set_references(RootPackageLoader::extract_references(
            &reqs,
            &root_package.get_references(),
        ));
        root_package.set_stability_flags(RootPackageLoader::extract_stability_flags(
            &reqs,
            root_package.get_minimum_stability(),
            root_package.get_stability_flags(),
        ));

        let parser = VersionParser::new();
        let mut temporary_constraints: IndexMap<String, _> = IndexMap::new();
        let root_requirements = array_merge(
            // TODO(phase-b): array_merge for IndexMap<String, Link>
            todo!("root_package.get_requires() as PhpMixed"),
            todo!("root_package.get_dev_requires() as PhpMixed"),
        );
        for (package, constraint) in &reqs {
            let package = strtolower(package);
            let parsed_constraint = parser.parse_constraints(constraint)?;
            temporary_constraints.insert(package.clone(), parsed_constraint.clone());
            // TODO(phase-b): access root_requirements[package].getConstraint()
            let intersected = todo!("Intervals::haveIntersections check");
            if let Some(_root_req) = todo!("root_requirements.get(&package)") {
                if !intersected {
                    io.write_error(
                        PhpMixed::String(format!(
                            "<error>The temporary constraint \"{}\" for \"{}\" must be a subset of the constraint in your composer.json ({})</error>",
                            constraint,
                            package,
                            todo!("root_requirements[package].get_pretty_constraint()"),
                        )),
                        true,
                        IOInterface::NORMAL,
                    );
                    io.write(
                        PhpMixed::String(format!(
                            "<info>Run `composer require {}` or `composer require {}:{}` instead to replace the constraint</info>",
                            package, package, constraint,
                        )),
                        true,
                        IOInterface::NORMAL,
                    );

                    return Ok(BaseCommand::FAILURE);
                }
            }
        }

        if input.get_option("patch-only").as_bool().unwrap_or(false) {
            if !composer.get_locker().is_locked() {
                return Err(InvalidArgumentException {
                    message: "patch-only can only be used with a lock file present".to_string(),
                    code: 0,
                }
                .into());
            }
            for package in composer
                .get_locker()
                .get_locked_repository(true)?
                .get_canonical_packages()
            {
                if package.is_dev() {
                    continue;
                }
                let matches = Preg::is_match_with_indexed_captures(
                    r"{^(\d+\.\d+\.\d+)}",
                    package.get_version(),
                )?;
                let Some(matches) = matches else {
                    continue;
                };
                let constraint = parser.parse_constraints(&format!(
                    "~{}",
                    matches.get(1).cloned().unwrap_or_default()
                ))?;
                if temporary_constraints.contains_key(package.get_name()) {
                    let existing = temporary_constraints
                        .get(package.get_name())
                        .cloned()
                        .unwrap();
                    temporary_constraints.insert(
                        package.get_name().to_string(),
                        // TODO(phase-b): MultiConstraint::create signature
                        todo!("MultiConstraint::create([existing, constraint], true)"),
                    );
                } else {
                    temporary_constraints.insert(package.get_name().to_string(), constraint);
                }
            }
        }

        if input.get_option("interactive").as_bool().unwrap_or(false) {
            packages = self.get_packages_interactively(io, input, output, &composer, packages)?;
        }

        if input.get_option("root-reqs").as_bool().unwrap_or(false) {
            let mut requires: Vec<String> = array_keys(&root_package.get_requires());
            if !input.get_option("no-dev").as_bool().unwrap_or(false) {
                requires = array_merge(
                    // TODO(phase-b): array_merge for Vec<String>
                    todo!("requires as PhpMixed"),
                    todo!("array_keys(&root_package.get_dev_requires()) as PhpMixed"),
                )
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
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
                    Box::new(PhpMixed::String("lock".to_string())),
                    Box::new(PhpMixed::String("nothing".to_string())),
                    Box::new(PhpMixed::String("mirrors".to_string())),
                ]),
                true,
            )
        });
        let update_mirrors = input.get_option("lock").as_bool().unwrap_or(false)
            || filtered_packages.len() != packages.len();
        packages = filtered_packages;

        if update_mirrors && !packages.is_empty() {
            io.write_error(
                PhpMixed::String(
                    "<error>You cannot simultaneously update only a selection of packages and regenerate the lock file metadata.</error>"
                        .to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );

            return Ok(-1);
        }

        let mut command_event = CommandEvent::new(PluginEvents::COMMAND, "update", input, output);
        composer
            .get_event_dispatcher()
            .dispatch(&command_event.get_name(), &mut command_event);

        composer
            .get_installation_manager()
            .set_output_progress(!input.get_option("no-progress").as_bool().unwrap_or(false));

        let mut install = Installer::create(io, &composer);

        let config = composer.get_config();
        let (prefer_source, prefer_dist) = self
            .inner
            .get_preferred_install_options(config, input, false);

        let optimize = input
            .get_option("optimize-autoloader")
            .as_bool()
            .unwrap_or(false)
            || config.get("optimize-autoloader").as_bool().unwrap_or(false);
        let authoritative = input
            .get_option("classmap-authoritative")
            .as_bool()
            .unwrap_or(false)
            || config
                .get("classmap-authoritative")
                .as_bool()
                .unwrap_or(false);
        let apcu_prefix = input.get_option("apcu-autoloader-prefix");
        let apcu = !matches!(apcu_prefix, PhpMixed::Null)
            || input
                .get_option("apcu-autoloader")
                .as_bool()
                .unwrap_or(false)
            || config.get("apcu-autoloader").as_bool().unwrap_or(false);
        let minimal_changes = input
            .get_option("minimal-changes")
            .as_bool()
            .unwrap_or(false)
            || config
                .get("update-with-minimal-changes")
                .as_bool()
                .unwrap_or(false);

        let mut update_allow_transitive_dependencies = UpdateAllowTransitiveDeps::UpdateOnlyListed;
        if input
            .get_option("with-all-dependencies")
            .as_bool()
            .unwrap_or(false)
        {
            update_allow_transitive_dependencies =
                UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDeps;
        } else if input
            .get_option("with-dependencies")
            .as_bool()
            .unwrap_or(false)
        {
            update_allow_transitive_dependencies =
                UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDepsNoRootRequire;
        }

        install
            .set_dry_run(input.get_option("dry-run").as_bool().unwrap_or(false))
            .set_verbose(input.get_option("verbose").as_bool().unwrap_or(false))
            .set_prefer_source(prefer_source)
            .set_prefer_dist(prefer_dist)
            .set_dev_mode(!input.get_option("no-dev").as_bool().unwrap_or(false))
            .set_dump_autoloader(!input.get_option("no-autoloader").as_bool().unwrap_or(false))
            .set_optimize_autoloader(optimize)
            .set_class_map_authoritative(authoritative)
            .set_apcu_autoloader(apcu, apcu_prefix)
            .set_update(true)
            .set_install(!input.get_option("no-install").as_bool().unwrap_or(false))
            .set_update_mirrors(update_mirrors)
            .set_update_allow_list(packages.clone())
            .set_update_allow_transitive_dependencies(update_allow_transitive_dependencies)
            .set_platform_requirement_filter(self.inner.get_platform_requirement_filter(input))
            .set_prefer_stable(input.get_option("prefer-stable").as_bool().unwrap_or(false))
            .set_prefer_lowest(input.get_option("prefer-lowest").as_bool().unwrap_or(false))
            .set_temporary_constraints(temporary_constraints)
            .set_audit_config(
                self.inner
                    .create_audit_config(composer.get_config(), input)?,
            )
            .set_minimal_update(minimal_changes);

        if input.get_option("no-plugins").as_bool().unwrap_or(false) {
            install.disable_plugins();
        }

        let mut result = install.run()?;

        if result == 0 && !input.get_option("lock").as_bool().unwrap_or(false) {
            let mut bump_after_update = input.get_option("bump-after-update");
            // PHP: false === $bumpAfterUpdate (strict)
            if matches!(bump_after_update, PhpMixed::Bool(false)) {
                bump_after_update = composer.get_config().get("bump-after-update");
            }

            if !matches!(bump_after_update, PhpMixed::Bool(false)) {
                io.write_error(
                    PhpMixed::String("<info>Bumping dependencies</info>".to_string()),
                    true,
                    IOInterface::NORMAL,
                );
                let mut bump_command = BumpCommand::new();
                bump_command.set_composer(composer.clone());
                result = bump_command.do_bump(
                    io,
                    bump_after_update.as_string() == Some("dev"),
                    bump_after_update.as_string() == Some("no-dev"),
                    input.get_option("dry-run").as_bool().unwrap_or(false),
                    input
                        .get_argument("packages")
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

    /// @param array<string> $packages
    /// @return array<string>
    fn get_packages_interactively(
        &self,
        io: &dyn IOInterface,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
        composer: &Composer,
        packages: Vec<String>,
    ) -> Result<Vec<String>> {
        if !input.is_interactive() {
            return Err(InvalidArgumentException {
                message: "--interactive cannot be used in non-interactive terminals.".to_string(),
                code: 0,
            }
            .into());
        }

        let platform_req_filter = self.inner.get_platform_requirement_filter(input);
        let stability_flags = composer.get_package().get_stability_flags();
        let requires = array_merge(
            // TODO(phase-b): array_merge for IndexMap<String, Link>
            todo!("composer.get_package().get_requires() as PhpMixed"),
            todo!("composer.get_package().get_dev_requires() as PhpMixed"),
        );

        let filter: Option<String> = if packages.len() > 0 {
            // TODO(phase-b): BasePackage::package_names_to_regexp signature
            Some(BasePackage::package_names_to_regexp(&packages, "%s"))
        } else {
            None
        };

        io.write_error(
            PhpMixed::String("<info>Loading packages that can be updated...</info>".to_string()),
            true,
            IOInterface::NORMAL,
        );
        let mut autocompleter_values: IndexMap<String, String> = IndexMap::new();
        let installed_packages = if composer.get_locker().is_locked() {
            composer
                .get_locker()
                .get_locked_repository(true)?
                .get_packages()
        } else {
            composer
                .get_repository_manager()
                .get_local_repository()
                .get_packages()
        };
        let version_selector = self.create_version_selector(composer);
        for package in &installed_packages {
            if let Some(filter) = &filter {
                if !Preg::is_match(filter, package.get_name()) {
                    continue;
                }
            }
            let current_version = package.get_pretty_version();
            let constraint =
                todo!("requires[package.get_name()].get_pretty_constraint() if present");
            let stability = todo!(
                "if stabilityFlags[package_name] use array_search(BasePackage::STABILITIES) else minimum_stability"
            );
            let latest_version = version_selector.find_best_candidate(
                package.get_name(),
                constraint,
                stability,
                &*platform_req_filter,
            );
            if let Some(latest) = latest_version {
                if package.get_version() != latest.get_version() || latest.is_dev() {
                    autocompleter_values.insert(
                        package.get_name().to_string(),
                        format!(
                            "<comment>{}</comment> => <comment>{}</comment>",
                            current_version,
                            latest.get_pretty_version(),
                        ),
                    );
                }
            }
        }
        if 0 == installed_packages.len() {
            for (req, _constraint) in &requires {
                if PlatformRepository::is_platform_package(req) {
                    continue;
                }
                autocompleter_values.insert(req.clone(), String::new());
            }
        }

        if 0 == autocompleter_values.len() {
            return Err(RuntimeException {
                message: "Could not find any package with new versions available".to_string(),
                code: 0,
            }
            .into());
        }

        let packages: Vec<String> = io.select(
            "Select packages: (Select more than one value separated by comma) ".to_string(),
            autocompleter_values,
            false,
            1,
            "No package named \"%s\" is installed.".to_string(),
            true,
        );

        let mut table = Table::new(output);
        table.set_headers(vec!["Selected packages".to_string()]);
        for package in &packages {
            table.add_row(vec![package.clone()]);
        }
        table.render();

        if io.ask_confirmation(
            sprintf(
                "Would you like to continue and update the above package%s [<comment>yes</comment>]? ",
                &[PhpMixed::String(
                    if 1 == packages.len() { "" } else { "s" }.to_string(),
                )],
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

    fn create_version_selector(&self, composer: &Composer) -> VersionSelector {
        let mut repository_set = RepositorySet::new();
        repository_set.add_repository(Box::new(CompositeRepository::new(array_filter(
            &composer.get_repository_manager().get_repositories(),
            |repository: &Box<dyn RepositoryInterface>| -> bool {
                // PHP: !$repository instanceof PlatformRepository
                repository
                    .as_any()
                    .downcast_ref::<PlatformRepository>()
                    .is_none()
            },
        ))));

        VersionSelector::new(repository_set)
    }
}

impl BaseCommand for UpdateCommand {
    fn inner(&self) -> &Command {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Command {
        &mut self.inner
    }

    fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    fn composer_mut(&mut self) -> &mut Option<Composer> {
        &mut self.composer
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.io.as_deref()
    }

    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>> {
        &mut self.io
    }
}
