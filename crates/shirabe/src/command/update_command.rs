//! ref: composer/src/Composer/Command/UpdateCommand.php

use crate::io::io_interface;
use crate::package::base_package;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::component::console::helper::Table;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, array_filter, array_intersect,
    array_keys, array_merge, array_search, count, empty, in_array, sprintf, strtolower,
};
use shirabe_semver::constraint::MultiConstraint;
use shirabe_semver::intervals::Intervals;

use crate::advisory::Auditor;
use crate::command::BumpCommand;
use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer::PartialComposerHandle;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::dependency_resolver::request::{self, Request, UpdateAllowTransitiveDeps};
use crate::installer::Installer;
use crate::io::IOInterface;
use crate::package::BasePackage;
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

impl UpdateCommand {
    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_installed_package(false, true) / suggest_prefer_install
        self
            .set_name("update")
            .set_aliases(&["u".to_string(), "upgrade".to_string()])
            .set_description("Updates your dependencies to the latest version according to composer.json, and updates the composer.lock file")
            // TODO(phase-b): set_definition with InputArgument/InputOption (see PHP UpdateCommand)
            .set_definition(&[])
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
        // TODO(phase-b): clone_box avoids the &mut self conflict with require_composer
        // below; revisit when get_io can return an Rc/Arc owned handle.
        let io_box = self.get_io().clone_box();
        let io: &dyn IOInterface = &*io_box;
        if input.get_option("dev").as_bool().unwrap_or(false) {
            io.write_error3(
                "<warning>You are using the deprecated option \"--dev\". It has no effect and will break in Composer 3.</warning>",
                true,
                io_interface::NORMAL,
            );
        }
        if input.get_option("no-suggest").as_bool().unwrap_or(false) {
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
            .get_argument("packages")
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let mut reqs: IndexMap<String, String> = self.format_requirements(
            input
                .get_option("with")
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
        )?;

        // extract --with shorthands from the allowlist
        if packages.len() > 0 {
            let allowlist_packages_with_requirements: Vec<String> =
                array_filter(&packages, |pkg: &String| -> bool {
                    Preg::is_match(r"{\S+[ =:]\S+}", pkg).unwrap_or(false)
                });
            for (package, constraint) in
                self.format_requirements(allowlist_packages_with_requirements.clone())?
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
        // TODO(phase-b): composer.get_package() returns &dyn RootPackageInterface so
        // set_references/set_stability_flags cannot be called; needs &mut access.
        let references =
            RootPackageLoader::extract_references(&reqs, root_package.get_references().clone());
        let stability_flags = RootPackageLoader::extract_stability_flags(
            &reqs,
            root_package.get_minimum_stability(),
            root_package.get_stability_flags().clone(),
        );
        let _ = references;
        let _ = stability_flags;

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
            temporary_constraints.insert(package.clone(), parsed_constraint);
            // TODO(phase-b): access root_requirements[package].getConstraint()
            let intersected: bool = todo!("Intervals::haveIntersections check");
            if let Some(_root_req) = todo!("root_requirements.get(&package)") as Option<PhpMixed> {
                if !intersected {
                    io.write_error3(
                        &format!(
                            "<error>The temporary constraint \"{}\" for \"{}\" must be a subset of the constraint in your composer.json ({})</error>",
                            constraint,
                            package,
                            todo!("root_requirements[package].get_pretty_constraint()"),
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
        }

        if input.get_option("patch-only").as_bool().unwrap_or(false) {
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
                        .map(|c| c.clone())
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
            packages =
                self.get_packages_interactively(io, input, output, &composer_handle, packages)?;
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
            io.write_error3("<error>You cannot simultaneously update only a selection of packages and regenerate the lock file metadata.</error>", true, io_interface::NORMAL);

            return Ok(-1);
        }

        let mut command_event = CommandEvent::new(PluginEvents::COMMAND, "update", input, output);
        // TODO(phase-b): dispatch should accept the CommandEvent itself; passing the
        // event by name only for now to keep types aligned with EventDispatcher::dispatch.
        composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch(Some(command_event.get_name()), None)?;

        composer
            .get_installation_manager()
            .borrow_mut()
            .set_output_progress(!input.get_option("no-progress").as_bool().unwrap_or(false));

        let mut install = Installer::create(io.clone_box(), &composer_handle);

        let config = composer.get_config();
        let (prefer_source, prefer_dist) =
            self.get_preferred_install_options(&*config.borrow(), input, false)?;

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
        let apcu_prefix: Option<String> = input
            .get_option("apcu-autoloader-prefix")
            .as_string()
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
        let minimal_changes = input
            .get_option("minimal-changes")
            .as_bool()
            .unwrap_or(false)
            || config
                .borrow_mut()
                .get("update-with-minimal-changes")
                .as_bool()
                .unwrap_or(false);

        let mut update_allow_transitive_dependencies: i64 = Request::UPDATE_ONLY_LISTED;
        if input
            .get_option("with-all-dependencies")
            .as_bool()
            .unwrap_or(false)
        {
            update_allow_transitive_dependencies = Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS;
        } else if input
            .get_option("with-dependencies")
            .as_bool()
            .unwrap_or(false)
        {
            update_allow_transitive_dependencies =
                Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS_NO_ROOT_REQUIRE;
        }
        // Keep `UpdateAllowTransitiveDeps` import alive while still using i64 for the setter.
        let _ = UpdateAllowTransitiveDeps::UpdateOnlyListed;

        install
            .set_dry_run(input.get_option("dry-run").as_bool().unwrap_or(false))
            .set_verbose(input.get_option("verbose").as_bool().unwrap_or(false))
            .set_prefer_source(prefer_source)
            .set_prefer_dist(prefer_dist)
            .set_dev_mode(!input.get_option("no-dev").as_bool().unwrap_or(false))
            .set_dump_autoloader(!input.get_option("no-autoloader").as_bool().unwrap_or(false))
            .set_optimize_autoloader(optimize)
            .set_class_map_authoritative(authoritative)
            .set_apcu_autoloader(apcu, apcu_prefix.clone())
            .set_update(true)
            .set_install(!input.get_option("no-install").as_bool().unwrap_or(false))
            .set_update_mirrors(update_mirrors)
            .set_update_allow_list(packages.clone())
            .set_update_allow_transitive_dependencies(update_allow_transitive_dependencies)?
            .set_platform_requirement_filter(self.get_platform_requirement_filter(input)?)
            .set_prefer_stable(input.get_option("prefer-stable").as_bool().unwrap_or(false))
            .set_prefer_lowest(input.get_option("prefer-lowest").as_bool().unwrap_or(false))
            // TODO(phase-b): VersionParser::parse_constraints returns Arc<dyn ...> but
            // Installer::set_temporary_constraints expects IndexMap<String, Box<dyn ...>>;
            // bridge the constraint storage types later.
            .set_temporary_constraints({
                let _ = &temporary_constraints;
                IndexMap::new()
            })
            .set_audit_config(
                self.create_audit_config(&mut *composer.get_config().borrow_mut(), input)?,
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
                bump_after_update = composer.get_config().borrow().get("bump-after-update");
            }

            if !matches!(bump_after_update, PhpMixed::Bool(false)) {
                io.write_error3(
                    "<info>Bumping dependencies</info>",
                    true,
                    io_interface::NORMAL,
                );
                let mut bump_command = BumpCommand::new(None);
                // TODO(phase-b): Composer is a PHP class shared by reference; calling
                // set_composer here requires a shared PartialComposer handle.
                // bump_command.set_composer(composer);
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
        composer: &PartialComposerHandle,
        packages: Vec<String>,
    ) -> Result<Vec<String>> {
        if !input.is_interactive() {
            return Err(InvalidArgumentException {
                message: "--interactive cannot be used in non-interactive terminals.".to_string(),
                code: 0,
            }
            .into());
        }

        let composer_ref = crate::command::composer_full(composer);
        let platform_req_filter = self.get_platform_requirement_filter(input);
        let stability_flags = composer_ref.get_package().get_stability_flags();
        let requires = array_merge(
            // TODO(phase-b): array_merge for IndexMap<String, Link>
            todo!("composer.get_package().get_requires() as PhpMixed"),
            todo!("composer.get_package().get_dev_requires() as PhpMixed"),
        );

        let filter: Option<String> = if packages.len() > 0 {
            // TODO(phase-b): base_package::package_names_to_regexp signature
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
        // TODO(phase-b): unify return types — CanonicalPackagesTrait returns
        // Vec<Box<dyn PackageInterface>> while RepositoryInterface::get_packages
        // returns Vec<Box<dyn BasePackage>>. Use only the locker branch for now.
        let installed_packages: Vec<Box<dyn crate::package::PackageInterface>> =
            if composer_ref.get_locker().borrow_mut().is_locked() {
                CanonicalPackagesTrait::get_packages(
                    &composer_ref
                        .get_locker()
                        .borrow_mut()
                        .get_locked_repository(true)?,
                )
            } else {
                let _ = composer_ref
                    .get_repository_manager()
                    .borrow()
                    .get_local_repository()
                    .get_packages();
                Vec::new()
            };
        let mut version_selector = self.create_version_selector(composer)?;
        for package in &installed_packages {
            if let Some(filter) = &filter {
                if !Preg::is_match(filter, package.get_name()).unwrap_or(false) {
                    continue;
                }
            }
            let current_version = package.get_pretty_version();
            // TODO(phase-b): pull from requires[package.get_name()].get_pretty_constraint()
            let constraint: Option<&str> = None;
            // TODO(phase-b): derive from stabilityFlags / minimum_stability
            let stability: &str = "stable";
            let latest_version = version_selector.find_best_candidate(
                package.get_name(),
                constraint,
                stability,
                None,
                0,
                None,
                PhpMixed::Bool(true),
            )?;
            let _ = &platform_req_filter;
            let _ = &stability_flags;
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
            // TODO(phase-b): iterate composer.get_package().get_requires() merged with
            // get_dev_requires(); requires is currently a PhpMixed placeholder.
            let _ = &requires;
            let _empty: IndexMap<String, ()> = IndexMap::new();
            for (req, _constraint) in &_empty {
                if PlatformRepository::is_platform_package(req) {
                    continue;
                }
                autocompleter_values.insert(req.to_string(), String::new());
            }
        }

        if 0 == autocompleter_values.len() {
            return Err(RuntimeException {
                message: "Could not find any package with new versions available".to_string(),
                code: 0,
            }
            .into());
        }

        // TODO(phase-b): IOInterface::select returns PhpMixed and takes
        // Vec<String> choices; convert IndexMap<String, String> autocompleter values
        // to choices and downcast PhpMixed back to Vec<String>.
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
            table.add_row(PhpMixed::List(vec![Box::new(PhpMixed::String(
                package.clone(),
            ))]));
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

    fn create_version_selector(&self, composer: &PartialComposerHandle) -> Result<VersionSelector> {
        let composer = crate::command::composer_full(composer);
        let mut repository_set = RepositorySet::new(
            composer.get_package().get_minimum_stability(),
            composer.get_package().get_stability_flags().clone(),
            // TODO(phase-b): collect root aliases from composer.get_package().get_aliases()
            Vec::new(),
            composer.get_package().get_references().clone(),
            IndexMap::new(),
            IndexMap::new(),
        );
        // TODO(phase-b): array_filter requires Clone on Box<dyn RepositoryInterface>
        // which PHP classes must not implement. Skipping the repo filter for now.
        let _ = &composer
            .get_repository_manager()
            .borrow()
            .get_repositories();
        let _ = |repository: &Box<dyn RepositoryInterface>| -> bool {
            repository
                .as_any()
                .downcast_ref::<PlatformRepository>()
                .is_none()
        };
        repository_set.add_repository(Box::new(CompositeRepository::new(Vec::new())))?;
        let _ = array_filter::<i64, fn(&i64) -> bool>;

        VersionSelector::new(repository_set, None)
    }
}

impl HasBaseCommandData for UpdateCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
