//! ref: composer/src/Composer/Command/CreateProjectCommand.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::seld::signal::SignalHandler;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_external_packages::symfony::component::finder::Finder;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, InvalidArgumentException, PhpMixed, RuntimeException,
    UnexpectedValueException, array_pop, chdir, explode_with_limit, file_exists, getcwd, implode,
    is_dir, is_file, mkdir, realpath, rtrim, strtolower, unlink,
};

use crate::advisory::Auditor;
use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer::PartialComposerHandle;
use crate::config::Config;
use crate::config::ConfigSourceInterface;
use crate::config::JsonConfigSource;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::dependency_resolver::operation::InstallOperation;
use crate::factory::Factory;
use crate::filter::platform_requirement_filter::IgnoreAllPlatformRequirementFilter;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::installer::Installer;
use crate::installer::ProjectInstaller;
use crate::installer::SuggestedPackagesReporter;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::json::JsonFile;
use crate::package::version::VersionParser;
use crate::package::version::VersionSelector;
use crate::package::{STABILITIES, SUPPORTED_LINK_TYPES};
use crate::plugin::PluginBlockedException;
use crate::repository::CompositeRepository;
use crate::repository::InstalledArrayRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryFactory;
use crate::repository::RepositorySet;
use crate::script::ScriptEvents;
use crate::util::Filesystem;
use crate::util::Platform;
use crate::util::ProcessExecutor;

/// Install a package as new project into new directory.
#[derive(Debug)]
pub struct CreateProjectCommand {
    base_command_data: BaseCommandData,

    /// @var SuggestedPackagesReporter
    pub(crate) suggested_packages_reporter: Option<SuggestedPackagesReporter>,
}

