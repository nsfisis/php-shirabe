//! ref: composer/src/Composer/Command/RequireCommand.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::seld::signal::SignalHandler;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, UnexpectedValueException, array_fill_keys, array_intersect,
    array_keys, array_map, array_merge, array_merge_recursive, array_unique, count, empty,
    file_exists, file_get_contents, file_put_contents, filesize, implode, is_writable, sprintf,
    strtolower, unlink,
};

use crate::advisory::Auditor;
use crate::command::PackageDiscoveryTrait;
use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer::Composer;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::dependency_resolver::Request;
use crate::factory::Factory;
use crate::installer::Installer;
use crate::installer::InstallerEvents;
use crate::io::IOInterface;
use crate::json::JsonFile;
use crate::json::JsonManipulator;
use crate::package::AliasPackage;
use crate::package::CompletePackageInterface;
use crate::package::PackageInterface;
use crate::package::base_package::{self, BasePackage};
use crate::package::loader::ArrayLoader;
use crate::package::loader::RootPackageLoader;
use crate::package::version::VersionParser;
use crate::package::version::VersionSelector;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::repository::CompositeRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositorySet;
use crate::util::Filesystem;
use crate::util::PackageSorter;
use crate::util::Silencer;

#[derive(Debug)]
pub struct RequireCommand {
    base_command_data: BaseCommandData,

    newly_created: bool,
    first_require: bool,
    json: Option<JsonFile>,
    file: String,
    composer_backup: String,
    /// file name
    lock: String,
    /// contents before modification if the lock file exists
    lock_backup: Option<String>,
    dependency_resolution_completed: bool,
}

impl PackageDiscoveryTrait for RequireCommand {
    fn get_repos_mut(&mut self) -> &mut Option<CompositeRepository> {
        todo!()
    }

    fn get_repository_sets_mut(&mut self) -> &mut IndexMap<String, RepositorySet> {
        todo!()
    }

    fn get_io(&self) -> &dyn IOInterface {
        todo!()
    }

    fn try_composer(&self) -> Option<Composer> {
        todo!()
    }

    fn require_composer(
        &self,
        disable_plugins: Option<bool>,
        disable_scripts: Option<bool>,
    ) -> Composer {
        todo!()
    }

    fn get_platform_requirement_filter(
        &self,
        input: &dyn InputInterface,
    ) -> Box<dyn crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface>
    {
        todo!()
    }

    fn normalize_requirements(&self, requires: Vec<String>) -> Vec<IndexMap<String, String>> {
        todo!()
    }
}

impl RequireCommand {
    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_available_package_incl_platform / suggest_prefer_install
        self
            .set_name("require")
            .set_aliases(&["r".to_string()])
            .set_description("Adds required packages to your composer.json and installs them")
            .set_definition(&[
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
            ])
            .set_help(
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
    }

    /// @throws \Seld\JsonLint\ParsingException
    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> Result<i64> {
        self.file = Factory::get_composer_file()?;

        if input.get_option("no-suggest").as_bool().unwrap_or(false) {
            self.get_io().write_error3("<warning>You are using the deprecated option \"--no-suggest\". It has no effect and will break in Composer 3.</warning>", true, io_interface::NORMAL);
        }

        self.newly_created = !file_exists(&self.file);
        let write_failed = self.newly_created && file_put_contents(&self.file, b"{\n}\n").is_none();
        if write_failed {
            let msg = format!("<error>{} could not be created.</error>", self.file);
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);

            return Ok(1);
        }
        if !Filesystem::is_readable(&self.file) {
            let msg = format!("<error>{} is not readable.</error>", self.file);
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);

