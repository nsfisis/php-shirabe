//! ref: composer/src/Composer/Command/RequireCommand.php

use crate::advisory::Auditor;
use crate::command::PackageDiscoveryTrait;
use crate::command::base_command::base_command_initialize;
use crate::command::{BaseCommand, BaseCommandData};
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::dependency_resolver::UpdateAllowTransitiveDeps;
use crate::factory::Factory;
use crate::installer::Installer;
use crate::installer::InstallerEvents;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::io_interface;
use crate::json::JsonFile;
use crate::json::JsonManipulator;
use crate::package::base_package;
use crate::package::loader::ArrayLoader;
use crate::package::loader::RootPackageLoader;
use crate::package::version::VersionParser;
use crate::package::version::VersionSelector;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::repository::CompositeRepository;
use crate::repository::PlatformRepository;
use crate::repository::PlatformRepositoryHandle;
use crate::repository::RepositorySet;
use crate::util::Filesystem;
use crate::util::PackageSorter;
use crate::util::Silencer;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::seld::signal::SignalHandler;
use shirabe_external_packages::symfony::console::command::command::Command;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, array_fill_keys, array_intersect, array_keys, array_map,
    array_merge, array_unique, empty, file_exists, file_get_contents, file_put_contents, filesize,
    implode, is_writable, strtolower, unlink,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct RequireCommand {
    base_command_data: BaseCommandData,

    newly_created: std::cell::Cell<bool>,
    first_require: std::cell::Cell<bool>,
    json: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<JsonFile>>>>,
    file: std::cell::RefCell<String>,
    composer_backup: std::cell::RefCell<String>,
    /// file name
    lock: std::cell::RefCell<String>,
    /// contents before modification if the lock file exists
    lock_backup: std::cell::RefCell<Option<String>>,
    dependency_resolution_completed: std::cell::Cell<bool>,
    repos: std::cell::RefCell<Option<crate::repository::RepositoryInterfaceHandle>>,
    repository_sets:
        std::cell::RefCell<IndexMap<String, std::rc::Rc<std::cell::RefCell<RepositorySet>>>>,
}

impl Default for RequireCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl RequireCommand {
    pub fn new() -> Self {
        let command = RequireCommand {
            base_command_data: BaseCommandData::new(None),
            newly_created: std::cell::Cell::new(false),
            first_require: std::cell::Cell::new(false),
            json: std::cell::RefCell::new(None),
            file: std::cell::RefCell::new(String::new()),
            composer_backup: std::cell::RefCell::new(String::new()),
            lock: std::cell::RefCell::new(String::new()),
            lock_backup: std::cell::RefCell::new(None),
            dependency_resolution_completed: std::cell::Cell::new(false),
            repos: std::cell::RefCell::new(None),
            repository_sets: std::cell::RefCell::new(IndexMap::new()),
        };
        command
            .configure()
            .expect("RequireCommand::configure uses static, valid metadata");
        command
    }
}

impl PackageDiscoveryTrait for RequireCommand {
    fn get_repos_mut(
        &self,
    ) -> std::cell::RefMut<'_, Option<crate::repository::RepositoryInterfaceHandle>> {
        self.repos.borrow_mut()
    }

    fn get_repository_sets_mut(
        &self,
    ) -> std::cell::RefMut<'_, IndexMap<String, std::rc::Rc<std::cell::RefCell<RepositorySet>>>>
    {
        self.repository_sets.borrow_mut()
    }
}