impl CreateProjectCommand {
    fn configure(&mut self) {
        // TODO(cli-completion): suggest_prefer_install / suggest_available_package
        self
            .set_name("create-project")
            .set_description("Creates new project from a package into given directory")
            .set_definition(&[
                InputArgument::new("package", Some(InputArgument::OPTIONAL), "Package name to be installed", None).unwrap().into(),
                InputArgument::new("directory", Some(InputArgument::OPTIONAL), "Directory where the files should be created", None).unwrap().into(),
                InputArgument::new("version", Some(InputArgument::OPTIONAL), "Version, will default to latest", None).unwrap().into(),
                InputOption::new("stability", Some(PhpMixed::String("s".to_string())), Some(InputOption::VALUE_REQUIRED), "Minimum-stability allowed (unless a version is specified).", None).unwrap().into(),
                InputOption::new("prefer-source", None, Some(InputOption::VALUE_NONE), "Forces installation from package sources when possible, including VCS information.", None).unwrap().into(),
                InputOption::new("prefer-dist", None, Some(InputOption::VALUE_NONE), "Forces installation from package dist (default behavior).", None).unwrap().into(),
                InputOption::new("prefer-install", None, Some(InputOption::VALUE_REQUIRED), "Forces installation from package dist|source|auto (auto chooses source for dev versions, dist for the rest).", None).unwrap().into(),
                InputOption::new("repository", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Add custom repositories to look the package up, either by URL or using JSON arrays", None).unwrap().into(),
                InputOption::new("repository-url", None, Some(InputOption::VALUE_REQUIRED), "DEPRECATED: Use --repository instead.", None).unwrap().into(),
                InputOption::new("add-repository", None, Some(InputOption::VALUE_NONE), "Add the custom repository in the composer.json. If a lock file is present it will be deleted and an update will be run instead of install.", None).unwrap().into(),
                InputOption::new("dev", None, Some(InputOption::VALUE_NONE), "Enables installation of require-dev packages (enabled by default, only present for BC).", None).unwrap().into(),
                InputOption::new("no-dev", None, Some(InputOption::VALUE_NONE), "Disables installation of require-dev packages.", None).unwrap().into(),
                InputOption::new("no-custom-installers", None, Some(InputOption::VALUE_NONE), "DEPRECATED: Use no-plugins instead.", None).unwrap().into(),
                InputOption::new("no-scripts", None, Some(InputOption::VALUE_NONE), "Whether to prevent execution of all defined scripts in the root package.", None).unwrap().into(),
                InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None).unwrap().into(),
                InputOption::new("no-secure-http", None, Some(InputOption::VALUE_NONE), "Disable the secure-http config option temporarily while installing the root package. Use at your own risk. Using this flag is a bad idea.", None).unwrap().into(),
                InputOption::new("keep-vcs", None, Some(InputOption::VALUE_NONE), "Whether to prevent deleting the vcs folder.", None).unwrap().into(),
                InputOption::new("remove-vcs", None, Some(InputOption::VALUE_NONE), "Whether to force deletion of the vcs folder without prompting.", None).unwrap().into(),
                InputOption::new("no-install", None, Some(InputOption::VALUE_NONE), "Whether to skip installation of the package dependencies.", None).unwrap().into(),
                InputOption::new("no-audit", None, Some(InputOption::VALUE_NONE), "Whether to skip auditing of the installed package dependencies (can also be set via the COMPOSER_NO_AUDIT=1 env var).", None).unwrap().into(),
                InputOption::new("audit-format", None, Some(InputOption::VALUE_REQUIRED), "Audit output format. Must be \"table\", \"plain\", \"json\" or \"summary\".", Some(PhpMixed::String(Auditor::FORMAT_SUMMARY.to_string()))).unwrap().into(),
                InputOption::new("no-security-blocking", None, Some(InputOption::VALUE_NONE), "Allows installing packages with security advisories or that are abandoned (can also be set via the COMPOSER_NO_SECURITY_BLOCKING=1 env var).", None).unwrap().into(),
                InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None).unwrap().into(),
                InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None).unwrap().into(),
                InputOption::new("ask", None, Some(InputOption::VALUE_NONE), "Whether to ask for project directory.", None).unwrap().into(),
            ])
            .set_help(
                "The <info>create-project</info> command creates a new project from a given\n\
                package into a new directory. If executed without params and in a directory\n\
                with a composer.json file it installs the packages for the current project.\n\n\
                You can use this command to bootstrap new projects or setup a clean\n\
                version-controlled installation for developers of your project.\n\n\
                <info>php composer.phar create-project vendor/project target-directory [version]</info>\n\n\
                You can also specify the version with the package name using = or : as separator.\n\n\
                <info>php composer.phar create-project vendor/project:version target-directory</info>\n\n\
                To install unstable packages, either specify the version you want, or use the\n\
                --stability=dev (where dev can be one of RC, beta, alpha or dev).\n\n\
                To setup a developer workable version you should create the project using the source\n\
                controlled code by appending the <info>'--prefer-source'</info> flag.\n\n\
                To install a package from another repository than the default one you\n\
                can pass the <info>'--repository=https://myrepository.org'</info> flag.\n\n\
                Read more at https://getcomposer.org/doc/03-cli.md#create-project"
            );
    }

    fn execute(
        &mut self,
        input: &mut dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> Result<i64> {
        let config = std::rc::Rc::new(std::cell::RefCell::new(Factory::create_config(None, None)?));
        // TODO(phase-b): get_io returns &mut Self-borrow; clone_box for an owned Box to dodge.
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = self.get_io().clone();

        let (prefer_source, prefer_dist) =
            self.get_preferred_install_options(&config.borrow(), input, true)?;

        if input.get_option("dev").as_bool().unwrap_or(false) {
            io.write_error("<warning>You are using the deprecated option \"dev\". Dev packages are installed by default now.</warning>");
        }
        if input
            .get_option("no-custom-installers")
            .as_bool()
            .unwrap_or(false)
        {
            io.write_error("<warning>You are using the deprecated option \"no-custom-installers\". Use \"no-plugins\" instead.</warning>");
            input.set_option("no-plugins", PhpMixed::Bool(true));
        }

        if input.is_interactive() && input.get_option("ask").as_bool().unwrap_or(false) {
            let package = input.get_argument("package");
            if package.is_null() {
                return Err(RuntimeException {
                    message: "Not enough arguments (missing: \"package\").".to_string(),
                    code: 0,
                }
                .into());
            }
            let mut parts =
                explode_with_limit("/", &strtolower(package.as_string().unwrap_or("")), 2);
            let prompt = format!(
                "New project directory [<comment>{}</comment>]: ",
                array_pop(&mut parts).unwrap_or_default()
            );
            input.set_argument("directory", io.ask(prompt, PhpMixed::Null));
        }

        let repository_opt = input.get_option("repository");
        let repository_url_opt = input.get_option("repository-url");
        let repositories = if repository_opt
            .as_list()
            .map(|l| l.len() > 0)
            .unwrap_or(false)
        {
            Some(repository_opt)
        } else {
            Some(repository_url_opt)
        };

        self.install_project(
            io,
            config,
            input,
            input
                .get_argument("package")
                .as_string()
                .map(|s| s.to_string()),
            input
                .get_argument("directory")
                .as_string()
                .map(|s| s.to_string()),
            input
                .get_argument("version")
                .as_string()
                .map(|s| s.to_string()),
            input
                .get_option("stability")
                .as_string()
                .map(|s| s.to_string()),
            prefer_source,
            prefer_dist,
            !input.get_option("no-dev").as_bool().unwrap_or(false),
            repositories,
            input.get_option("no-plugins").as_bool().unwrap_or(false),
            input.get_option("no-scripts").as_bool().unwrap_or(false),
            input.get_option("no-progress").as_bool().unwrap_or(false),
            input.get_option("no-install").as_bool().unwrap_or(false),
            Some(self.get_platform_requirement_filter(input)?),
            !input
                .get_option("no-secure-http")
                .as_bool()
                .unwrap_or(false),
            input
                .get_option("add-repository")
                .as_bool()
                .unwrap_or(false),
        )
    }

    /// @param string|array<string>|null $repositories
    ///
    /// @throws \Exception
    #[allow(clippy::too_many_arguments)]
    pub fn install_project(
        &mut self,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        input: &dyn InputInterface,
        package_name: Option<String>,
        directory: Option<String>,
        package_version: Option<String>,
        stability: Option<String>,
        mut prefer_source: bool,
        mut prefer_dist: bool,
        install_dev_packages: bool,
        repositories: Option<PhpMixed>,
        disable_plugins: bool,
        disable_scripts: bool,
        no_progress: bool,
        no_install: bool,
        platform_requirement_filter: Option<Box<dyn PlatformRequirementFilterInterface>>,
        secure_http: bool,
        add_repository: bool,
    ) -> Result<i64> {
        let old_cwd = Platform::get_cwd(false)?;

        let repositories: Option<Vec<String>> = match repositories {
            Some(PhpMixed::Null) | None => None,
            Some(PhpMixed::List(list)) => Some(
                list.into_iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect(),
            ),
            Some(PhpMixed::Array(map)) => Some(
                map.into_iter()
                    .filter_map(|(_, v)| v.as_string().map(|s| s.to_string()))
                    .collect(),
            ),
            Some(other) => Some(vec![other.as_string().unwrap_or("").to_string()]),
        };

        let platform_requirement_filter = platform_requirement_filter
            .unwrap_or_else(PlatformRequirementFilterFactory::ignore_nothing);

        // we need to manually load the configuration to pass the auth credentials to the io interface!
        io.borrow_mut()
            .load_configuration(&mut *config.borrow_mut())?;

        self.suggested_packages_reporter = Some(SuggestedPackagesReporter::new(io.clone()));

        let installed_from_vcs = if let Some(package_name) = package_name.as_ref() {
            self.install_root_package(
                input,
                io.clone(),
                &config,
                package_name,
                &*platform_requirement_filter,
                directory.clone(),
                package_version,
                stability,
                prefer_source,
                prefer_dist,
                install_dev_packages,
                repositories.as_ref(),
                disable_plugins,
                disable_scripts,
                no_progress,
                secure_http,
            )?
        } else {
            false
        };

        if repositories.is_some() && add_repository && is_file("composer.lock") {
            unlink("composer.lock");
        }

        let mut composer_handle = self.create_composer_instance(
            input,
            io.clone(),
            None,
            disable_plugins,
            Some(disable_scripts),
        )?;

        // add the repository to the composer.json and use it for the install run later
        if let Some(repos) = repositories.as_ref() {
            if add_repository {
                for (index, repo) in repos.iter().enumerate() {
                    let config = crate::command::composer_full(&composer_handle).get_config();
                    let repo_config =
                        RepositoryFactory::config_from_string(io.clone(), &config, repo, true)?;
                    let composer_json_repositories_config =
                        crate::command::composer_full(&composer_handle)
                            .get_config()
                            .borrow()
                            .get_repositories();
                    // TODO(phase-b): generate_repository_name expects existing repos as
                    // IndexMap<String, Box<dyn RepositoryInterface>>; pass empty placeholder.
                    let _ = &composer_json_repositories_config;
                    let placeholder_existing: IndexMap<
                        String,
                        crate::repository::RepositoryInterfaceHandle,
                    > = IndexMap::new();
                    let name = RepositoryFactory::generate_repository_name(
                        &PhpMixed::Int(index as i64),
                        &repo_config,
                        &placeholder_existing,
                    );
                    let mut config_source = JsonConfigSource::new(
                        JsonFile::new("composer.json".to_string(), None, None)?,
                        false,
                    );

                    let is_packagist_disabled = (repo_config.contains_key("packagist")
                        && repo_config.len() == 1
                        && repo_config.get("packagist").and_then(|v| v.as_bool()) == Some(false))
                        || (repo_config.contains_key("packagist.org")
                            && repo_config.len() == 1
                            && repo_config.get("packagist.org").and_then(|v| v.as_bool())
                                == Some(false));
                    if is_packagist_disabled {
                        config_source.add_repository("packagist.org", PhpMixed::Bool(false), false);
                    } else {
                        config_source.add_repository(
                            &name,
                            PhpMixed::Array(
                                repo_config
                                    .iter()
                                    .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                                    .collect(),
                            ),
                            false,
                        );
                    }

                    composer_handle = self.create_composer_instance(
                        input,
                        io.clone(),
                        None,
                        disable_plugins,
                        None,
                    )?;
                }
            }
        }

        let mut composer = crate::command::composer_full_mut(&composer_handle);

        let process = composer
            .get_loop()
            .borrow()
            .get_process_executor()
            .map(std::rc::Rc::clone);
        let mut fs = Filesystem::new(process);

        // dispatch event
        composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch_script(
                ScriptEvents::POST_ROOT_PACKAGE_INSTALL,
                install_dev_packages,
                vec![],
                IndexMap::new(),
            );

        // use the new config including the newly installed project
        let config = composer.get_config();
        let (ps, pd) = self.get_preferred_install_options(&*config.borrow(), input, false)?;
        prefer_source = ps;
        prefer_dist = pd;

        // install dependencies of the created project
        if no_install == false {
            composer
                .get_installation_manager()
                .borrow_mut()
                .set_output_progress(!no_progress);

            let mut installer = Installer::create(io.clone(), &composer_handle);
            // TODO(phase-b): set_suggested_packages_reporter takes by value but PHP class
            // means shared ownership; needs Rc<SuggestedPackagesReporter> for proper sharing.
            installer
                .set_prefer_source(prefer_source)
                .set_prefer_dist(prefer_dist)
                .set_dev_mode(install_dev_packages)
                .set_platform_requirement_filter(platform_requirement_filter.clone_box())
                .set_suggested_packages_reporter(SuggestedPackagesReporter::new(io.clone()))
                .set_optimize_autoloader(
                    config
                        .borrow_mut()
                        .get("optimize-autoloader")
                        .as_bool()
                        .unwrap_or(false),
                )
                .set_class_map_authoritative(
                    config
                        .borrow_mut()
                        .get("classmap-authoritative")
                        .as_bool()
                        .unwrap_or(false),
                )
                .set_apcu_autoloader(
                    config
                        .borrow_mut()
                        .get("apcu-autoloader")
                        .as_bool()
                        .unwrap_or(false),
                    None,
                )
                .set_audit_config(self.create_audit_config(&mut *config.borrow_mut(), input)?);

            if !composer.get_locker().borrow_mut().is_locked() {
                installer.set_update(true);
            }

            if disable_plugins {
                installer.disable_plugins();
            }

            match installer.run() {
                Ok(status) => {
                    if 0 != status {
                        return Ok(status);
                    }
                }
                Err(e) => {
                    // TODO(phase-b): catch only PluginBlockedException
                    if let Some(_pbe) = e.downcast_ref::<PluginBlockedException>() {
                        io.write_error("<error>Hint: To allow running the config command recommended below before dependencies are installed, run create-project with --no-install.</error>");
                        io.write_error(&format!(
                            "<error>You can then cd into {}, configure allow-plugins, and finally run a composer install to complete the process.</error>",
                            getcwd().unwrap_or_default()
                        ));
                    }
                    return Err(e);
                }
            }
        }

        let mut has_vcs = installed_from_vcs;
        let remove_vcs = !input.get_option("keep-vcs").as_bool().unwrap_or(false)
            && installed_from_vcs
            && (input.get_option("remove-vcs").as_bool().unwrap_or(false)
                || !io.is_interactive()
                || io.ask_confirmation(
                    "<info>Do you want to remove the existing VCS (.git, .svn..) history?</info> [<comment>y,n</comment>]? ".to_string(),
                    true,
                ));
        if remove_vcs {
            let mut finder = Finder::new();
            finder
                .depth(0)
                .directories()
                .r#in(&Platform::get_cwd(false)?)
                .ignore_vcs(false)
                .ignore_dot_files(false);
            for vcs_name in [
                ".svn",
                "_svn",
                "CVS",
                "_darcs",
                ".arch-params",
                ".monotone",
                ".bzr",
                ".git",
                ".hg",
                ".fslckout",
                "_FOSSIL_",
            ]
            .iter()
            {
                finder.name(vcs_name);
            }

            // PHP: try { $dirs = iterator_to_array($finder); ... } catch (\Exception $e) { ... }
            let dirs: Vec<String> = finder.iter().map(|f| f.get_pathname()).collect();
            drop(finder);
            let mut had_error: Option<anyhow::Error> = None;
            for dir in &dirs {
                if !fs.remove_directory(dir)? {
                    had_error = Some(
                        RuntimeException {
                            message: format!("Could not remove {}", dir),
                            code: 0,
                        }
                        .into(),
                    );
                    break;
                }
            }
            if let Some(e) = had_error {
                io.write_error(&format!(
                    "<error>An error occurred while removing the VCS metadata: {}</error>",
                    e
                ));
            }

            has_vcs = false;
        }

        // rewriting self.version dependencies with explicit version numbers if the package's vcs metadata is gone
        if !has_vcs {
            let package = composer.get_package();
            let mut config_source = JsonConfigSource::new(
                JsonFile::new("composer.json".to_string(), None, None)?,
                false,
            );
            for (r#type, meta) in SUPPORTED_LINK_TYPES.iter() {
                // PHP: $package->{'get'.$meta['method']}() — dynamic getter dispatch
                // TODO(phase-b): dynamic getter dispatch by name
                let _method = format!("get{}", meta.method);
                let links: Vec<crate::package::Link> = vec![];
                for link in links {
                    if link.get_pretty_constraint().as_deref().ok() == Some("self.version") {
                        config_source.add_link(
                            r#type,
                            link.get_target(),
                            &package.get_pretty_version(),
                        );
                    }
                }
            }
        }

        // dispatch event
        composer
            .get_event_dispatcher()
            .borrow_mut()
            .dispatch_script(
                ScriptEvents::POST_CREATE_PROJECT_CMD,
                install_dev_packages,
                vec![],
                indexmap::IndexMap::new(),
            );

        chdir(&old_cwd);

        Ok(0)
    }

    /// @param array<string>|null $repositories
    ///
    /// @throws \Exception
    #[allow(clippy::too_many_arguments)]
    fn install_root_package(
        &self,
        input: &dyn InputInterface,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
        package_name: &str,
        platform_requirement_filter: &dyn PlatformRequirementFilterInterface,
        directory: Option<String>,
        mut package_version: Option<String>,
        mut stability: Option<String>,
        prefer_source: bool,
        prefer_dist: bool,
        _install_dev_packages: bool,
        repositories: Option<&Vec<String>>,
        disable_plugins: bool,
        disable_scripts: bool,
        no_progress: bool,
        secure_http: bool,
    ) -> Result<bool> {
        let parser: VersionParser = VersionParser::new();
        let requirements = parser.parse_name_version_pairs(vec![package_name.to_string()])?;
        let name = strtolower(
            requirements[0]
                .get("name")
                .map(|s| s.as_str())
                .unwrap_or(""),
        );
        if package_version.is_none() && requirements[0].contains_key("version") {
            package_version = requirements[0].get("version").cloned();
        }

        // if no directory was specified, use the 2nd part of the package name
        let mut directory = if directory.is_none() {
            let mut parts = explode_with_limit("/", &name, 2);
            format!(
                "{}{}{}",
                Platform::get_cwd(false)?,
                DIRECTORY_SEPARATOR,
                array_pop(&mut parts).unwrap_or_default()
            )
        } else {
            directory.unwrap()
        };
        directory = rtrim(&directory, Some("/\\"));

        let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
            io.clone(),
        ))));
        let fs = std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(Some(process))));
        if !fs.borrow().is_absolute_path(&directory) {
            directory = format!(
                "{}{}{}",
                Platform::get_cwd(false)?,
                DIRECTORY_SEPARATOR,
                directory
            );
        }
        if "" == directory {
            return Err(UnexpectedValueException {
                message: "Got an empty target directory, something went wrong".to_string(),
                code: 0,
            }
            .into());
        }

        // set the base dir to ensure $config->all() below resolves the correct absolute paths to vendor-dir etc
        config.borrow_mut().set_base_dir(Some(directory.clone()));
        if !secure_http {
            let mut merge_map: indexmap::IndexMap<String, PhpMixed> = indexmap::IndexMap::new();
            let mut inner_map: indexmap::IndexMap<String, Box<PhpMixed>> =
                indexmap::IndexMap::new();
            inner_map.insert("secure-http".to_string(), Box::new(PhpMixed::Bool(false)));
            merge_map.insert("config".to_string(), PhpMixed::Array(inner_map));
            config
                .borrow_mut()
                .merge(&merge_map, Config::SOURCE_COMMAND);
        }

        io.write_error(&format!(
            "<info>Creating a \"{}\" project at \"{}\"</info>",
            package_name,
            fs.borrow()
                .find_shortest_path(&Platform::get_cwd(false)?, &directory, true, false)
        ));

        if file_exists(&directory) {
            if !is_dir(&directory) {
                return Err(InvalidArgumentException {
                    message: format!(
                        "Cannot create project directory at \"{}\", it exists as a file.",
                        directory
                    ),
                    code: 0,
                }
                .into());
            }
            if !fs.borrow().is_dir_empty(&directory) {
                return Err(InvalidArgumentException {
                    message: format!("Project directory \"{}\" is not empty.", directory),
                    code: 0,
                }
                .into());
            }
        }

        if stability.is_none() {
            if package_version.is_none() {
                stability = Some("stable".to_string());
            } else if {
                let mut matched: IndexMap<CaptureKey, String> = IndexMap::new();
                let ok = Preg::is_match_strict_groups3(
                    &format!(
                        "{{^[^,\\s]*?@({})$}}i",
                        implode(
                            "|",
                            &STABILITIES
                                .keys()
                                .map(|k| k.to_string())
                                .collect::<Vec<_>>()
                        )
                    ),
                    package_version.as_deref().unwrap_or(""),
                    Some(&mut matched),
                )
                .unwrap_or(false);
                if ok {
                    stability = Some(
                        matched
                            .get(&CaptureKey::ByIndex(1))
                            .cloned()
                            .unwrap_or_default(),
                    );
                }
                ok
            } {
                // stability already set above
            } else {
                stability = Some(VersionParser::parse_stability(
                    package_version.as_deref().unwrap_or(""),
                ));
            }
        }

        let stability = VersionParser::normalize_stability(stability.as_deref().unwrap_or(""))
            .unwrap_or_default();

        if !STABILITIES.contains_key(stability.as_str()) {
            return Err(InvalidArgumentException {
                message: format!(
                    "Invalid stability provided ({}), must be one of: {}",
                    stability,
                    implode(
                        ", ",
                        &STABILITIES
                            .keys()
                            .map(|k| k.to_string())
                            .collect::<Vec<_>>()
                    )
                ),
                code: 0,
            }
            .into());
        }

        let composer_handle = self.create_composer_instance(
            input,
            io.clone(),
            Some(config.borrow_mut().all(0)?),
            disable_plugins,
            Some(disable_scripts),
        )?;
        let composer = crate::command::composer_full(&composer_handle);
        let config = composer.get_config();
        // set the base dir here again on the new config instance, as otherwise in case the vendor dir is defined in an env var for example it would still override the value set above by $config->all()
        config.borrow_mut().set_base_dir(Some(directory.clone()));
        let rm = composer.get_repository_manager();

        let mut repository_set = RepositorySet::new(
            &stability,
            indexmap::IndexMap::new(),
            vec![],
            indexmap::IndexMap::new(),
            indexmap::IndexMap::new(),
            indexmap::IndexMap::new(),
        );
        if repositories.is_none() {
            // TODO(phase-b): default_repos needs &mut RepositoryManager but we hold &RepositoryManager.
            let _ = rm;
            repository_set.add_repository(crate::repository::RepositoryInterfaceHandle::new(
                CompositeRepository::new(
                    RepositoryFactory::default_repos(Some(io.clone()), Some(config.clone()), None)?
                        .into_iter()
                        .map(|(_, v)| v)
                        .collect(),
                ),
            ))?;
        } else {
            for repo in repositories.unwrap() {
                let mut repo_config =
                    RepositoryFactory::config_from_string(io.clone(), &config, repo, true)?;
                let is_packagist_disabled = (repo_config.contains_key("packagist")
                    && repo_config.len() == 1
                    && repo_config.get("packagist").and_then(|v| v.as_bool()) == Some(false))
                    || (repo_config.contains_key("packagist.org")
                        && repo_config.len() == 1
                        && repo_config.get("packagist.org").and_then(|v| v.as_bool())
                            == Some(false));
                if is_packagist_disabled {
                    continue;
                }

                // disable symlinking for the root package by default as that most likely makes no sense
                let is_path_type =
                    repo_config.get("type").and_then(|v| v.as_string()) == Some("path");
                let has_symlink_option = repo_config
                    .get("options")
                    .and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m.contains_key("symlink")),
                        _ => None,
                    })
                    .unwrap_or(false);
                if is_path_type && !has_symlink_option {
                    let options_entry = repo_config
                        .entry("options".to_string())
                        .or_insert(PhpMixed::Array(indexmap::IndexMap::new()));
                    if let PhpMixed::Array(options_map) = options_entry {
                        options_map.insert("symlink".to_string(), Box::new(PhpMixed::Bool(false)));
                    }
                }

                repository_set.add_repository(RepositoryFactory::create_repo(
                    io.clone(),
                    &config,
                    repo_config.clone(),
                    None,
                )?);
            }
        }

        let platform_overrides = config.borrow_mut().get("platform");
        let platform_repo = PlatformRepository::new(
            vec![],
            match platform_overrides {
                PhpMixed::Array(m) => m
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            PhpMixed::String(v.as_string().unwrap_or("").to_string()),
                        )
                    })
                    .collect(),
                _ => indexmap::IndexMap::new(),
            },
        )?;

        // find the latest version if there are multiple
        let mut version_selector = VersionSelector::new(repository_set, Some(&platform_repo))?;
        // TODO(phase-b): platform_requirement_filter is &dyn here but VersionSelector expects
        // Option<Box<dyn ...>>; pass None as placeholder.
        let _ = platform_requirement_filter;
        let package = version_selector.find_best_candidate(
            &name,
            package_version.as_deref(),
            &stability,
            None,
            0,
            Some(io.clone()),
            PhpMixed::Bool(true),
        )?;

        if package.is_none() {
            let error_message = format!(
                "Could not find package {} with {}",
                name,
                if let Some(v) = package_version.as_ref() {
                    format!("version {}", v)
                } else {
                    format!("stability {}", stability)
                }
            );
            let is_ignore_all = platform_requirement_filter
                .as_any()
                .downcast_ref::<IgnoreAllPlatformRequirementFilter>()
                .is_some();
            if !is_ignore_all
                && version_selector
                    .find_best_candidate(
                        &name,
                        package_version.as_deref(),
                        &stability,
                        Some(PlatformRequirementFilterFactory::ignore_all()),
                        0,
                        None,
                        PhpMixed::Bool(true),
                    )?
                    .is_some()
            {
                return Err(InvalidArgumentException {
                    message: format!(
                        "{} in a version installable using your PHP version, PHP extensions and Composer version.",
                        error_message
                    ),
                    code: 0,
                }
                .into());
            }

            return Err(InvalidArgumentException {
                message: format!("{}.", error_message),
                code: 0,
            }
            .into());
        }
        let mut package = package.unwrap();

        // handler Ctrl+C aborts gracefully
        let _ = mkdir(&directory, 0o777, true);
        let mut signal_handler: Option<SignalHandler> = None;
        if let Some(real_dir) = realpath(&directory) {
            let real_dir_clone = real_dir.clone();
            signal_handler = Some(SignalHandler::create(
                vec![
                    SignalHandler::SIGINT.to_string(),
                    SignalHandler::SIGTERM.to_string(),
                    SignalHandler::SIGHUP.to_string(),
                ],
                Box::new(move |signal: String, handler: &SignalHandler| {
                    // TODO(phase-b): self.get_io().write_error(...) inside the closure
                    let _ = &signal;
                    let mut fs = Filesystem::new(None);
                    fs.remove_directory(&real_dir_clone).ok();
                    handler.exit_with_last_signal();
                }),
            ));
        }

        // avoid displaying 9999999-dev as version if default-branch was selected
        if let Some(alias) = package.as_alias() {
            if package.get_pretty_version() == VersionParser::DEFAULT_BRANCH_ALIAS {
                package = alias.get_alias_of().into();
            }
        }

        io.write_error(&format!(
            "<info>Installing {} ({})</info>",
            package.get_name(),
            package.get_full_pretty_version(
                false,
                <dyn crate::package::PackageInterface>::DISPLAY_SOURCE_REF_IF_DEV
            )
        ));

        if disable_plugins {
            io.write_error("<info>Plugins have been disabled.</info>");
        }

        if let Some(alias) = package.as_alias() {
            package = alias.get_alias_of().into();
        }

        let dm = composer.get_download_manager();
        dm.borrow_mut()
            .set_prefer_source(prefer_source)
            .set_prefer_dist(prefer_dist);

        let project_installer = ProjectInstaller::new(&directory, dm.clone(), fs.clone());
        let installation_manager = composer.get_installation_manager().clone();
        let mut im = installation_manager.borrow_mut();
        im.set_output_progress(!no_progress);
        im.add_installer(Box::new(project_installer));
        let mut installed_repo = InstalledArrayRepository::new()?;
        im.execute(
            &mut installed_repo,
            vec![Box::new(InstallOperation::new(package.clone()))],
            true,
            true,
            false,
        )?;
        im.notify_installs(io.clone());

        // collect suggestions
        // TODO(phase-b): self.suggested_packages_reporter is on the outer scope via &self
        // self.suggested_packages_reporter.add_suggestions_from_package(&*package);

        let installed_from_vcs = package.get_installation_source().as_deref() == Some("source");

        io.write_error(&format!("<info>Created project in {}</info>", directory));
        chdir(&directory);

        // ensure that the env var being set does not interfere with create-project
        // as it is probably not meant to be used here, so we do not use it if a composer.json can be found
        // in the project
        if file_exists(&format!("{}/composer.json", directory))
            && Platform::get_env("COMPOSER").is_some()
        {
            Platform::clear_env("COMPOSER");
        }

        Platform::put_env("COMPOSER_ROOT_VERSION", &package.get_pretty_version());

        // once the root project is fully initialized, we do not need to wipe everything on user abort anymore even if it happens during deps install
        if let Some(handler) = signal_handler {
            handler.unregister();
        }

        Ok(installed_from_vcs)
    }

    // helpers reachable via $this in PHP, defined on BaseCommand here
    fn create_composer_instance(
        &self,
        input: &dyn InputInterface,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: Option<indexmap::IndexMap<String, PhpMixed>>,
        disable_plugins: bool,
        disable_scripts: Option<bool>,
    ) -> Result<PartialComposerHandle> {
        self.create_composer_instance(input, io, config, disable_plugins, disable_scripts)
    }

    fn create_audit_config(
        &self,
        config: &Config,
        input: &dyn InputInterface,
    ) -> Result<crate::advisory::AuditConfig> {
        self.create_audit_config(config, input)
    }
}

impl HasBaseCommandData for CreateProjectCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