            return Ok(1);
        }
        if filesize(&self.file) == Some(0) {
            file_put_contents(&self.file, b"{\n}\n");
        }

        self.json = Some(JsonFile::new(self.file.clone(), None, None)?);
        self.lock = Factory::get_lock_file(&self.file);
        self.composer_backup =
            file_get_contents(self.json.as_ref().unwrap().get_path()).unwrap_or_default();
        self.lock_backup = if file_exists(&self.lock) {
            file_get_contents(&self.lock)
        } else {
            None
        };

        // TODO(phase-b): closure captures `self` which requires complex borrow handling; the closure needs
        // to call self.get_io().write_error(...), self.revert_composer_file(), and handler.exit_with_last_signal()
        let signal_handler = SignalHandler::create(
            vec![
                SignalHandler::SIGINT.to_string(),
                SignalHandler::SIGTERM.to_string(),
                SignalHandler::SIGHUP.to_string(),
            ],
            Box::new(move |signal: String, handler: &SignalHandler| {
                // TODO(phase-b): self.get_io().write_error('Received '.$signal.', aborting', true, io_interface::DEBUG);
                // TODO(phase-b): self.revert_composer_file();
                let _ = signal;
                handler.exit_with_last_signal();
            }),
        );

        // check for writability by writing to the file as is_writable can not be trusted on network-mounts
        // see https://github.com/composer/composer/issues/8231 and https://bugs.php.net/bug.php?id=68926
        let file_path = self.file.clone();
        let backup_contents = self.composer_backup.clone();
        if !is_writable(&self.file)
            && Silencer::call(|| {
                shirabe_php_shim::file_put_contents(&file_path, backup_contents.as_bytes());
                Ok::<bool, anyhow::Error>(false)
            })
            .ok()
                == Some(false)
        {
            let msg = format!("<error>{} is not writable.</error>", self.file);
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);

            return Ok(1);
        }

        if input.get_option("fixed").as_bool() == Some(true) {
            let config = self.json.as_mut().unwrap().read()?;

            let package_type = if empty(&config.get("type").cloned().unwrap_or(PhpMixed::Null)) {
                "library".to_string()
            } else {
                config
                    .get("type")
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string()
            };

            /// @see https://github.com/composer/composer/pull/8313#issuecomment-532637955
            if package_type != "project" && !input.get_option("dev").as_bool().unwrap_or(false) {
                self.get_io().write_error3("<error>The \"--fixed\" option is only allowed for packages with a \"project\" type or for dev dependencies to prevent possible misuses.</error>", true, io_interface::NORMAL);

                if config.get("type").is_none() {
                    self.get_io().write_error3("<error>If your package is not a library, you can explicitly specify the \"type\" by using \"composer config type project\".</error>", true, io_interface::NORMAL);
                }

                return Ok(1);
            }
        }

        let composer = self.require_composer(None, None)?;
        let repos = composer.get_repository_manager().get_repositories();

        let platform_overrides = composer.get_config().borrow_mut().get("platform");
        let platform_overrides_map: IndexMap<String, PhpMixed> = platform_overrides
            .as_array()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect())
            .unwrap_or_default();
        // initialize self.repos as it is used by the PackageDiscoveryTrait
        let platform_repo = PlatformRepository::new(vec![], platform_overrides_map)?;
        let mut combined: Vec<Box<dyn crate::repository::RepositoryInterface>> = vec![
            // TODO(phase-b): PlatformRepository should be shared via Rc; use placeholder until
            // CompositeRepository accepts shared references
            Box::new(todo!("share platform_repo with PlatformRepository") as PlatformRepository),
        ];
        for _repo in repos {
            // TODO(phase-b): repos are borrowed from RepositoryManager; need to take ownership
            combined.push(todo!("take ownership of repo from RepositoryManager"));
        }
        *self.get_repos_mut() = Some(CompositeRepository::new(combined));

        let preferred_stability = if composer.get_package().get_prefer_stable() {
            "stable".to_string()
        } else {
            composer.get_package().get_minimum_stability().to_string()
        };

        let requirements_result = self.determine_requirements(
            input,
            output,
            input
                .get_argument("packages")
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
            input.get_option("no-update").as_bool().unwrap_or(false),
            input.get_option("fixed").as_bool().unwrap_or(false),
        );

        let requirements = match requirements_result {
            Ok(r) => r,
            Err(e) => {
                if self.newly_created {
                    self.revert_composer_file();

                    return Err(RuntimeException {
                        message: format!(
                            "No composer.json present in the current directory ({}), this may be the cause of the following exception.",
                            self.file
                        ),
                        code: 0,
                    }
                    .into());
                }

                return Err(e);
            }
        };

        let mut requirements = self.format_requirements(requirements)?;

        if !input.get_option("dev").as_bool().unwrap_or(false)
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

                // TODO(phase-b): find_packages returns Vec<Box<dyn BasePackage>> but
                // get_most_current_version expects Vec<Box<dyn PackageInterface>>; needs trait
                // upcasting once Rust supports it stably or an adapter.
                let _ = self.get_repos().find_packages(name, None);
                let pkg: Option<Box<dyn PackageInterface>> =
                    PackageSorter::get_most_current_version(todo!(
                        "convert Vec<Box<dyn BasePackage>> to Vec<Box<dyn PackageInterface>>"
                    ));
                // TODO(phase-b): instanceof CompletePackageInterface downcast
                let pkg_as_complete: Option<&dyn CompletePackageInterface> = None;
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
                // TODO(phase-b): PHP's array_merge_recursive + array_unique on a list of
                // string lists; collapsed here to a flat unique Vec<String>.
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
                    input.set_option("dev", PhpMixed::Bool(true));
                }
            }

            // unset($devPackages, $pkgDevTags);
        }

        let mut require_key = if input.get_option("dev").as_bool().unwrap_or(false) {
            "require-dev"
        } else {
            "require"
        };
        let mut remove_key = if input.get_option("dev").as_bool().unwrap_or(false) {
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
                let msg = sprintf(
                    "<error>Root package '%s' cannot require itself in its composer.json</error>",
                    &[PhpMixed::String(package.clone())],
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
                let warn_msg = sprintf(
                    "%s is currently present in the %s key and you ran the command %s the --dev flag, which will move it to the %s key.",
                    &[
                        PhpMixed::String(package.clone()),
                        PhpMixed::String(remove_key.to_string()),
                        PhpMixed::String(
                            if input.get_option("dev").as_bool().unwrap_or(false) {
                                "with"
                            } else {
                                "without"
                            }
                            .to_string(),
                        ),
                        PhpMixed::String(require_key.to_string()),
                    ],
                );
                self.get_io().warning(&warn_msg, &[]);
            }

            if self.get_io().is_interactive() {
                let q1 = sprintf(
                    "<info>Do you want to move %s?</info> [<comment>no</comment>]? ",
                    &[PhpMixed::String(
                        if (inconsistent_require_keys.len() as i64) > 1 {
                            "these requirements"
                        } else {
                            "this requirement"
                        }
                        .to_string(),
                    )],
                );
                if !self.get_io().ask_confirmation(q1, false) {
                    let q2 = sprintf(
                        "<info>Do you want to re-run the command %s --dev?</info> [<comment>yes</comment>]? ",
                        &[PhpMixed::String(
                            if input.get_option("dev").as_bool().unwrap_or(false) {
                                "without"
                            } else {
                                "with"
                            }
                            .to_string(),
                        )],
                    );
                    if !self.get_io().ask_confirmation(q2, true) {
                        return Ok(0);
                    }

                    input.set_option("dev", PhpMixed::Bool(true));
                    let swap = require_key;
                    require_key = remove_key;
                    remove_key = swap;
                }
            }
        }

        let sort_packages = input.get_option("sort-packages").as_bool().unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get("sort-packages")
                .as_bool()
                .unwrap_or(false);

        self.first_require = self.newly_created;
        if !self.first_require {
            let composer_definition = self.json.as_mut().unwrap().read()?;
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
                self.first_require = true;
            }
        }

        if !input.get_option("dry-run").as_bool().unwrap_or(false) {
            self.update_file(
                self.json.as_ref().unwrap(),
                &requirements,
                require_key,
                remove_key,
                sort_packages,
            );
        }

        let updated_msg = format!(
            "<info>{} has been {}</info>",
            self.file,
            if self.newly_created {
                "created"
            } else {
                "updated"
            }
        );
        self.get_io()
            .write_error3(&updated_msg, true, io_interface::NORMAL);

        if input.get_option("no-update").as_bool().unwrap_or(false) {
            return Ok(0);
        }

        composer.get_plugin_manager().deactivate_installed_plugins();

        // try/catch/finally
        // TODO(phase-b): do_update borrows io from self while also needing &mut self for state
        // mutations; needs an Rc<dyn IOInterface> on self for clean sharing.
        let do_update_result = self.do_update(
            input,
            output,
            todo!("share io reference for do_update"),
            &requirements,
            require_key,
            remove_key,
        );
        let dry_run = input.get_option("dry-run").as_bool().unwrap_or(false);

        let result = match do_update_result {
            Ok(result) => {
                let final_result = if result == 0 && (requirements_to_guess.len() as i64) > 0 {
                    self.update_requirements_after_resolution(
                        &requirements_to_guess,
                        require_key,
                        remove_key,
                        sort_packages,
                        dry_run,
                        input.get_option("fixed").as_bool().unwrap_or(false),
                    )?
                } else {
                    result
                };
                Ok(final_result)
            }
            Err(e) => {
                if !self.dependency_resolution_completed {
                    self.revert_composer_file();
                }
                Err(e)
            }
        };

        // finally
        if dry_run && self.newly_created {
            // @unlink($this->json->getPath());
            unlink(self.json.as_ref().unwrap().get_path());
        }
        signal_handler.unregister();

        result
    }

    /// @param array<string, string> $newRequirements
    /// @return string[]
    fn get_inconsistent_require_keys(
        &mut self,
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
    fn get_packages_by_require_key(&mut self) -> IndexMap<String, String> {
        let composer_definition = self.json.as_mut().unwrap().read().unwrap_or_default();
        let mut require: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut require_dev: IndexMap<String, PhpMixed> = IndexMap::new();

        if let Some(r) = composer_definition
            .get("require")
            .and_then(|v| v.as_array())
        {
            for (k, v) in r {
                require.insert(k.clone(), (**v).clone());
            }
        }

        if let Some(r) = composer_definition
            .get("require-dev")
            .and_then(|v| v.as_array())
        {
            for (k, v) in r {
                require_dev.insert(k.clone(), (**v).clone());
            }
        }

        array_merge(
            array_fill_keys(
                PhpMixed::List(
                    array_keys(&require)
                        .into_iter()
                        .map(|k| Box::new(PhpMixed::String(k)))
                        .collect(),
                ),
                PhpMixed::String("require".to_string()),
            ),
            array_fill_keys(
                PhpMixed::List(
                    array_keys(&require_dev)
                        .into_iter()
                        .map(|k| Box::new(PhpMixed::String(k)))
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
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
        io: &dyn IOInterface,
        requirements: &IndexMap<String, String>,
        require_key: &str,
        _remove_key: &str,
    ) -> Result<i64> {
        // Update packages
        self.reset_composer()?;
        let mut composer = self.require_composer(None, None)?;

        self.dependency_resolution_completed = false;
        // TODO(phase-b): add_listener expects a Callable enum; PHP closure should set
        // self.dependency_resolution_completed = true when invoked.
        composer.get_event_dispatcher().borrow_mut().add_listener(
            InstallerEvents::PRE_OPERATIONS_EXEC,
            crate::event_dispatcher::Callable::Closure,
            10000,
        );

        if input.get_option("dry-run").as_bool().unwrap_or(false) {
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
                root_package.get_name(),
                root_package.get_pretty_version(),
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
            // TODO(phase-b): root_package mutation requires &mut RootPackageInterface but
            // Composer::get_package() exposes only & dyn; needs accessor returning &mut for
            // the dry-run case to update requires/dev-requires/stability flags/references.
            let _ = &links;
            let _ = root_package.get_references().clone();
            let _ = RootPackageLoader::extract_references(
                requirements,
                root_package.get_references().clone(),
            );
            let _ = RootPackageLoader::extract_stability_flags(
                requirements,
                root_package.get_minimum_stability(),
                root_package.get_stability_flags().clone(),
            );
            // unset($stabilityFlags, $references);
        }

        let update_dev_mode = !input.get_option("update-no-dev").as_bool().unwrap_or(false);
        let optimize = input
            .get_option("optimize-autoloader")
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get("optimize-autoloader")
                .as_bool()
                .unwrap_or(false);
        let authoritative = input
            .get_option("classmap-authoritative")
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get("classmap-authoritative")
                .as_bool()
                .unwrap_or(false);
        let apcu_prefix = input
            .get_option("apcu-autoloader-prefix")
            .as_string()
            .map(|s| s.to_string());
        let apcu = apcu_prefix.is_some()
            || input
                .get_option("apcu-autoloader")
                .as_bool()
                .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get("apcu-autoloader")
                .as_bool()
                .unwrap_or(false);
        let minimal_changes = input
            .get_option("minimal-changes")
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get("update-with-minimal-changes")
                .as_bool()
                .unwrap_or(false);

        let mut update_allow_transitive_dependencies = Request::UPDATE_ONLY_LISTED;
        let mut flags = String::new();
        if input
            .get_option("update-with-all-dependencies")
            .as_bool()
            .unwrap_or(false)
            || input
                .get_option("with-all-dependencies")
                .as_bool()
                .unwrap_or(false)
        {
            update_allow_transitive_dependencies = Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS;
            flags += " --with-all-dependencies";
        } else if input
            .get_option("update-with-dependencies")
            .as_bool()
            .unwrap_or(false)
            || input
                .get_option("with-dependencies")
                .as_bool()
                .unwrap_or(false)
        {
            update_allow_transitive_dependencies =
                Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS_NO_ROOT_REQUIRE;
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

        let command_event = CommandEvent::new(PluginEvents::COMMAND, "require", input, output);
        composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch(Some(command_event.get_name()), None);

        composer
            .get_installation_manager_mut()
            .set_output_progress(!input.get_option("no-progress").as_bool().unwrap_or(false));

        // TODO(phase-b): Installer::create takes Box<dyn IOInterface> for ownership but io is a
        // borrowed &dyn here; needs Rc<dyn IOInterface> for proper sharing.
        let mut install = Installer::create(todo!("share io as Box<dyn IOInterface>"), &composer);

        let (prefer_source, prefer_dist) =
            self.get_preferred_install_options(&*composer.get_config().borrow(), input, false)?;

        install
            .set_dry_run(input.get_option("dry-run").as_bool().unwrap_or(false))
            .set_verbose(input.get_option("verbose").as_bool().unwrap_or(false))
            .set_prefer_source(prefer_source)
            .set_prefer_dist(prefer_dist)
            .set_dev_mode(update_dev_mode)
            .set_optimize_autoloader(optimize)
            .set_class_map_authoritative(authoritative)
            .set_apcu_autoloader(apcu, apcu_prefix.clone())
            .set_update(true)
            .set_install(!input.get_option("no-install").as_bool().unwrap_or(false))
            .set_update_allow_transitive_dependencies(update_allow_transitive_dependencies)?
            .set_platform_requirement_filter(BaseCommand::get_platform_requirement_filter(
                self, input,
            )?)
            .set_prefer_stable(input.get_option("prefer-stable").as_bool().unwrap_or(false))
            .set_prefer_lowest(input.get_option("prefer-lowest").as_bool().unwrap_or(false))
            .set_audit_config(
                self.create_audit_config(&mut *composer.get_config().borrow_mut(), input)?,
            )
            .set_minimal_update(minimal_changes);

        // if no lock is present, or the file is brand new, we do not do a
        // partial update as this is not supported by the Installer
        if !self.first_require && composer.get_locker().is_locked() {
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
                        .get_argument("packages")
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
        &mut self,
        requirements_to_update: &[String],
        require_key: &str,
        remove_key: &str,
        sort_packages: bool,
        dry_run: bool,
        fixed: bool,
    ) -> Result<i64> {
        let mut composer = self.require_composer(None, None)?;
        let locker_is_locked = composer.get_locker_mut().is_locked();
        let mut requirements: IndexMap<String, String> = IndexMap::new();
        let mut version_selector = VersionSelector::new(
            RepositorySet::new(
                "stable",
                IndexMap::new(),
                vec![],
                IndexMap::new(),
                IndexMap::new(),
                IndexMap::new(),
            ),
            None,
        )?;
        // TODO(phase-b): get_locked_repository returns LockArrayRepository (owned) and
        // get_local_repository returns &dyn InstalledRepositoryInterface; need a common
        // interface for find_package.
        let locked_repo;
        let repo: &dyn RepositoryInterface = if locker_is_locked {
            locked_repo = composer.get_locker_mut().get_locked_repository(true)?;
            &locked_repo
        } else {
            todo!("convert &dyn InstalledRepositoryInterface to &dyn RepositoryInterface")
        };
        for package_name in requirements_to_update {
            let mut package = repo.find_package(
                package_name,
                crate::repository::FindPackageConstraint::String("*".to_string()),
            );
            // TODO(phase-b): `$package instanceof AliasPackage` downcast
            let package_as_alias: Option<&AliasPackage> = None;
            while let Some(_alias) = package_as_alias {
                // TODO(phase-b): get_alias_of returns &dyn BasePackage; clone is not available
                // and BasePackage is not PackageInterface (the latter is a super-trait).
                package = todo!("upcast alias.get_alias_of() to Box<dyn BasePackage>");
            }

            let package = match package {
                Some(p) => p,
                None => continue,
            };

            if fixed {
                requirements.insert(
                    package_name.clone(),
                    package.get_pretty_version().to_string(),
                );
            } else {
                // TODO(phase-b): trait upcast from &dyn BasePackage to &dyn PackageInterface
                // is not yet stable in Rust; use explicit as_package_interface() when available.
                let pkg_as_pi: &dyn PackageInterface =
                    todo!("upcast &dyn BasePackage to &dyn PackageInterface");
                requirements.insert(
                    package_name.clone(),
                    version_selector.find_recommended_require_version(pkg_as_pi)?,
                );
            }
            self.get_io().write_error3(
                &sprintf(
                    "Using version <info>%s</info> for <info>%s</info>",
                    &[
                        PhpMixed::String(
                            requirements.get(package_name).cloned().unwrap_or_default(),
                        ),
                        PhpMixed::String(package_name.clone()),
                    ],
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
            )
            .unwrap_or(false)
            {
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
            // TODO(phase-b): update_file takes &mut self while self.json is borrowed; needs
            // refactor to pass the JsonFile owned/cloned or use interior mutability.
            let json_path = self.json.as_ref().unwrap().get_path().to_string();
            let _ = (
                json_path,
                &requirements,
                require_key,
                remove_key,
                sort_packages,
            );
            todo!("call self.update_file without overlapping borrows of self.json");
            #[allow(unreachable_code)]
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
                    composer.get_package().get_minimum_stability(),
                    IndexMap::new(),
                );
                let stability_flags_clone = stability_flags.clone();
                // TODO(phase-b): get_locker_mut needs update_hash with stability flags rewriter.
                let _ = &stability_flags_clone;
                todo!("update locker hash with stability flags rewriter");
            }
        }

        Ok(0)
    }

    /// @param array<string, string> $new
    fn update_file(
        &mut self,
        json: &JsonFile,
        new: &IndexMap<String, String>,
        require_key: &str,
        remove_key: &str,
        sort_packages: bool,
    ) {
        if self.update_file_cleanly(json, new, require_key, remove_key, sort_packages) {
            return;
        }

        let composer_definition_mixed = self.json.as_mut().unwrap().read().unwrap_or_default();
        let mut composer_definition: IndexMap<String, Box<PhpMixed>> = composer_definition_mixed
            .as_array()
            .cloned()
            .unwrap_or_default();
        for (package, version) in new {
            let section = composer_definition
                .entry(require_key.to_string())
                .or_insert_with(|| Box::new(PhpMixed::Array(IndexMap::new())));
            if let Some(section) = section.as_array_mut() {
                section.insert(package.clone(), Box::new(PhpMixed::String(version.clone())));
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
        let _ = self
            .json
            .as_ref()
            .unwrap()
            .write(PhpMixed::Array(composer_definition));
    }

    /// @param array<string, string> $new
    fn update_file_cleanly(
        &self,
        json: &JsonFile,
        new: &IndexMap<String, String>,
        require_key: &str,
        remove_key: &str,
        sort_packages: bool,
    ) -> bool {
        let contents = file_get_contents(json.get_path()).unwrap_or_default();

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

        file_put_contents(json.get_path(), manipulator.get_contents().as_bytes());

        true
    }

    pub(crate) fn interact(&self, _input: &dyn InputInterface, _output: &dyn OutputInterface) {}

    fn revert_composer_file(&mut self) {
        if self.newly_created {
            let msg = format!(
                "\n<error>Installation failed, deleting {}.</error>",
                self.file
            );
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);
            unlink(self.json.as_ref().unwrap().get_path());
            if file_exists(&self.lock) {
                unlink(&self.lock);
            }
        } else {
            let extra = if self.lock_backup.is_some() {
                format!(" and {} to their ", self.lock)
            } else {
                " to its ".to_string()
            };
            let msg = format!(
                "\n<error>Installation failed, reverting {}{}original content.</error>",
                self.file, extra
            );
            self.get_io().write_error3(&msg, true, io_interface::NORMAL);
            file_put_contents(
                self.json.as_ref().unwrap().get_path(),
                self.composer_backup.as_bytes(),
            );
            if let Some(ref lock_backup) = self.lock_backup {
                file_put_contents(&self.lock, lock_backup.as_bytes());
            }
        }
    }
}

impl HasBaseCommandData for RequireCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
