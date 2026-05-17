//! ref: composer/src/Composer/Command/RequireCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::seld::signal::signal_handler::SignalHandler;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, UnexpectedValueException, array_fill_keys, array_intersect,
    array_keys, array_map, array_merge, array_merge_recursive, array_unique, count, empty,
    file_exists, file_get_contents, file_put_contents, filesize, implode, is_writable, sprintf,
    strtolower, unlink,
};

use crate::advisory::auditor::Auditor;
use crate::command::base_command::BaseCommand;
use crate::command::completion_trait::CompletionTrait;
use crate::command::package_discovery_trait::PackageDiscoveryTrait;
use crate::composer::Composer;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::dependency_resolver::request::Request;
use crate::factory::Factory;
use crate::installer::Installer;
use crate::installer::installer_events::InstallerEvents;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::json::json_manipulator::JsonManipulator;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::BasePackage;
use crate::package::complete_package_interface::CompletePackageInterface;
use crate::package::loader::array_loader::ArrayLoader;
use crate::package::loader::root_package_loader::RootPackageLoader;
use crate::package::package_interface::PackageInterface;
use crate::package::version::version_parser::VersionParser;
use crate::package::version::version_selector::VersionSelector;
use crate::plugin::command_event::CommandEvent;
use crate::plugin::plugin_events::PluginEvents;
use crate::repository::composite_repository::CompositeRepository;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_set::RepositorySet;
use crate::util::filesystem::Filesystem;
use crate::util::package_sorter::PackageSorter;
use crate::util::silencer::Silencer;

#[derive(Debug)]
pub struct RequireCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,

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

impl CompletionTrait for RequireCommand {}
impl PackageDiscoveryTrait for RequireCommand {}