impl Command for RequireCommand {
    fn configure(&self) -> anyhow::Result<()> {
        // TODO(cli-completion): suggest_available_package_incl_platform / suggest_prefer_install
        self.set_name("require")?;
        self.set_aliases(vec!["r".to_string()])?;
        self.set_description("Adds required packages to your composer.json and installs them");
        self.set_definition(&[
            InputArgument::new("packages", Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL), "Optional package name can also include a version constraint, e.g. foo/bar or foo/bar:1.0.0 or foo/bar=1.0.0 or \"foo/bar 1.0.0\"", None).unwrap().into(),
            InputOption::new("dev", None, Some(InputOption::VALUE_NONE), "Add requirement to require-dev.", None).unwrap().into(),
            InputOption::new("dry-run", None, Some(InputOption::VALUE_NONE), "Outputs the operations but will not execute anything (implicitly enables --verbose).", None).unwrap().into(),
            InputOption::new("prefer-source", None, Some(InputOption::VALUE_NONE), "Forces installation from package sources when possible, including VCS information.", None).unwrap().into(),
            InputOption::new("prefer-dist", None, Some(InputOption::VALUE_NONE), "Forces installation from package dist (default behavior).", None).unwrap().into(),
            InputOption::new("prefer-install", None, Some(InputOption::VALUE_REQUIRED), "Forces installation from package dist|source|auto (auto chooses source for dev versions, dist for the rest).", None).unwrap().into(),
            InputOption::new("fixed", None, Some(InputOption::VALUE_NONE), "Write fixed version to the composer.json.", None).unwrap().into(),
            InputOption::new("no-suggest", None, Some(InputOption::VALUE_NONE), "DEPRECATED: This flag does not exist anymore.", None).unwrap().into(),
            InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None).unwrap().into(),
            InputOption::new("no-update", None, Some(InputOption::VALUE_NONE), "Disables the automatic update of the dependencies (implies --no-install).", None).unwrap().into(),
            InputOption::new("no-install", None, Some(InputOption::VALUE_NONE), "Skip the install step after updating the composer.lock file.", None).unwrap().into(),
            InputOption::new("no-audit", None, Some(InputOption::VALUE_NONE), "Skip the audit step after updating the composer.lock file (can also be set via the COMPOSER_NO_AUDIT=1 env var).", None).unwrap().into(),
            InputOption::new("audit-format", None, Some(InputOption::VALUE_REQUIRED), "Audit output format. Must be \"table\", \"plain\", \"json\", or \"summary\".", Some(PhpMixed::String(Auditor::FORMAT_SUMMARY.to_string()))).unwrap().into(),
            InputOption::new("no-security-blocking", None, Some(InputOption::VALUE_NONE), "Allows installing packages with security advisories or that are abandoned (can also be set via the COMPOSER_NO_SECURITY_BLOCKING=1 env var).", None).unwrap().into(),
            InputOption::new("update-no-dev", None, Some(InputOption::VALUE_NONE), "Run the dependency update with the --no-dev option.", None).unwrap().into(),
            InputOption::new("update-with-dependencies", Some(PhpMixed::String("w".to_string())), Some(InputOption::VALUE_NONE), "Allows inherited dependencies to be updated, except those that are root requirements (can also be set via the COMPOSER_WITH_DEPENDENCIES=1 env var).", None).unwrap().into(),
            InputOption::new("update-with-all-dependencies", Some(PhpMixed::String("W".to_string())), Some(InputOption::VALUE_NONE), "Allows all inherited dependencies to be updated, including those that are root requirements (can also be set via the COMPOSER_WITH_ALL_DEPENDENCIES=1 env var).", None).unwrap().into(),
            InputOption::new("with-dependencies", None, Some(InputOption::VALUE_NONE), "Alias for --update-with-dependencies", None).unwrap().into(),
            InputOption::new("with-all-dependencies", None, Some(InputOption::VALUE_NONE), "Alias for --update-with-all-dependencies", None).unwrap().into(),
            InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None).unwrap().into(),
            InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None).unwrap().into(),
            InputOption::new("prefer-stable", None, Some(InputOption::VALUE_NONE), "Prefer stable versions of dependencies (can also be set via the COMPOSER_PREFER_STABLE=1 env var).", None).unwrap().into(),
            InputOption::new("prefer-lowest", None, Some(InputOption::VALUE_NONE), "Prefer lowest versions of dependencies (can also be set via the COMPOSER_PREFER_LOWEST=1 env var).", None).unwrap().into(),
            InputOption::new("minimal-changes", Some(PhpMixed::String("m".to_string())), Some(InputOption::VALUE_NONE), "During an update with -w/-W, only perform absolutely necessary changes to transitive dependencies (can also be set via the COMPOSER_MINIMAL_CHANGES=1 env var).", None).unwrap().into(),
            InputOption::new("sort-packages", None, Some(InputOption::VALUE_NONE), "Sorts packages when adding/updating a new dependency", None).unwrap().into(),
            InputOption::new("optimize-autoloader", Some(PhpMixed::String("o".to_string())), Some(InputOption::VALUE_NONE), "Optimize autoloader during autoloader dump", None).unwrap().into(),
            InputOption::new("classmap-authoritative", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Autoload classes from the classmap only. Implicitly enables `--optimize-autoloader`.", None).unwrap().into(),
            InputOption::new("apcu-autoloader", None, Some(InputOption::VALUE_NONE), "Use APCu to cache found/not-found classes.", None).unwrap().into(),
            InputOption::new("apcu-autoloader-prefix", None, Some(InputOption::VALUE_REQUIRED), "Use a custom prefix for the APCu autoloader cache. Implicitly enables --apcu-autoloader", None).unwrap().into(),
        ]);
        self.set_help(
            "The require command adds required packages to your composer.json and installs them.\n\
            \n\
            If you do not specify a package, composer will prompt you to search for a package, and given results, provide a list of\n\
            matches to require.\n\
            \n\
            If you do not specify a version constraint, composer will choose a suitable one based on the available package versions.\n\
            \n\
            If you do not want to install the new dependencies immediately you can call it with --no-update\n\
            \n\
            Read more at https://getcomposer.org/doc/03-cli.md#require-r"
        );
        Ok(())
    }

    /// @throws \Seld\JsonLint\ParsingException
    fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        *self.file.borrow_mut() = Factory::get_composer_file()?;

        if input
            .borrow()
            .get_option("no-suggest")?
            .as_bool()
            .unwrap_or(false)
        {
            self.get_io().write_error3("<warning>You are using the deprecated option \"--no-suggest\". It has no effect and will break in Composer 3.</warning>", true, io_interface::NORMAL);
        }

        let file = self.file.borrow().clone();
        self.newly_created.set(!file_exists(&file));
        let write_failed =
            self.newly_created.get() && file_put_contents(&file, b"{\n}\n").is_none();
        if write_failed {
            let msg = format!("<error>{} could not be created.</error>", file);
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);

            return Ok(1);
        }
        if !Filesystem::is_readable(&file) {
            let msg = format!("<error>{} is not readable.</error>", file);
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);

            return Ok(1);
        }
        if filesize(&file) == Some(0) {
            file_put_contents(&file, b"{\n}\n");
        }

        *self.json.borrow_mut() = Some(std::rc::Rc::new(std::cell::RefCell::new(JsonFile::new(
            file.clone(),
            None,
            None,
        )?)));
        *self.lock.borrow_mut() = Factory::get_lock_file(&file);
        let json = self.json.borrow().as_ref().unwrap().clone();
        *self.composer_backup.borrow_mut() =
            file_get_contents(json.borrow().get_path()).unwrap_or_default();
        let lock = self.lock.borrow().clone();
        *self.lock_backup.borrow_mut() = if file_exists(&lock) {
            file_get_contents(&lock)
        } else {
            None
        };

        // PHP: function ($signal, $handler) use ($io, $self) {
        //   $io->writeError('Received '.$signal.', aborting', true, IOInterface::DEBUG);
        //   $self->revertComposerFile(); $handler->exitWithLastSignal(); }
        // TODO(phase-c): SignalHandler::create takes a `Box<dyn Fn> + 'static` handler that cannot
        // borrow &self, but the body must call self.revert_composer_file() (which mutates the
        // command's composer.json backup state) and self.get_io(). Faithfully wiring this needs the
        // revert state + io shared into the closure (Rc<RefCell<...>>), i.e. the shared-ownership
        // rework of the command — the same pattern as InstallationManager::execute's signal handler.
        let signal_handler = SignalHandler::create(
            vec![
                SignalHandler::SIGINT.to_string(),
                SignalHandler::SIGTERM.to_string(),
                SignalHandler::SIGHUP.to_string(),
            ],
            Box::new(move |signal: String, handler: &SignalHandler| {
                let _ = signal;
                handler.exit_with_last_signal();
            }),
        );

        // check for writability by writing to the file as is_writable can not be trusted on network-mounts
        // see https://github.com/composer/composer/issues/8231 and https://bugs.php.net/bug.php?id=68926
        let file_path = file.clone();
        let backup_contents = self.composer_backup.borrow().clone();
        if !is_writable(&file)
            && Silencer::call(|| {
                shirabe_php_shim::file_put_contents(&file_path, backup_contents.as_bytes());
                Ok::<bool, anyhow::Error>(false)
            })
            .ok()
                == Some(false)
        {
            let msg = format!("<error>{} is not writable.</error>", file);
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);

            return Ok(1);
        }

        if input.borrow().get_option("fixed")?.as_bool() == Some(true) {
            let config = json.borrow_mut().read()?;

            let package_type = if empty(&config.get("type").cloned().unwrap_or(PhpMixed::Null)) {
                "library".to_string()
            } else {
                config
                    .get("type")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string()
            };

            // @see https://github.com/composer/composer/pull/8313#issuecomment-532637955
            if package_type != "project"
                && !input.borrow().get_option("dev")?.as_bool().unwrap_or(false)
            {
                self.get_io().write_error3("<error>The \"--fixed\" option is only allowed for packages with a \"project\" type or for dev dependencies to prevent possible misuses.</error>", true, io_interface::NORMAL);

                if config.get("type").is_none() {
                    self.get_io().write_error3("<error>If your package is not a library, you can explicitly specify the \"type\" by using \"composer config type project\".</error>", true, io_interface::NORMAL);
                }

                return Ok(1);
            }
        }

        let composer = self.require_composer(None, None)?;
        let composer = crate::composer::composer_full(&composer);
        let repository_manager = composer.get_repository_manager().clone();
        let repository_manager = repository_manager.borrow();
        let repos = repository_manager.get_repositories();

        let platform_overrides = composer.get_config().borrow_mut().get("platform");
        let platform_overrides_map: IndexMap<String, PhpMixed> = platform_overrides
            .as_array()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();
        // initialize self.repos as it is used by the PackageDiscoveryTrait
        let platform_repo =
            PlatformRepositoryHandle::new(PlatformRepository::new(vec![], platform_overrides_map)?);
        let mut combined: Vec<crate::repository::RepositoryInterfaceHandle> =
            vec![platform_repo.clone().into()];
        for repo in repos {
            combined.push(repo.clone());
        }
        *self.get_repos_mut() = Some(crate::repository::RepositoryInterfaceHandle::new(
            CompositeRepository::new(combined),
        ));

        let preferred_stability = if composer.get_package().get_prefer_stable() {
            "stable".to_string()
        } else {
            composer.get_package().get_minimum_stability().to_string()
        };

        let requirements_result = self.determine_requirements(
            input.clone(),
            output.clone(),
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
            Some(&platform_repo),
            &preferred_stability,
            // if there is no update, we need to use the best possible version constraint directly as we cannot rely on the solver to guess the best constraint
            input
                .borrow()
                .get_option("no-update")?
                .as_bool()
                .unwrap_or(false),
            input
                .borrow()
                .get_option("fixed")?
                .as_bool()
                .unwrap_or(false),
        );

        let requirements = match requirements_result {
            Ok(r) => r,
            Err(e) => {
                if self.newly_created.get() {
                    self.revert_composer_file();

                    return Err(RuntimeException {
                        message: format!(
                            "No composer.json present in the current directory ({}), this may be the cause of the following exception.",
                            self.file.borrow()
                        ),
                        code: 0,
                    }
                    .into());
                }

                return Err(e);
            }
        };

        let mut requirements = self.format_requirements(requirements)?;

        if !input.borrow().get_option("dev")?.as_bool().unwrap_or(false)
            && self.get_io().is_interactive()
            && !composer.is_global()
        {
            let mut dev_packages: Vec<Vec<String>> = vec![];
            let dev_tags: Vec<String> = vec![
                "dev".to_string(),
                "testing".to_string(),
                "static analysis".to_string(),
            ];
            let current_requires_by_key = self.get_packages_by_require_key();
            for (name, _version) in &requirements {
                // skip packages which are already in the composer.json as those have already been decided
                if current_requires_by_key.contains_key(name) {
                    continue;
                }

                let found_packages: Vec<crate::package::PackageInterfaceHandle> = self
                    .get_repos()
                    .find_packages(name, None)?
                    .into_iter()
                    .collect();
                let pkg: Option<crate::package::PackageInterfaceHandle> =
                    PackageSorter::get_most_current_version(found_packages);
                let pkg_as_complete: Option<crate::package::CompletePackageInterfaceHandle> =
                    pkg.as_ref().and_then(|p| p.as_complete());
                if let Some(pkg_complete) = pkg_as_complete {
                    let lowered: Vec<String> =
                        array_map(|s: &String| strtolower(s), &pkg_complete.get_keywords());
                    let pkg_dev_tags: Vec<String> = array_intersect(&dev_tags, &lowered);
                    if (pkg_dev_tags.len() as i64) > 0 {
                        dev_packages.push(pkg_dev_tags);
                    }
                }
                let _ = pkg;
            }

            if (dev_packages.len() as i64) == (requirements.len() as i64) {
                let plural = if (requirements.len() as i64) > 1 {
                    "s"
                } else {
                    ""
                };
                let plural2 = if (requirements.len() as i64) > 1 {
                    "are"
                } else {
                    "is"
                };
                let plural3 = if (requirements.len() as i64) > 1 {
                    "they are"
                } else {
                    "it is"
                };
                let merged: Vec<String> = dev_packages.iter().flatten().cloned().collect();
                let pkg_dev_tags: Vec<String> = array_unique(&merged);
                let warn_msg = format!(
                    "The package{} you required {} recommended to be placed in require-dev (because {} tagged as \"{}\") but you did not use --dev.",
                    plural,
                    plural2,
                    plural3,
                    implode("\", \"", &pkg_dev_tags),
                );
                self.get_io().warning(&warn_msg, &[]);
                if self.get_io().ask_confirmation(
                    "<info>Do you want to re-run the command with --dev?</> [<comment>yes</>]? "
                        .to_string(),
                    true,
                ) {
                    input.borrow_mut().set_option("dev", PhpMixed::Bool(true))?;
                }
            }

            // unset($devPackages, $pkgDevTags);
        }

        let mut require_key = if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
            "require-dev"
        } else {
            "require"
        };
        let mut remove_key = if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
            "require"
        } else {
            "require-dev"
        };

        // check which requirements need the version guessed
        let mut requirements_to_guess: Vec<String> = vec![];
        for (package, constraint) in requirements.clone().iter() {
            if constraint == "guess" {
                requirements.insert(package.clone(), "*".to_string());
                requirements_to_guess.push(package.clone());
            }
        }

        // validate requirements format
        let version_parser = VersionParser::new();
        for (package, constraint) in &requirements {
            if strtolower(package) == composer.get_package().get_name() {
                let msg = format!(
                    "<error>Root package '{}' cannot require itself in its composer.json</error>",
                    package.clone(),
                );
                self.get_io().write_error3(&msg, true, io_interface::NORMAL);

                return Ok(1);
            }
            if constraint == "self.version" {
                continue;
            }
            version_parser.parse_constraints(constraint)?;
        }

        let inconsistent_require_keys =
            self.get_inconsistent_require_keys(&requirements, require_key);
        if (inconsistent_require_keys.len() as i64) > 0 {
            for package in &inconsistent_require_keys {
                let warn_msg = format!(
                    "{} is currently present in the {} key and you ran the command {} the --dev flag, which will move it to the {} key.",
                    package.clone(),
                    remove_key,
                    if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
                        "with"
                    } else {
                        "without"
                    },
                    require_key,
                );
                self.get_io().warning(&warn_msg, &[]);
            }

            if self.get_io().is_interactive() {
                let q1 = format!(
                    "<info>Do you want to move {}?</info> [<comment>no</comment>]? ",
                    if (inconsistent_require_keys.len() as i64) > 1 {
                        "these requirements"
                    } else {
                        "this requirement"
                    },
                );
                if !self.get_io().ask_confirmation(q1, false) {
                    let q2 = format!(
                        "<info>Do you want to re-run the command {} --dev?</info> [<comment>yes</comment>]? ",
                        if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
                            "without"
                        } else {
                            "with"
                        },
                    );
                    if !self.get_io().ask_confirmation(q2, true) {
                        return Ok(0);
                    }

                    input.borrow_mut().set_option("dev", PhpMixed::Bool(true))?;
                    std::mem::swap(&mut require_key, &mut remove_key);
                }
            }
        }

        let sort_packages = input
            .borrow()
            .get_option("sort-packages")?
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get("sort-packages")
                .as_bool()
                .unwrap_or(false);

        self.first_require.set(self.newly_created.get());
        if !self.first_require.get() {
            let composer_definition = json.borrow_mut().read()?;
            let require_count = composer_definition
                .get("require")
                .and_then(|v| v.as_array())
                .map(|m| m.len() as i64)
                .unwrap_or(0);
            let require_dev_count = composer_definition
                .get("require-dev")
                .and_then(|v| v.as_array())
                .map(|m| m.len() as i64)
                .unwrap_or(0);
            if require_count == 0 && require_dev_count == 0 {
                self.first_require.set(true);
            }
        }

        if !input
            .borrow()
            .get_option("dry-run")?
            .as_bool()
            .unwrap_or(false)
        {
            self.update_file(&json, &requirements, require_key, remove_key, sort_packages);
        }

        let updated_msg = format!(
            "<info>{} has been {}</info>",
            file,
            if self.newly_created.get() {
                "created"
            } else {
                "updated"
            }
        );
        self.get_io()
            .write_error3(&updated_msg, true, io_interface::NORMAL);

        if input
            .borrow()
            .get_option("no-update")?
            .as_bool()
            .unwrap_or(false)
        {
            return Ok(0);
        }

        composer
            .get_plugin_manager()
            .borrow_mut()
            .deactivate_installed_plugins()?;

        let io = self.get_io().clone();
        let do_update_result = self.do_update(
            input.clone(),
            output,
            io,
            &requirements,
            require_key,
            remove_key,
        );
        let dry_run = input
            .borrow()
            .get_option("dry-run")?
            .as_bool()
            .unwrap_or(false);

        let result = match do_update_result {
            Ok(result) => {
                let final_result = if result == 0 && (requirements_to_guess.len() as i64) > 0 {
                    self.update_requirements_after_resolution(
                        &requirements_to_guess,
                        require_key,
                        remove_key,
                        sort_packages,
                        dry_run,
                        input
                            .borrow()
                            .get_option("fixed")?
                            .as_bool()
                            .unwrap_or(false),
                    )?
                } else {
                    result
                };
                Ok(final_result)
            }
            Err(e) => {
                if !self.dependency_resolution_completed.get() {
                    self.revert_composer_file();
                }
                Err(e)
            }
        };

        // finally
        if dry_run && self.newly_created.get() {
            // @unlink($this->json->getPath());
            unlink(json.borrow().get_path());
        }
        signal_handler.unregister();

        result
    }

    fn interact(
        &self,
        _input: Rc<RefCell<dyn InputInterface>>,
        _output: Rc<RefCell<dyn OutputInterface>>,
    ) {
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

impl BaseCommand for RequireCommand {
    fn command_data(
        &self,
    ) -> &shirabe_external_packages::symfony::console::command::command::CommandData {
        self.base_command_data.command_data()
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}

impl RequireCommand {
    /// @param array<string, string> $newRequirements
    /// @return string[]
    fn get_inconsistent_require_keys(
        &self,
        new_requirements: &IndexMap<String, String>,
        require_key: &str,
    ) -> Vec<String> {
        let require_keys = self.get_packages_by_require_key();
        let mut inconsistent_requirements: Vec<String> = vec![];
        for (package, package_require_key) in &require_keys {
            if !new_requirements.contains_key(package) {
                continue;
            }
            if require_key != package_require_key {
                inconsistent_requirements.push(package.clone());
            }
        }

        inconsistent_requirements
    }

    /// @return array<string, string>
    fn get_packages_by_require_key(&self) -> IndexMap<String, String> {
        let json = self.json.borrow().as_ref().unwrap().clone();
        let composer_definition = json.borrow_mut().read().unwrap_or_default();
        let mut require: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut require_dev: IndexMap<String, PhpMixed> = IndexMap::new();

        if let Some(r) = composer_definition
            .get("require")
            .and_then(|v| v.as_array())
        {
            for (k, v) in r {
                require.insert(k.clone(), v.clone());
            }
        }

        if let Some(r) = composer_definition
            .get("require-dev")
            .and_then(|v| v.as_array())
        {
            for (k, v) in r {
                require_dev.insert(k.clone(), v.clone());
            }
        }

        array_merge(
            array_fill_keys(
                PhpMixed::List(
                    array_keys(&require)
                        .into_iter()
                        .map(PhpMixed::String)
                        .collect(),
                ),
                PhpMixed::String("require".to_string()),
            ),
            array_fill_keys(
                PhpMixed::List(
                    array_keys(&require_dev)
                        .into_iter()
                        .map(PhpMixed::String)
                        .collect(),
                ),
                PhpMixed::String("require-dev".to_string()),
            ),
        )
        .as_array()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_string().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
    }

    /// @param array<string, string> $requirements
    /// @param 'require'|'require-dev' $requireKey
    /// @param 'require'|'require-dev' $removeKey
    /// @throws \Exception
    fn do_update(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        requirements: &IndexMap<String, String>,
        require_key: &str,
        _remove_key: &str,
    ) -> anyhow::Result<i64> {
        // Update packages
        self.reset_composer()?;
        let composer_handle = self.require_composer(None, None)?;
        let composer = crate::composer::composer_full(&composer_handle);

        self.dependency_resolution_completed.set(false);
        // PHP: $composer->getEventDispatcher()->addListener(InstallerEvents::PRE_OPERATIONS_EXEC,
        //   function () use (&$dependencyResolutionCompleted) { $dependencyResolutionCompleted = true; }, 10000);
        // TODO(phase-c): the event dispatcher's Callable::Closure is a placeholder variant that
        // stores no actual closure, so the listener that flips dependency_resolution_completed
        // cannot be registered. Resolving needs the closure model (Callable holding an Rc<dyn Fn>)
        // plus dependency_resolution_completed shared (Rc<RefCell<bool>>) into both the listener
        // and this command.
        composer.get_event_dispatcher().borrow_mut().add_listener(
            InstallerEvents::PRE_OPERATIONS_EXEC,
            crate::event_dispatcher::Callable::Closure,
            10000,
        );

        if input
            .borrow()
            .get_option("dry-run")?
            .as_bool()
            .unwrap_or(false)
        {
            let root_package = composer.get_package();
            let mut links: IndexMap<String, IndexMap<String, crate::package::Link>> =
                IndexMap::new();
            links.insert("require".to_string(), root_package.get_requires());
            links.insert("require-dev".to_string(), root_package.get_dev_requires());
            let loader = ArrayLoader::new(None, false);
            let requirements_mixed: IndexMap<String, PhpMixed> = requirements
                .iter()
                .map(|(k, v)| (k.clone(), PhpMixed::String(v.clone())))
                .collect();
            let new_links = loader.parse_links(
                &root_package.get_name(),
                &root_package.get_pretty_version(),
                base_package::SUPPORTED_LINK_TYPES
                    .get(require_key)
                    .map(|t| t.method)
                    .unwrap_or_default(),
                requirements_mixed,
            )?;
            if let Some(section) = links.get_mut(require_key) {
                for (k, v) in new_links {
                    section.insert(k, v);
                }
            }
            for (package, _constraint) in requirements {
                if let Some(section) = links.get_mut(_remove_key) {
                    section.shift_remove(package);
                }
            }
            root_package.set_requires(links["require"].clone());
            root_package.set_dev_requires(links["require-dev"].clone());

            // extract stability flags & references as they weren't present when loading the unmodified composer.json
            let references = RootPackageLoader::extract_references(
                requirements,
                root_package.get_references().clone(),
            );
            root_package.set_references(references);
            let stability_flags = RootPackageLoader::extract_stability_flags(
                requirements,
                &root_package.get_minimum_stability(),
                root_package.get_stability_flags().clone(),
            );
            root_package.set_stability_flags(stability_flags);
        }

        let update_dev_mode = !input
            .borrow()
            .get_option("update-no-dev")?
            .as_bool()
            .unwrap_or(false);
        let optimize = input
            .borrow()
            .get_option("optimize-autoloader")?
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get("optimize-autoloader")
                .as_bool()
                .unwrap_or(false);
        let authoritative = input
            .borrow()
            .get_option("classmap-authoritative")?
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get("classmap-authoritative")
                .as_bool()
                .unwrap_or(false);
        let apcu_prefix = input
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
            || composer
                .get_config()
                .borrow()
                .get("apcu-autoloader")
                .as_bool()
                .unwrap_or(false);
        let minimal_changes = input
            .borrow()
            .get_option("minimal-changes")?
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get("update-with-minimal-changes")
                .as_bool()
                .unwrap_or(false);

        let mut update_allow_transitive_dependencies = UpdateAllowTransitiveDeps::UpdateOnlyListed;
        let mut flags = String::new();
        if input
            .borrow()
            .get_option("update-with-all-dependencies")?
            .as_bool()
            .unwrap_or(false)
            || input
                .borrow()
                .get_option("with-all-dependencies")?
                .as_bool()
                .unwrap_or(false)
        {
            update_allow_transitive_dependencies =
                UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDeps;
            flags += " --with-all-dependencies";
        } else if input
            .borrow()
            .get_option("update-with-dependencies")?
            .as_bool()
            .unwrap_or(false)
            || input
                .borrow()
                .get_option("with-dependencies")?
                .as_bool()
                .unwrap_or(false)
        {
            update_allow_transitive_dependencies =
                UpdateAllowTransitiveDeps::UpdateListedWithTransitiveDepsNoRootRequire;
            flags += " --with-dependencies";
        }

        io.write_error3(
            &format!(
                "<info>Running composer update {}{}</info>",
                implode(
                    " ",
                    &array_keys(requirements)
                        .into_iter()
                        .collect::<Vec<String>>()
                ),
                flags,
            ),
            true,
            io_interface::NORMAL,
        );

        let command_event =
            CommandEvent::new(PluginEvents::COMMAND, "require", input.clone(), output);
        composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch(Some(command_event.get_name()), None);

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

        let (prefer_source, prefer_dist) = self.get_preferred_install_options(
            &composer.get_config().borrow(),
            input.clone(),
            false,
        )?;

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
            .set_dev_mode(update_dev_mode)
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
            .set_update_allow_transitive_dependencies(update_allow_transitive_dependencies)?
            .set_platform_requirement_filter(BaseCommand::get_platform_requirement_filter(
                self,
                input.clone(),
            )?)
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
            .set_audit_config(
                self.create_audit_config(&mut composer.get_config().borrow_mut(), input.clone())?,
            )
            .set_minimal_update(minimal_changes);

        // if no lock is present, or the file is brand new, we do not do a
        // partial update as this is not supported by the Installer
        if !self.first_require.get() && composer.get_locker().borrow_mut().is_locked() {
            install.set_update_allow_list(
                array_keys(requirements)
                    .into_iter()
                    .collect::<Vec<String>>(),
            );
        }

        let status = install.run()?;
        if status != 0 && status != Installer::ERROR_AUDIT_FAILED {
            if status == Installer::ERROR_DEPENDENCY_RESOLUTION_FAILED {
                for req in BaseCommand::normalize_requirements(
                    self,
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
                )? {
                    if !req.contains_key("version") {
                        io.write_error3(&format!(
                            "You can also try re-running composer require with an explicit version constraint, e.g. \"composer require {}:*\" to figure out if any version is installable, or \"composer require {}:^2.1\" if you know which you need.",
                            req.get("name").cloned().unwrap_or_default(),
                            req.get("name").cloned().unwrap_or_default(),
                        ), true, io_interface::NORMAL);
                        break;
                    }
                }
            }
            self.revert_composer_file();
        }

        Ok(status)
    }

    /// @param list<string> $requirementsToUpdate
    fn update_requirements_after_resolution(
        &self,
        requirements_to_update: &[String],
        require_key: &str,
        remove_key: &str,
        sort_packages: bool,
        dry_run: bool,
        fixed: bool,
    ) -> anyhow::Result<i64> {
        let composer = self.require_composer(None, None)?;
        let composer = crate::composer::composer_full(&composer);
        let locker_is_locked = composer.get_locker().borrow_mut().is_locked();
        let mut requirements: IndexMap<String, String> = IndexMap::new();
        let mut version_selector = VersionSelector::new(
            std::rc::Rc::new(std::cell::RefCell::new(RepositorySet::new(
                "stable",
                IndexMap::new(),
                vec![],
                IndexMap::new(),
                IndexMap::new(),
                IndexMap::new(),
            ))),
            None,
        )?;
        let repo: crate::repository::RepositoryInterfaceHandle = if locker_is_locked {
            composer
                .get_locker()
                .borrow_mut()
                .get_locked_repository(true)?
                .into()
        } else {
            composer
                .get_repository_manager()
                .borrow()
                .get_local_repository()
        };
        for package_name in requirements_to_update {
            let mut package = repo.find_package(
                package_name,
                crate::repository::FindPackageConstraint::String("*".to_string()),
            )?;
            while let Some(alias) = package.as_ref().and_then(|p| p.as_alias()) {
                package = Some(alias.get_alias_of().into());
            }

            let package = match package {
                Some(p) => p,
                None => continue,
            };

            if fixed {
                requirements.insert(package_name.clone(), package.get_pretty_version());
            } else {
                requirements.insert(
                    package_name.clone(),
                    version_selector.find_recommended_require_version(package.clone())?,
                );
            }
            self.get_io().write_error3(
                &format!(
                    "Using version <info>{}</info> for <info>{}</info>",
                    requirements.get(package_name).cloned().unwrap_or_default(),
                    package_name.clone(),
                ),
                true,
                io_interface::NORMAL,
            );

            if Preg::is_match(
                r"{^dev-(?!main$|master$|trunk$|latest$)}",
                requirements
                    .get(package_name)
                    .map(|s| s.as_str())
                    .unwrap_or(""),
            ) {
                self.get_io().warning(
                    &format!(
                        "Version {} looks like it may be a feature branch which is unlikely to keep working in the long run and may be in an unstable state",
                        requirements.get(package_name).cloned().unwrap_or_default(),
                    ),
                    &[],
                );
                if self.get_io().is_interactive()
                    && !self.get_io().ask_confirmation(
                        "Are you sure you want to use this constraint (<comment>y</comment>) or would you rather abort (<comment>n</comment>) the whole operation [<comment>y,n</comment>]? "
                            .to_string(),
                        true,
                    )
                {
                    self.revert_composer_file();

                    return Ok(1);
                }
            }
        }

        if !dry_run {
            let json = self.json.borrow().as_ref().unwrap().clone();
            self.update_file(&json, &requirements, require_key, remove_key, sort_packages);
            if locker_is_locked
                && composer
                    .get_config()
                    .borrow_mut()
                    .get("lock")
                    .as_bool()
                    .unwrap_or(false)
            {
                let stability_flags = RootPackageLoader::extract_stability_flags(
                    &requirements,
                    &composer.get_package().get_minimum_stability(),
                    IndexMap::new(),
                );
                composer.get_locker().borrow_mut().update_hash(
                    &json.borrow(),
                    Some(Box::new(move |mut lock_data| {
                        let section = lock_data
                            .entry("stability-flags".to_string())
                            .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
                        if let Some(section) = section.as_array_mut() {
                            for (package_name, flag) in &stability_flags {
                                section.insert(package_name.clone(), PhpMixed::Int(*flag));
                            }
                        }
                        lock_data
                    })),
                )?;
            }
        }

        Ok(0)
    }

    /// @param array<string, string> $new
    fn update_file(
        &self,
        json: &std::rc::Rc<std::cell::RefCell<JsonFile>>,
        new: &IndexMap<String, String>,
        require_key: &str,
        remove_key: &str,
        sort_packages: bool,
    ) {
        if self.update_file_cleanly(json, new, require_key, remove_key, sort_packages) {
            return;
        }

        let composer_definition_mixed = json.borrow_mut().read().unwrap_or_default();
        let mut composer_definition = composer_definition_mixed
            .as_array()
            .cloned()
            .unwrap_or_default();
        for (package, version) in new {
            let section = composer_definition
                .entry(require_key.to_string())
                .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
            if let Some(section) = section.as_array_mut() {
                section.insert(package.clone(), PhpMixed::String(version.clone()));
            }
            if let Some(section) = composer_definition
                .get_mut(remove_key)
                .and_then(|v| v.as_array_mut())
            {
                section.shift_remove(package);
            }
            let remove_empty = composer_definition
                .get(remove_key)
                .and_then(|v| v.as_array())
                .map(|m| m.is_empty())
                .unwrap_or(false);
            if remove_empty && composer_definition.contains_key(remove_key) {
                composer_definition.shift_remove(remove_key);
            }
        }
        let _ = json.borrow().write(PhpMixed::Array(composer_definition));
    }

    /// @param array<string, string> $new
    fn update_file_cleanly(
        &self,
        json: &std::rc::Rc<std::cell::RefCell<JsonFile>>,
        new: &IndexMap<String, String>,
        require_key: &str,
        remove_key: &str,
        sort_packages: bool,
    ) -> bool {
        let contents = file_get_contents(json.borrow().get_path()).unwrap_or_default();

        let mut manipulator = match JsonManipulator::new(contents) {
            Ok(m) => m,
            Err(_) => return false,
        };

        for (package, constraint) in new {
            if !manipulator
                .add_link(require_key, package, constraint, sort_packages)
                .unwrap_or(false)
            {
                return false;
            }
            if !manipulator
                .remove_sub_node(remove_key, package)
                .unwrap_or(false)
            {
                return false;
            }
        }

        let _ = manipulator.remove_main_key_if_empty(remove_key);

        file_put_contents(
            json.borrow().get_path(),
            manipulator.get_contents().as_bytes(),
        );

        true
    }

    fn revert_composer_file(&self) {
        let json = self.json.borrow().as_ref().unwrap().clone();
        let lock = self.lock.borrow().clone();
        if self.newly_created.get() {
            let msg = format!(
                "\n<error>Installation failed, deleting {}.</error>",
                self.file.borrow()
            );
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);
            unlink(json.borrow().get_path());
            if file_exists(&lock) {
                unlink(&lock);
            }
        } else {
            let extra = if self.lock_backup.borrow().is_some() {
                format!(" and {} to their ", lock)
            } else {
                " to its ".to_string()
            };
            let msg = format!(
                "\n<error>Installation failed, reverting {}{}original content.</error>",
                self.file.borrow(),
                extra
            );
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);
            file_put_contents(
                json.borrow().get_path(),
                self.composer_backup.borrow().as_bytes(),
            );
            if let Some(ref lock_backup) = *self.lock_backup.borrow() {
                file_put_contents(&lock, lock_backup.as_bytes());
            }
        }
    }
}