impl RequireCommand {
    pub fn configure(&mut self) {
        let suggest_available_package_incl_platform =
            self.suggest_available_package_incl_platform();
        let suggest_prefer_install = self.suggest_prefer_install();
        self.inner
            .set_name("require")
            .set_aliases(vec!["r".to_string()])
            .set_description("Adds required packages to your composer.json and installs them")
            .set_definition(vec![
                InputArgument::new("packages", Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL), "Optional package name can also include a version constraint, e.g. foo/bar or foo/bar:1.0.0 or foo/bar=1.0.0 or \"foo/bar 1.0.0\"", None, suggest_available_package_incl_platform),
                InputOption::new("dev", None, Some(InputOption::VALUE_NONE), "Add requirement to require-dev.", None, vec![]),
                InputOption::new("dry-run", None, Some(InputOption::VALUE_NONE), "Outputs the operations but will not execute anything (implicitly enables --verbose).", None, vec![]),
                InputOption::new("prefer-source", None, Some(InputOption::VALUE_NONE), "Forces installation from package sources when possible, including VCS information.", None, vec![]),
                InputOption::new("prefer-dist", None, Some(InputOption::VALUE_NONE), "Forces installation from package dist (default behavior).", None, vec![]),
                InputOption::new("prefer-install", None, Some(InputOption::VALUE_REQUIRED), "Forces installation from package dist|source|auto (auto chooses source for dev versions, dist for the rest).", None, suggest_prefer_install),
                InputOption::new("fixed", None, Some(InputOption::VALUE_NONE), "Write fixed version to the composer.json.", None, vec![]),
                InputOption::new("no-suggest", None, Some(InputOption::VALUE_NONE), "DEPRECATED: This flag does not exist anymore.", None, vec![]),
                InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None, vec![]),
                InputOption::new("no-update", None, Some(InputOption::VALUE_NONE), "Disables the automatic update of the dependencies (implies --no-install).", None, vec![]),
                InputOption::new("no-install", None, Some(InputOption::VALUE_NONE), "Skip the install step after updating the composer.lock file.", None, vec![]),
                InputOption::new("no-audit", None, Some(InputOption::VALUE_NONE), "Skip the audit step after updating the composer.lock file (can also be set via the COMPOSER_NO_AUDIT=1 env var).", None, vec![]),
                InputOption::new("audit-format", None, Some(InputOption::VALUE_REQUIRED), "Audit output format. Must be \"table\", \"plain\", \"json\", or \"summary\".", Some(PhpMixed::String(Auditor::FORMAT_SUMMARY.to_string())), Auditor::FORMATS.to_vec()),
                InputOption::new("no-security-blocking", None, Some(InputOption::VALUE_NONE), "Allows installing packages with security advisories or that are abandoned (can also be set via the COMPOSER_NO_SECURITY_BLOCKING=1 env var).", None, vec![]),
                InputOption::new("update-no-dev", None, Some(InputOption::VALUE_NONE), "Run the dependency update with the --no-dev option.", None, vec![]),
                InputOption::new("update-with-dependencies", Some(PhpMixed::String("w".to_string())), Some(InputOption::VALUE_NONE), "Allows inherited dependencies to be updated, except those that are root requirements (can also be set via the COMPOSER_WITH_DEPENDENCIES=1 env var).", None, vec![]),
                InputOption::new("update-with-all-dependencies", Some(PhpMixed::String("W".to_string())), Some(InputOption::VALUE_NONE), "Allows all inherited dependencies to be updated, including those that are root requirements (can also be set via the COMPOSER_WITH_ALL_DEPENDENCIES=1 env var).", None, vec![]),
                InputOption::new("with-dependencies", None, Some(InputOption::VALUE_NONE), "Alias for --update-with-dependencies", None, vec![]),
                InputOption::new("with-all-dependencies", None, Some(InputOption::VALUE_NONE), "Alias for --update-with-all-dependencies", None, vec![]),
                InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None, vec![]),
                InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None, vec![]),
                InputOption::new("prefer-stable", None, Some(InputOption::VALUE_NONE), "Prefer stable versions of dependencies (can also be set via the COMPOSER_PREFER_STABLE=1 env var).", None, vec![]),
                InputOption::new("prefer-lowest", None, Some(InputOption::VALUE_NONE), "Prefer lowest versions of dependencies (can also be set via the COMPOSER_PREFER_LOWEST=1 env var).", None, vec![]),
                InputOption::new("minimal-changes", Some(PhpMixed::String("m".to_string())), Some(InputOption::VALUE_NONE), "During an update with -w/-W, only perform absolutely necessary changes to transitive dependencies (can also be set via the COMPOSER_MINIMAL_CHANGES=1 env var).", None, vec![]),
                InputOption::new("sort-packages", None, Some(InputOption::VALUE_NONE), "Sorts packages when adding/updating a new dependency", None, vec![]),
                InputOption::new("optimize-autoloader", Some(PhpMixed::String("o".to_string())), Some(InputOption::VALUE_NONE), "Optimize autoloader during autoloader dump", None, vec![]),
                InputOption::new("classmap-authoritative", Some(PhpMixed::String("a".to_string())), Some(InputOption::VALUE_NONE), "Autoload classes from the classmap only. Implicitly enables `--optimize-autoloader`.", None, vec![]),
                InputOption::new("apcu-autoloader", None, Some(InputOption::VALUE_NONE), "Use APCu to cache found/not-found classes.", None, vec![]),
                InputOption::new("apcu-autoloader-prefix", None, Some(InputOption::VALUE_REQUIRED), "Use a custom prefix for the APCu autoloader cache. Implicitly enables --apcu-autoloader", None, vec![]),
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
        self.file = Factory::get_composer_file();
        let io = self.inner.get_io();

        if input.get_option("no-suggest").as_bool().unwrap_or(false) {
            io.write_error(
                PhpMixed::String(
                    "<warning>You are using the deprecated option \"--no-suggest\". It has no effect and will break in Composer 3.</warning>"
                        .to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
        }

        self.newly_created = !file_exists(&self.file);
        if self.newly_created && !file_put_contents(&self.file, "{\n}\n") {
            io.write_error(
                PhpMixed::String(format!(
                    "<error>{} could not be created.</error>",
                    self.file
                )),
                true,
                IOInterface::NORMAL,
            );

            return Ok(1);
        }
        if !Filesystem::is_readable(&self.file) {
            io.write_error(
                PhpMixed::String(format!("<error>{} is not readable.</error>", self.file)),
                true,
                IOInterface::NORMAL,
            );

            return Ok(1);
        }

        if filesize(&self.file) == 0 {
            file_put_contents(&self.file, "{\n}\n");
        }

        self.json = Some(JsonFile::new(&self.file, None, None));
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
                SignalHandler::SIGINT,
                SignalHandler::SIGTERM,
                SignalHandler::SIGHUP,
            ],
            Box::new(move |signal: String, handler: &SignalHandler| {
                // TODO(phase-b): self.get_io().write_error('Received '.$signal.', aborting', true, IOInterface::DEBUG);
                // TODO(phase-b): self.revert_composer_file();
                let _ = signal;
                handler.exit_with_last_signal();
            }),
        );

        // check for writability by writing to the file as is_writable can not be trusted on network-mounts
        // see https://github.com/composer/composer/issues/8231 and https://bugs.php.net/bug.php?id=68926
        if !is_writable(&self.file)
            && Silencer::call(
                "file_put_contents",
                &[
                    PhpMixed::String(self.file.clone()),
                    PhpMixed::String(self.composer_backup.clone()),
                ],
            )
            .as_bool()
                == Some(false)
        {
            io.write_error(
                PhpMixed::String(format!("<error>{} is not writable.</error>", self.file)),
                true,
                IOInterface::NORMAL,
            );

            return Ok(1);
        }

        if input.get_option("fixed").as_bool() == Some(true) {
            let config = self.json.as_ref().unwrap().read()?;

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
                io.write_error(
                    PhpMixed::String(
                        "<error>The \"--fixed\" option is only allowed for packages with a \"project\" type or for dev dependencies to prevent possible misuses.</error>"
                            .to_string(),
                    ),
                    true,
                    IOInterface::NORMAL,
                );

                if !config.contains_key("type") {
                    io.write_error(
                        PhpMixed::String(
                            "<error>If your package is not a library, you can explicitly specify the \"type\" by using \"composer config type project\".</error>"
                                .to_string(),
                        ),
                        true,
                        IOInterface::NORMAL,
                    );
                }

                return Ok(1);
            }
        }

        let composer = self.inner.require_composer(None, None)?;
        let repos = composer.get_repository_manager().get_repositories();

        let platform_overrides = composer.get_config().get("platform");
        // initialize self.repos as it is used by the PackageDiscoveryTrait
        let platform_repo = PlatformRepository::new(vec![], platform_overrides);
        let mut combined: Vec<
            Box<dyn crate::repository::repository_interface::RepositoryInterface>,
        > = vec![Box::new(platform_repo.clone())];
        for repo in repos {
            combined.push(repo);
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
            &platform_repo,
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

        let mut requirements = self.inner.format_requirements(requirements)?;

        if !input.get_option("dev").as_bool().unwrap_or(false)
            && io.is_interactive()
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

                let pkg = PackageSorter::get_most_current_version(
                    self.get_repos().find_packages(name, None),
                );
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
                let pkg_dev_tags: Vec<String> = array_unique(&array_merge_recursive(
                    dev_packages
                        .iter()
                        .map(|v| {
                            PhpMixed::List(
                                v.iter()
                                    .map(|s| Box::new(PhpMixed::String(s.clone())))
                                    .collect(),
                            )
                        })
                        .collect(),
                ));
                io.warning(format!(
                    "The package{} you required {} recommended to be placed in require-dev (because {} tagged as \"{}\") but you did not use --dev.",
                    plural,
                    plural2,
                    plural3,
                    implode("\", \"", &pkg_dev_tags),
                ));
                if io.ask_confirmation(
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
                io.write_error(
                    PhpMixed::String(sprintf(
                        "<error>Root package '%s' cannot require itself in its composer.json</error>",
                        &[PhpMixed::String(package.clone())],
                    )),
                    true,
                    IOInterface::NORMAL,
                );

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
                io.warning(sprintf(
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
                ));
            }

            if io.is_interactive() {
                if !io.ask_confirmation(
                    sprintf(
                        "<info>Do you want to move %s?</info> [<comment>no</comment>]? ",
                        &[PhpMixed::String(
                            if (inconsistent_require_keys.len() as i64) > 1 {
                                "these requirements"
                            } else {
                                "this requirement"
                            }
                            .to_string(),
                        )],
                    ),
                    false,
                ) {
                    if !io.ask_confirmation(
                        sprintf(
                            "<info>Do you want to re-run the command %s --dev?</info> [<comment>yes</comment>]? ",
                            &[PhpMixed::String(
                                if input.get_option("dev").as_bool().unwrap_or(false) {
                                    "without"
                                } else {
                                    "with"
                                }
                                .to_string(),
                            )],
                        ),
                        true,
                    ) {
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
                .get("sort-packages")
                .as_bool()
                .unwrap_or(false);

        self.first_require = self.newly_created;
        if !self.first_require {
            let composer_definition = self.json.as_ref().unwrap().read()?;
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

        io.write_error(
            PhpMixed::String(format!(
                "<info>{} has been {}</info>",
                self.file,
                if self.newly_created {
                    "created"
                } else {
                    "updated"
                }
            )),
            true,
            IOInterface::NORMAL,
        );

        if input.get_option("no-update").as_bool().unwrap_or(false) {
            return Ok(0);
        }

        composer.get_plugin_manager().deactivate_installed_plugins();

        // try/catch/finally
        let do_update_result =
            self.do_update(input, output, io, &requirements, require_key, remove_key);
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
        let composer_definition = self.json.as_ref().unwrap().read().unwrap_or_default();
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
        self.inner.reset_composer()?;
        let composer = self.inner.require_composer(None, None)?;

        self.dependency_resolution_completed = false;
        composer.get_event_dispatcher().add_listener(
            InstallerEvents::PRE_OPERATIONS_EXEC,
            Box::new(move || {
                // TODO(phase-b): self.dependency_resolution_completed = true;
            }),
            10000,
        );

        if input.get_option("dry-run").as_bool().unwrap_or(false) {
            let root_package = composer.get_package();
            let mut links: IndexMap<String, IndexMap<String, crate::package::link::Link>> =
                IndexMap::new();
            links.insert("require".to_string(), root_package.get_requires());
            links.insert("require-dev".to_string(), root_package.get_dev_requires());
            let loader = ArrayLoader::new(None, None, false);
            let new_links = loader.parse_links(
                root_package.get_name(),
                root_package.get_pretty_version(),
                BasePackage::supported_link_types(require_key)
                    .get("method")
                    .cloned()
                    .unwrap_or_default(),
                requirements,
            );
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
            root_package.set_requires(links.get("require").cloned().unwrap_or_default());
            root_package.set_dev_requires(links.get("require-dev").cloned().unwrap_or_default());

            // extract stability flags & references as they weren't present when loading the unmodified composer.json
            let mut references = root_package.get_references();
            references = RootPackageLoader::extract_references(requirements, references);
            root_package.set_references(references);
            let mut stability_flags = root_package.get_stability_flags();
            stability_flags = RootPackageLoader::extract_stability_flags(
                requirements,
                root_package.get_minimum_stability(),
                stability_flags,
            );
            root_package.set_stability_flags(stability_flags);
            // unset($stabilityFlags, $references);
        }

        let update_dev_mode = !input.get_option("update-no-dev").as_bool().unwrap_or(false);
        let optimize = input
            .get_option("optimize-autoloader")
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .get("optimize-autoloader")
                .as_bool()
                .unwrap_or(false);
        let authoritative = input
            .get_option("classmap-authoritative")
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
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
                .get("apcu-autoloader")
                .as_bool()
                .unwrap_or(false);
        let minimal_changes = input
            .get_option("minimal-changes")
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
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

        io.write_error(
            PhpMixed::String(format!(
                "<info>Running composer update {}{}</info>",
                implode(
                    " ",
                    &array_keys(requirements)
                        .into_iter()
                        .collect::<Vec<String>>()
                ),
                flags,
            )),
            true,
            IOInterface::NORMAL,
        );

        let command_event = CommandEvent::new(PluginEvents::COMMAND, "require", input, output);
        composer
            .get_event_dispatcher()
            .dispatch(command_event.get_name(), &command_event);

        composer
            .get_installation_manager()
            .set_output_progress(!input.get_option("no-progress").as_bool().unwrap_or(false));

        let install = Installer::create(io, &composer);

        let (prefer_source, prefer_dist) = self
            .inner
            .get_preferred_install_options(composer.get_config(), input)?;

        install
            .set_dry_run(input.get_option("dry-run").as_bool().unwrap_or(false))
            .set_verbose(input.get_option("verbose").as_bool().unwrap_or(false))
            .set_prefer_source(prefer_source)
            .set_prefer_dist(prefer_dist)
            .set_dev_mode(update_dev_mode)
            .set_optimize_autoloader(optimize)
            .set_class_map_authoritative(authoritative)
            .set_apcu_autoloader(apcu, apcu_prefix.as_deref())
            .set_update(true)
            .set_install(!input.get_option("no-install").as_bool().unwrap_or(false))
            .set_update_allow_transitive_dependencies(update_allow_transitive_dependencies)
            .set_platform_requirement_filter(self.inner.get_platform_requirement_filter(input)?)
            .set_prefer_stable(input.get_option("prefer-stable").as_bool().unwrap_or(false))
            .set_prefer_lowest(input.get_option("prefer-lowest").as_bool().unwrap_or(false))
            .set_audit_config(
                self.inner
                    .create_audit_config(composer.get_config(), input)?,
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
                for req in self.inner.normalize_requirements(
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
                        io.write_error(
                            PhpMixed::String(format!(
                                "You can also try re-running composer require with an explicit version constraint, e.g. \"composer require {}:*\" to figure out if any version is installable, or \"composer require {}:^2.1\" if you know which you need.",
                                req.get("name").cloned().unwrap_or_default(),
                                req.get("name").cloned().unwrap_or_default(),
                            )),
                            true,
                            IOInterface::NORMAL,
                        );
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
        let composer = self.inner.require_composer(None, None)?;
        let locker = composer.get_locker();
        let mut requirements: IndexMap<String, String> = IndexMap::new();
        let version_selector = VersionSelector::new(RepositorySet::new(None, None), None);
        let repo = if locker.is_locked() {
            composer.get_locker().get_locked_repository(Some(true))?
        } else {
            composer.get_repository_manager().get_local_repository()
        };
        for package_name in requirements_to_update {
            let mut package = repo.find_package(package_name, "*");
            // TODO(phase-b): `$package instanceof AliasPackage` downcast
            let mut package_as_alias: Option<&AliasPackage> = None;
            while let Some(alias) = package_as_alias {
                package = Some(Box::new(alias.get_alias_of().clone()) as Box<dyn PackageInterface>);
                package_as_alias = None;
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
                requirements.insert(
                    package_name.clone(),
                    version_selector.find_recommended_require_version(&*package),
                );
            }
            self.inner.get_io().write_error(
                PhpMixed::String(sprintf(
                    "Using version <info>%s</info> for <info>%s</info>",
                    &[
                        PhpMixed::String(
                            requirements.get(package_name).cloned().unwrap_or_default(),
                        ),
                        PhpMixed::String(package_name.clone()),
                    ],
                )),
                true,
                IOInterface::NORMAL,
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
                self.inner.get_io().warning(format!(
                    "Version {} looks like it may be a feature branch which is unlikely to keep working in the long run and may be in an unstable state",
                    requirements.get(package_name).cloned().unwrap_or_default(),
                ));
                if self.inner.get_io().is_interactive()
                    && !self.inner.get_io().ask_confirmation(
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
            self.update_file(
                self.json.as_ref().unwrap(),
                &requirements,
                require_key,
                remove_key,
                sort_packages,
            );
            if locker.is_locked() && composer.get_config().get("lock").as_bool().unwrap_or(false) {
                let stability_flags = RootPackageLoader::extract_stability_flags(
                    &requirements,
                    composer.get_package().get_minimum_stability(),
                    IndexMap::new(),
                );
                let stability_flags_clone = stability_flags.clone();
                locker.update_hash(
                    self.json.as_ref().unwrap(),
                    Box::new(move |mut lock_data: IndexMap<String, PhpMixed>| {
                        for (package_name, flag) in &stability_flags_clone {
                            let entry = lock_data
                                .entry("stability-flags".to_string())
                                .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
                            if let PhpMixed::Array(m) = entry {
                                m.insert(package_name.clone(), Box::new(PhpMixed::Int(*flag)));
                            }
                        }

                        lock_data
                    }),
                );
            }
        }

        Ok(0)
    }

    /// @param array<string, string> $new
    fn update_file(
        &self,
        json: &JsonFile,
        new: &IndexMap<String, String>,
        require_key: &str,
        remove_key: &str,
        sort_packages: bool,
    ) {
        if self.update_file_cleanly(json, new, require_key, remove_key, sort_packages) {
            return;
        }

        let mut composer_definition = self.json.as_ref().unwrap().read().unwrap_or_default();
        for (package, version) in new {
            if let Some(section) = composer_definition
                .entry(require_key.to_string())
                .or_insert_with(|| PhpMixed::Array(IndexMap::new()))
                .as_array_mut()
            {
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
        self.json.as_ref().unwrap().write(&PhpMixed::Array(
            composer_definition
                .into_iter()
                .map(|(k, v)| (k, Box::new(v)))
                .collect(),
        ));
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

        let manipulator = JsonManipulator::new(&contents);

        for (package, constraint) in new {
            if !manipulator.add_link(require_key, package, constraint, sort_packages) {
                return false;
            }
            if !manipulator.remove_sub_node(remove_key, package) {
                return false;
            }
        }

        manipulator.remove_main_key_if_empty(remove_key);

        file_put_contents(json.get_path(), &manipulator.get_contents());

        true
    }

    pub(crate) fn interact(&self, _input: &dyn InputInterface, _output: &dyn OutputInterface) {}

    fn revert_composer_file(&mut self) {
        let io = self.inner.get_io();

        if self.newly_created {
            io.write_error(
                PhpMixed::String(format!(
                    "\n<error>Installation failed, deleting {}.</error>",
                    self.file
                )),
                true,
                IOInterface::NORMAL,
            );
            unlink(self.json.as_ref().unwrap().get_path());
            if file_exists(&self.lock) {
                unlink(&self.lock);
            }
        } else {
            let msg = if self.lock_backup.is_some() {
                format!(" and {} to their ", self.lock)
            } else {
                " to its ".to_string()
            };
            io.write_error(
                PhpMixed::String(format!(
                    "\n<error>Installation failed, reverting {}{}original content.</error>",
                    self.file, msg
                )),
                true,
                IOInterface::NORMAL,
            );
            file_put_contents(
                self.json.as_ref().unwrap().get_path(),
                &self.composer_backup,
            );
            if let Some(ref lock_backup) = self.lock_backup {
                file_put_contents(&self.lock, lock_backup);
            }
        }
    }
}

impl BaseCommand for RequireCommand {
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
