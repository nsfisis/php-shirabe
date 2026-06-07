//! ref: composer/src/Composer/Command/ShowCommand.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::composer::semver::Semver;
use shirabe_external_packages::composer::spdx_licenses::SpdxLicenses;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_external_packages::symfony::console::formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::formatter::OutputFormatterStyle;
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, UnexpectedValueException, array_search,
    date, extension_loaded, in_array, realpath, strtolower, version_compare,
};

use shirabe_semver::constraint::AnyConstraint;

use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::composer::PartialComposerHandle;
use crate::console::input::InputOption;
use crate::dependency_resolver::DefaultPolicy;
use crate::dependency_resolver::PolicyInterface;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::json::JsonFile;
use crate::package::CompletePackageInterfaceHandle;
use crate::package::Link;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::package::base_package;
use crate::package::version::VersionParser;
use crate::package::version::VersionSelector;
use crate::plugin::CommandEvent;
use crate::plugin::PluginEvents;
use crate::repository::CompositeRepository;
use crate::repository::FilterRepository;
use crate::repository::InstalledArrayRepository;
use crate::repository::InstalledRepository;
use crate::repository::PlatformRepository;
use crate::repository::PlatformRepositoryHandle;
use crate::repository::RepositoryFactory;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryInterfaceHandle;
use crate::repository::RepositorySet;
use crate::repository::RepositoryUtils;
use crate::repository::RootPackageRepository;
use crate::util::PackageInfo;

// keep InputOption referenced; the configure() definition list is currently abbreviated
#[allow(dead_code)]
const _INPUT_OPTION_REF: i64 = InputOption::VALUE_NONE;

#[derive(Debug)]
pub struct ShowCommand {
    base_command_data: BaseCommandData,

    pub(crate) version_parser: VersionParser,
    pub(crate) colors: Vec<String>,
    repository_set: Option<std::rc::Rc<std::cell::RefCell<RepositorySet>>>,
}

impl ShowCommand {
    pub fn configure(&mut self) {
        self.set_name("show")
            .set_aliases(&["info".to_string()])
            .set_description("Shows information about packages")
            .set_definition(&[
                // TODO(cli-completion): wire up suggest_package_based_on_mode / suggest_installed_package closures here.
            ])
            .set_help(
                "The show command displays detailed information about a package, or\n\
                 lists all packages available.\n\n\
                 Read more at https://getcomposer.org/doc/03-cli.md#show-info",
            );
    }

    // TODO(cli-completion): pub fn suggest_package_based_on_mode(&self) -> Box<dyn Fn(&CompletionInput) -> Vec<String>>

    pub fn execute(
        &mut self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        self.version_parser = VersionParser::new();
        if input.borrow().get_option("tree").as_bool() == Some(true) {
            self.init_styles(output.clone());
        }

        let mut composer = self.try_composer(None, None);

        if input.borrow().get_option("installed").as_bool() == Some(true)
            && input.borrow().get_option("self").as_bool() != Some(true)
        {
            self.get_io().write_error("<warning>You are using the deprecated option \"installed\". Only installed packages are shown by default now. The --all option can be used to show all packages.</warning>");
        }

        if input.borrow().get_option("outdated").as_bool() == Some(true) {
            input
                .borrow_mut()
                .set_option("latest", PhpMixed::Bool(true));
        } else if input
            .borrow()
            .get_option("ignore")
            .as_list()
            .map_or(0, |l| l.len())
            > 0
        {
            self.get_io().write_error("<warning>You are using the option \"ignore\" for action other than \"outdated\", it will be ignored.</warning>");
        }

        if input.borrow().get_option("direct").as_bool() == Some(true)
            && (input.borrow().get_option("all").as_bool() == Some(true)
                || input.borrow().get_option("available").as_bool() == Some(true)
                || input.borrow().get_option("platform").as_bool() == Some(true))
        {
            self.get_io().write_error("The --direct (-D) option is not usable in combination with --all, --platform (-p) or --available (-a)");

            return Ok(1);
        }

        if input.borrow().get_option("tree").as_bool() == Some(true)
            && (input.borrow().get_option("all").as_bool() == Some(true)
                || input.borrow().get_option("available").as_bool() == Some(true))
        {
            self.get_io().write_error("The --tree (-t) option is not usable in combination with --all or --available (-a)");

            return Ok(1);
        }

        let only_count: usize = [
            input.borrow().get_option("patch-only").as_bool() == Some(true),
            input.borrow().get_option("minor-only").as_bool() == Some(true),
            input.borrow().get_option("major-only").as_bool() == Some(true),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if only_count > 1 {
            self.get_io().write_error(
                "Only one of --major-only, --minor-only or --patch-only can be used at once",
            );

            return Ok(1);
        }

        if input.borrow().get_option("tree").as_bool() == Some(true)
            && input.borrow().get_option("latest").as_bool() == Some(true)
        {
            self.get_io().write_error(
                "The --tree (-t) option is not usable in combination with --latest (-l)",
            );

            return Ok(1);
        }

        if input.borrow().get_option("tree").as_bool() == Some(true)
            && input.borrow().get_option("path").as_bool() == Some(true)
        {
            self.get_io().write_error(
                "The --tree (-t) option is not usable in combination with --path (-P)",
            );

            return Ok(1);
        }

        let format = input
            .borrow()
            .get_option("format")
            .as_string()
            .unwrap_or("text")
            .to_string();
        if !in_array(
            PhpMixed::String(format.clone()),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::String("text".to_string())),
                Box::new(PhpMixed::String("json".to_string())),
            ]),
            false,
        ) {
            self.get_io().write_error(&format!(
                "Unsupported format \"{}\". See help for supported formats.",
                format
            ));

            return Ok(1);
        }

        let platform_req_filter = self.get_platform_requirement_filter(input.clone())?;

        // init repos
        let mut platform_overrides: IndexMap<String, PhpMixed> = IndexMap::new();
        if let Some(ref composer) = composer {
            let composer = crate::command::composer_full(composer);
            if let Some(p) = composer
                .get_config()
                .borrow()
                .get("platform")
                .as_array()
                .cloned()
            {
                platform_overrides = p.into_iter().map(|(k, v)| (k, *v)).collect();
            }
        }
        let platform_repo =
            PlatformRepositoryHandle::new(PlatformRepository::new(vec![], platform_overrides)?);
        let mut locked_repo: Option<RepositoryInterfaceHandle> = None;

        // The single-package $package binding from PHP gets surfaced here.
        let mut single_package: Option<crate::package::CompletePackageInterfaceHandle> = None;
        let mut versions_map: IndexMap<String, String> = IndexMap::new();
        let installed_repo: RepositoryInterfaceHandle;
        let repos: RepositoryInterfaceHandle;

        if input.borrow().get_option("self").as_bool() == Some(true)
            && input.borrow().get_option("installed").as_bool() != Some(true)
            && input.borrow().get_option("locked").as_bool() != Some(true)
        {
            let composer = self.require_composer(None, None)?;
            let package = crate::package::RootPackageInterfaceHandle::dup(
                composer.borrow_partial().get_package(),
            );
            if input.borrow().get_option("name-only").as_bool() == Some(true) {
                self.get_io().write(&package.get_name());

                return Ok(0);
            }
            if input.borrow().get_argument("package").as_string().is_some() {
                return Err(InvalidArgumentException {
                    message: "You cannot use --self together with a package name".to_string(),
                    code: 0,
                }
                .into());
            }
            installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                RepositoryInterfaceHandle::new(RootPackageRepository::new(package.clone())),
            ]));
            repos = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                RepositoryInterfaceHandle::new(RootPackageRepository::new(package.clone())),
            ]));
            single_package = Some(package.clone().into());
        } else if input.borrow().get_option("platform").as_bool() == Some(true) {
            installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                platform_repo.clone().into(),
            ]));
            repos = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                platform_repo.clone().into(),
            ]));
        } else if input.borrow().get_option("available").as_bool() == Some(true) {
            let mut ir = InstalledRepository::new(vec![platform_repo.clone().into()]);
            if let Some(ref composer) = composer {
                let composer = crate::command::composer_full(composer);
                repos = RepositoryInterfaceHandle::new(CompositeRepository::new(
                    composer
                        .get_repository_manager()
                        .borrow()
                        .get_repositories()
                        .iter()
                        .map(|r| r.clone())
                        .collect(),
                ));
                ir.add_repository(
                    composer
                        .get_repository_manager()
                        .borrow()
                        .get_local_repository(),
                );
                installed_repo = RepositoryInterfaceHandle::new(ir);
            } else {
                let default_repos =
                    RepositoryFactory::default_repos_with_default_manager(self.get_io())?;
                let names: Vec<String> = default_repos.keys().cloned().collect();
                repos = RepositoryInterfaceHandle::new(CompositeRepository::new(
                    default_repos.into_values().collect(),
                ));
                self.get_io().write_error(&format!(
                    "No composer.json found in the current directory, showing available packages from {}",
                    names.join(", ")
                ));
                installed_repo = RepositoryInterfaceHandle::new(ir);
            }
        } else if input.borrow().get_option("all").as_bool() == Some(true) && composer.is_some() {
            let mut composer_ref = crate::command::composer_full_mut(composer.as_ref().unwrap());
            let local_repo = composer_ref
                .get_repository_manager()
                .borrow()
                .get_local_repository();
            let locker_rc = composer_ref.get_locker().clone();
            let mut locker = locker_rc.borrow_mut();
            if locker.is_locked() {
                let lr_handle: RepositoryInterfaceHandle =
                    locker.get_locked_repository(true)?.into();
                installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                    lr_handle.clone(),
                    local_repo,
                    platform_repo.clone().into(),
                ]));
                locked_repo = Some(lr_handle);
            } else {
                installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                    local_repo,
                    platform_repo.clone().into(),
                ]));
            }
            let mut composite_input: Vec<RepositoryInterfaceHandle> =
                vec![RepositoryInterfaceHandle::new(FilterRepository::new(
                    installed_repo.clone(),
                    {
                        let mut m = IndexMap::new();
                        m.insert("canonical".to_string(), PhpMixed::Bool(false));
                        m
                    },
                )?)];
            for r in composer_ref
                .get_repository_manager()
                .borrow()
                .get_repositories()
            {
                composite_input.push(r.clone());
            }
            repos = RepositoryInterfaceHandle::new(CompositeRepository::new(composite_input));
        } else if input.borrow().get_option("all").as_bool() == Some(true) {
            let default_repos =
                RepositoryFactory::default_repos_with_default_manager(self.get_io())?;
            let names: Vec<String> = default_repos.keys().cloned().collect();
            self.get_io().write_error(&format!(
                "No composer.json found in the current directory, showing available packages from {}",
                names.join(", ")
            ));
            installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                platform_repo.clone().into(),
            ]));
            let mut composite_input: Vec<RepositoryInterfaceHandle> = vec![installed_repo.clone()];
            for (_k, v) in default_repos.into_iter() {
                composite_input.push(v);
            }
            repos = RepositoryInterfaceHandle::new(CompositeRepository::new(composite_input));
        } else if input.borrow().get_option("locked").as_bool() == Some(true) {
            if composer.is_none()
                || !crate::command::composer_full_mut(composer.as_ref().unwrap())
                    .get_locker()
                    .borrow_mut()
                    .is_locked()
            {
                return Err(UnexpectedValueException {
                    message: "A valid composer.json and composer.lock files is required to run this command with --locked".to_string(),
                    code: 0,
                }
                .into());
            }
            let mut composer_ref = crate::command::composer_full_mut(composer.as_ref().unwrap());
            let locker_rc = composer_ref.get_locker().clone();
            let mut locker = locker_rc.borrow_mut();
            let lr = locker.get_locked_repository(
                input.borrow().get_option("no-dev").as_bool() != Some(true),
            )?;
            let lr_handle: RepositoryInterfaceHandle = lr.into();
            if input.borrow().get_option("self").as_bool() == Some(true) {
                // TODO(phase-b): LockArrayRepository needs add_package via WritableRepositoryInterface;
                // skipping the insertion here keeps compile clean.
                let _ = &lr_handle;
            }
            installed_repo =
                RepositoryInterfaceHandle::new(InstalledRepository::new(vec![lr_handle.clone()]));
            repos =
                RepositoryInterfaceHandle::new(InstalledRepository::new(vec![lr_handle.clone()]));
            locked_repo = Some(lr_handle);
        } else {
            // --installed / default case
            let composer_local_owned;
            let _guard_from_existing;
            let composer_local = match composer.as_ref() {
                Some(c) => {
                    _guard_from_existing = crate::command::composer_full(c);
                    &*_guard_from_existing
                }
                None => {
                    composer_local_owned = self.require_composer(None, None)?;
                    _guard_from_existing = crate::command::composer_full(&composer_local_owned);
                    &*_guard_from_existing
                }
            };
            let root_pkg = composer_local.get_package();

            let root_repo: RepositoryInterfaceHandle =
                if input.borrow().get_option("self").as_bool() == Some(true) {
                    RepositoryInterfaceHandle::new(RootPackageRepository::new(
                        crate::package::RootPackageInterfaceHandle::dup(
                            composer_local.get_package(),
                        ),
                    ))
                } else {
                    RepositoryInterfaceHandle::new(InstalledArrayRepository::new()?)
                };
            if input.borrow().get_option("no-dev").as_bool() == Some(true) {
                let local_packages = composer_local
                    .get_repository_manager()
                    .borrow()
                    .get_local_repository()
                    .get_packages()?;
                let packages = RepositoryUtils::filter_required_packages(
                    &local_packages,
                    root_pkg.clone().into(),
                    false,
                    Vec::new(),
                );
                let cloned: Vec<crate::package::PackageInterfaceHandle> = packages
                    .iter()
                    .map(crate::package::PackageInterfaceHandle::dup)
                    .collect();
                installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                    root_repo.clone(),
                    RepositoryInterfaceHandle::new(InstalledArrayRepository::new_with_packages(
                        cloned,
                    )?),
                ]));
                repos = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                    root_repo,
                    RepositoryInterfaceHandle::new(InstalledArrayRepository::new_with_packages(
                        Vec::new(),
                    )?),
                ]));
            } else {
                let repository_manager = composer_local.get_repository_manager().clone();
                let repository_manager = repository_manager.borrow();
                let lr = repository_manager.get_local_repository();
                installed_repo = RepositoryInterfaceHandle::new(InstalledRepository::new(vec![
                    root_repo.clone(),
                    lr.clone(),
                ]));
                repos =
                    RepositoryInterfaceHandle::new(InstalledRepository::new(vec![root_repo, lr]));
            }

            if installed_repo.get_packages()?.is_empty() {
                let has_non_platform_reqs = |reqs: &IndexMap<String, Link>| -> bool {
                    reqs.keys()
                        .any(|name| !PlatformRepository::is_platform_package(name))
                };

                if has_non_platform_reqs(&root_pkg.get_requires())
                    || has_non_platform_reqs(&root_pkg.get_dev_requires())
                {
                    // Borrow is local; release composer_local borrow first.
                    let _ = root_pkg;
                    self.get_io().write_error("<warning>No dependencies installed. Try running composer install or update.</warning>");
                }
            }
        }

        if let Some(ref composer) = composer {
            let composer = crate::command::composer_full(composer);
            let command_event = CommandEvent::new6(
                PluginEvents::COMMAND,
                "show",
                input.clone(),
                output,
                vec![],
                IndexMap::new(),
            );
            // TODO(phase-b): EventDispatcher::dispatch wants Option<Event>, but PHP passes
            // the CommandEvent subclass directly. Phase D will introduce trait-based dispatch.
            let _event_name = command_event.get_name().to_string();
            composer
                .get_event_dispatcher()
                .borrow_mut()
                .dispatch(Some(&_event_name), None)?;
        }

        if input.borrow().get_option("latest").as_bool() == Some(true) && composer.is_none() {
            self.get_io().write_error(
                "No composer.json found in the current directory, disabling \"latest\" option",
            );
            input
                .borrow_mut()
                .set_option("latest", PhpMixed::Bool(false));
        }

        let package_filter: Option<String> = input
            .borrow()
            .get_argument("package")
            .as_string()
            .map(|s| s.to_string());

        // show single package or single version
        if let Some(ref pkg) = single_package {
            versions_map.insert(pkg.get_pretty_version(), pkg.get_version());
        } else if let Some(ref pf) = package_filter {
            if !pf.contains('*') {
                let (matched_package, vers) = self.get_package(
                    &*installed_repo.borrow(),
                    &repos,
                    pf,
                    input.borrow().get_argument("version"),
                )?;

                if let Some(ref pkg) = matched_package {
                    if input.borrow().get_option("direct").as_bool() == Some(true) {
                        if !in_array(
                            PhpMixed::String(pkg.get_name()),
                            &PhpMixed::List(
                                self.get_root_requires()
                                    .into_iter()
                                    .map(|s| Box::new(PhpMixed::String(s)))
                                    .collect(),
                            ),
                            true,
                        ) {
                            return Err(InvalidArgumentException {
                                message: format!(
                                    "Package \"{}\" is installed but not a direct dependent of the root package.",
                                    pkg.get_name()
                                ),
                                code: 0,
                            }
                            .into());
                        }
                    }
                }

                if matched_package.is_none() {
                    let options = input.borrow().get_options();
                    let mut hint = String::new();
                    if input.borrow().get_option("locked").as_bool() == Some(true) {
                        hint.push_str(" in lock file");
                    }
                    if options.contains_key("working-dir") {
                        hint.push_str(&format!(
                            " in {}/composer.json",
                            options
                                .get("working-dir")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                        ));
                    }
                    if PlatformRepository::is_platform_package(pf)
                        && input.borrow().get_option("platform").as_bool() != Some(true)
                    {
                        hint.push_str(", try using --platform (-p) to show platform packages");
                    }
                    if input.borrow().get_option("all").as_bool() != Some(true)
                        && input.borrow().get_option("available").as_bool() != Some(true)
                    {
                        hint.push_str(
                            ", try using --available (-a) to show all available packages",
                        );
                    }

                    return Err(InvalidArgumentException {
                        message: format!("Package \"{}\" not found{}.", pf, hint),
                        code: 0,
                    }
                    .into());
                }
                single_package = matched_package;
                versions_map = vers;
            }
        }

        if let Some(ref package) = single_package {
            // assert(isset($versions));

            let mut exit_code: i64 = 0;
            if input.borrow().get_option("tree").as_bool() == Some(true) {
                let array_tree = self.generate_package_tree(
                    package.clone().into(),
                    &*installed_repo.borrow(),
                    &repos,
                );

                if format == "json" {
                    let mut wrapper: IndexMap<String, PhpMixed> = IndexMap::new();
                    wrapper.insert(
                        "installed".to_string(),
                        PhpMixed::List(vec![Box::new(PhpMixed::Array(
                            array_tree
                                .into_iter()
                                .map(|(k, v)| (k, Box::new(v)))
                                .collect(),
                        ))]),
                    );
                    self.get_io().write(&JsonFile::encode(&PhpMixed::Array(
                        wrapper.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                    )));
                } else {
                    self.display_package_tree(vec![array_tree]);
                }

                return Ok(exit_code);
            }

            let mut latest_package: Option<crate::package::PackageInterfaceHandle> = None;
            if input.borrow().get_option("latest").as_bool() == Some(true) {
                latest_package = self.find_latest_package(
                    package.clone().into(),
                    composer.as_ref().unwrap(),
                    &platform_repo,
                    input
                        .borrow()
                        .get_option("major-only")
                        .as_bool()
                        .unwrap_or(false),
                    input
                        .borrow()
                        .get_option("minor-only")
                        .as_bool()
                        .unwrap_or(false),
                    input
                        .borrow()
                        .get_option("patch-only")
                        .as_bool()
                        .unwrap_or(false),
                    &*platform_req_filter,
                )?;
            }
            if input.borrow().get_option("outdated").as_bool() == Some(true)
                && input.borrow().get_option("strict").as_bool() == Some(true)
                && latest_package.is_some()
                && latest_package
                    .as_ref()
                    .unwrap()
                    .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
                    != package
                        .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
                && (latest_package
                    .as_ref()
                    .unwrap()
                    .as_complete()
                    .map_or(true, |c| !c.is_abandoned()))
            {
                exit_code = 1;
            }
            if input.borrow().get_option("path").as_bool() == Some(true) {
                self.get_io().write_no_newline(&package.get_name());
                let path = {
                    let composer_ref = composer.as_ref().unwrap();
                    composer_ref
                        .borrow_partial()
                        .get_installation_manager()
                        .borrow_mut()
                        .get_install_path(package.clone().into())
                };
                if let Some(path) = path {
                    let real = realpath(&path).unwrap_or_default();
                    let trimmed = real.split(|c| c == '\r' || c == '\n').next().unwrap_or("");
                    self.get_io().write(&format!(" {}", trimmed));
                } else {
                    self.get_io().write(" null");
                }

                return Ok(exit_code);
            }

            if format == "json" {
                self.print_package_info_as_json(
                    package.clone(),
                    &versions_map,
                    &*installed_repo.borrow(),
                    latest_package,
                )?;
            } else {
                self.print_package_info(
                    package.clone(),
                    &versions_map,
                    &mut *installed_repo.borrow_mut(),
                    latest_package,
                )?;
            }

            return Ok(exit_code);
        }

        // show tree view if requested
        if input.borrow().get_option("tree").as_bool() == Some(true) {
            let root_requires = self.get_root_requires();
            let mut packages = installed_repo.get_packages()?;
            packages.sort_by(|a, b| {
                let sa: String = a.to_string();
                let sb: String = b.to_string();
                sa.cmp(&sb)
            });
            let mut array_tree: Vec<IndexMap<String, PhpMixed>> = Vec::new();
            for package in packages.iter() {
                if in_array(
                    PhpMixed::String(package.get_name()),
                    &PhpMixed::List(
                        root_requires
                            .iter()
                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                            .collect(),
                    ),
                    true,
                ) {
                    array_tree.push(self.generate_package_tree(
                        package.clone(),
                        &*installed_repo.borrow(),
                        &repos,
                    ));
                }
            }

            if format == "json" {
                let mut wrapper: IndexMap<String, PhpMixed> = IndexMap::new();
                wrapper.insert(
                    "installed".to_string(),
                    PhpMixed::List(
                        array_tree
                            .into_iter()
                            .map(|m| {
                                Box::new(PhpMixed::Array(
                                    m.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                                ))
                            })
                            .collect(),
                    ),
                );
                self.get_io().write(&JsonFile::encode(&PhpMixed::Array(
                    wrapper.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                )));
            } else {
                self.display_package_tree(array_tree);
            }

            return Ok(0);
        }

        // list packages
        let mut packages: IndexMap<String, IndexMap<String, PackageOrName>> = IndexMap::new();
        let mut package_filter_regex: Option<String> = None;
        if let Some(ref pf) = package_filter {
            let escaped = shirabe_php_shim::preg_quote(pf, None);
            package_filter_regex = Some(format!("{{^{}$}}i", escaped.replace("\\*", ".*?")));
        }

        let mut package_list_filter: Option<Vec<String>> = None;
        if input.borrow().get_option("direct").as_bool() == Some(true) {
            package_list_filter = Some(self.get_root_requires());
        }

        if input.borrow().get_option("path").as_bool() == Some(true) && composer.is_none() {
            self.get_io().write_error(
                "No composer.json found in the current directory, disabling \"path\" option",
            );
            input.borrow_mut().set_option("path", PhpMixed::Bool(false));
        }

        for repo in RepositoryUtils::flatten_repositories(repos.clone(), false) {
            // TODO(phase-b): InstalledRepository needs as_repository_interface / get_repositories
            // wired through; placeholder classification until then.
            let r#type = if Self::same_repository(&repo, &platform_repo) {
                "platform"
            } else if let Some(ref lr) = locked_repo {
                if Self::same_repository(&repo, lr) {
                    "locked"
                } else {
                    "available"
                }
            } else {
                "available"
            };
            let type_owned = r#type.to_string();
            // TODO(phase-b): RepositoryInterface needs as_composer_repository_mut downcast helper
            if false {
                let _ = package_filter.as_deref();
            } else {
                for package in repo.get_packages()? {
                    let existing = packages
                        .get(&type_owned)
                        .and_then(|m| m.get(&package.get_name()));
                    let need_replace = match existing {
                        None => true,
                        Some(PackageOrName::Name(_)) => true,
                        Some(PackageOrName::Pkg(existing)) => {
                            version_compare(&existing.get_version(), &package.get_version(), "<")
                        }
                    };
                    if need_replace {
                        let mut p: crate::package::PackageInterfaceHandle = package.clone().into();
                        while let Some(alias) = p.as_alias() {
                            p = alias.get_alias_of().into();
                        }
                        let matches_filter = match &package_filter_regex {
                            None => true,
                            Some(r) => Preg::is_match(r, &p.get_name())?,
                        };
                        if matches_filter {
                            let matches_list = match &package_list_filter {
                                None => true,
                                Some(list) => in_array(
                                    PhpMixed::String(p.get_name()),
                                    &PhpMixed::List(
                                        list.iter()
                                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                                            .collect(),
                                    ),
                                    true,
                                ),
                            };
                            if matches_list {
                                packages
                                    .entry(type_owned.clone())
                                    .or_insert_with(IndexMap::new)
                                    .insert(p.get_name(), PackageOrName::Pkg(p));
                            }
                        }
                    }
                }
                if Self::same_repository(&repo, &platform_repo) {
                    for (name, p) in platform_repo.borrow().get_disabled_packages() {
                        packages
                            .entry(type_owned.clone())
                            .or_insert_with(IndexMap::new)
                            .insert(name.clone(), PackageOrName::Pkg(p.clone().into()));
                    }
                }
            }
        }

        let show_all_types = input.borrow().get_option("all").as_bool() == Some(true);
        let show_latest = input.borrow().get_option("latest").as_bool() == Some(true);
        let show_major_only = input.borrow().get_option("major-only").as_bool() == Some(true);
        let show_minor_only = input.borrow().get_option("minor-only").as_bool() == Some(true);
        let show_patch_only = input.borrow().get_option("patch-only").as_bool() == Some(true);
        let ignored_packages_regex = base_package::package_names_to_regexp(
            &input
                .borrow()
                .get_option("ignore")
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| strtolower(s)))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            "{^(?:%s)$}iD",
        );
        let indent = if show_all_types { "  " } else { "" };
        let mut latest_packages: IndexMap<String, crate::package::PackageInterfaceHandle> =
            IndexMap::new();
        let mut exit_code: i64 = 0;
        let mut view_data: IndexMap<String, Vec<IndexMap<String, PhpMixed>>> = IndexMap::new();
        let mut view_meta_data: IndexMap<String, ViewMetaData> = IndexMap::new();

        let mut write_version = false;
        let mut write_description = false;

        let type_order: Vec<(&str, bool)> = vec![
            ("platform", true),
            ("locked", true),
            ("available", false),
            ("installed", true),
        ];
        for (r#type, show_version) in type_order.iter() {
            if let Some(type_packages) = packages.get_mut(*r#type) {
                type_packages.sort_keys();

                let mut name_length: usize = 0;
                let mut version_length: usize = 0;
                let mut latest_length: usize = 0;
                let mut release_date_length: usize = 0;

                if show_latest && *show_version {
                    for package_or_name in type_packages.values() {
                        if let PackageOrName::Pkg(package) = package_or_name {
                            if !Preg::is_match(&ignored_packages_regex, &package.get_pretty_name())?
                            {
                                let latest = self.find_latest_package(
                                    package.clone(),
                                    composer.as_ref().unwrap(),
                                    &platform_repo,
                                    show_major_only,
                                    show_minor_only,
                                    show_patch_only,
                                    &*platform_req_filter,
                                )?;
                                if latest.is_none() {
                                    continue;
                                }

                                latest_packages.insert(package.get_pretty_name(), latest.unwrap());
                            }
                        }
                    }
                }

                let write_path = input.borrow().get_option("name-only").as_bool() != Some(true)
                    && input.borrow().get_option("path").as_bool() == Some(true);
                write_version = input.borrow().get_option("name-only").as_bool() != Some(true)
                    && input.borrow().get_option("path").as_bool() != Some(true)
                    && *show_version;
                let write_latest = write_version && show_latest;
                write_description = input.borrow().get_option("name-only").as_bool() != Some(true)
                    && input.borrow().get_option("path").as_bool() != Some(true);
                let write_release_date = write_latest
                    && (input.borrow().get_option("sort-by-age").as_bool() == Some(true)
                        || format == "json");

                let mut has_outdated_packages = false;

                if input.borrow().get_option("sort-by-age").as_bool() == Some(true) {
                    type_packages.sort_by(|_ka, a, _kb, b| match (a, b) {
                        (PackageOrName::Pkg(a), PackageOrName::Pkg(b)) => {
                            a.get_release_date().cmp(&b.get_release_date())
                        }
                        _ => std::cmp::Ordering::Equal,
                    });
                }

                let mut view_type: Vec<IndexMap<String, PhpMixed>> = Vec::new();
                for package_or_name in type_packages.values() {
                    let mut package_view_data: IndexMap<String, PhpMixed> = IndexMap::new();
                    if let PackageOrName::Pkg(package) = package_or_name {
                        let latest_package = if show_latest
                            && latest_packages.contains_key(&package.get_pretty_name())
                        {
                            latest_packages.get(&package.get_pretty_name())
                        } else {
                            None
                        };

                        // Determine if Composer is checking outdated dependencies and if current package should trigger non-default exit code
                        let mut package_is_up_to_date = if let Some(latest) = latest_package {
                            latest.get_full_pretty_version(
                                true,
                                crate::package::DisplayMode::SourceRefIfDev,
                            ) == package.get_full_pretty_version(
                                true,
                                crate::package::DisplayMode::SourceRefIfDev,
                            ) && latest.as_complete().map_or(true, |c| !c.is_abandoned())
                        } else {
                            false
                        };
                        // When using --major-only, and no bigger version than current major is found then it is considered up to date
                        package_is_up_to_date =
                            package_is_up_to_date || (latest_package.is_none() && show_major_only);
                        let package_is_ignored =
                            Preg::is_match(&ignored_packages_regex, &package.get_pretty_name())?;
                        if input.borrow().get_option("outdated").as_bool() == Some(true)
                            && (package_is_up_to_date || package_is_ignored)
                        {
                            continue;
                        }

                        if input.borrow().get_option("outdated").as_bool() == Some(true)
                            || input.borrow().get_option("strict").as_bool() == Some(true)
                        {
                            has_outdated_packages = true;
                        }

                        package_view_data.insert(
                            "name".to_string(),
                            PhpMixed::String(package.get_pretty_name()),
                        );
                        package_view_data.insert(
                            "direct-dependency".to_string(),
                            PhpMixed::Bool(in_array(
                                PhpMixed::String(package.get_name()),
                                &PhpMixed::List(
                                    self.get_root_requires()
                                        .into_iter()
                                        .map(|s| Box::new(PhpMixed::String(s)))
                                        .collect(),
                                ),
                                true,
                            )),
                        );
                        if format != "json"
                            || input.borrow().get_option("name-only").as_bool() != Some(true)
                        {
                            package_view_data.insert(
                                "homepage".to_string(),
                                match package.as_complete() {
                                    Some(c) => match c.get_homepage() {
                                        Some(h) => PhpMixed::String(h),
                                        None => PhpMixed::Null,
                                    },
                                    None => PhpMixed::Null,
                                },
                            );
                            package_view_data.insert(
                                "source".to_string(),
                                match PackageInfo::get_view_source_url(package.clone()) {
                                    Some(s) => PhpMixed::String(s),
                                    None => PhpMixed::Null,
                                },
                            );
                        }
                        name_length = name_length.max(package.get_pretty_name().len());
                        if write_version {
                            let mut version_str = package.get_full_pretty_version(
                                true,
                                crate::package::DisplayMode::SourceRefIfDev,
                            );
                            if format == "text" {
                                version_str = version_str.trim_start_matches('v').to_string();
                            }
                            version_length = version_length.max(version_str.len());
                            package_view_data
                                .insert("version".to_string(), PhpMixed::String(version_str));
                        }
                        if write_release_date {
                            if let Some(release_date) = package.get_release_date() {
                                let mut age = self
                                    .get_relative_time(&release_date)
                                    .replace(" ago", " old");
                                if !age.contains(" old") {
                                    age = format!("from {}", age);
                                }
                                release_date_length = release_date_length.max(age.len());
                                package_view_data
                                    .insert("release-age".to_string(), PhpMixed::String(age));
                                package_view_data.insert(
                                    "release-date".to_string(),
                                    PhpMixed::String(release_date.to_rfc3339()),
                                );
                            } else {
                                package_view_data.insert(
                                    "release-age".to_string(),
                                    PhpMixed::String(String::new()),
                                );
                                package_view_data.insert(
                                    "release-date".to_string(),
                                    PhpMixed::String(String::new()),
                                );
                            }
                        }
                        if write_latest && latest_package.is_some() {
                            let latest = latest_package.unwrap();
                            let mut latest_version_str = latest.get_full_pretty_version(
                                true,
                                crate::package::DisplayMode::SourceRefIfDev,
                            );
                            if format == "text" {
                                latest_version_str =
                                    latest_version_str.trim_start_matches('v').to_string();
                            }
                            let update_status =
                                Self::get_update_status(latest.clone(), package.clone());
                            latest_length = latest_length.max(latest_version_str.len());
                            package_view_data
                                .insert("latest".to_string(), PhpMixed::String(latest_version_str));
                            package_view_data.insert(
                                "latest-status".to_string(),
                                PhpMixed::String(update_status),
                            );

                            if let Some(rd) = latest.get_release_date() {
                                package_view_data.insert(
                                    "latest-release-date".to_string(),
                                    PhpMixed::String(rd.to_rfc3339()),
                                );
                            } else {
                                package_view_data.insert(
                                    "latest-release-date".to_string(),
                                    PhpMixed::String(String::new()),
                                );
                            }
                        } else if write_latest {
                            package_view_data.insert(
                                "latest".to_string(),
                                PhpMixed::String("[none matched]".to_string()),
                            );
                            package_view_data.insert(
                                "latest-status".to_string(),
                                PhpMixed::String("up-to-date".to_string()),
                            );
                            latest_length = latest_length.max("[none matched]".len());
                        }
                        if write_description {
                            if let Some(c) = package.as_complete() {
                                package_view_data.insert(
                                    "description".to_string(),
                                    PhpMixed::String(c.get_description().unwrap_or_default()),
                                );
                            }
                        }
                        if write_path {
                            let installation_manager = composer
                                .as_ref()
                                .unwrap()
                                .borrow_partial()
                                .get_installation_manager();
                            let path: Option<String> = installation_manager
                                .borrow_mut()
                                .get_install_path(package.clone());
                            if let Some(p) = path {
                                let r = realpath(&p).unwrap_or_default();
                                let trimmed =
                                    r.split(|c| c == '\r' || c == '\n').next().unwrap_or("");
                                package_view_data.insert(
                                    "path".to_string(),
                                    PhpMixed::String(trimmed.to_string()),
                                );
                            } else {
                                package_view_data.insert("path".to_string(), PhpMixed::Null);
                            }
                        }

                        let mut package_is_abandoned: PhpMixed = PhpMixed::Bool(false);
                        if let Some(latest) = latest_package {
                            if let Some(c) = latest.as_complete() {
                                if c.is_abandoned() {
                                    let replacement_package_name = c.get_replacement_package();
                                    let replacement = if let Some(ref rp) = replacement_package_name
                                    {
                                        format!("Use {} instead", rp)
                                    } else {
                                        "No replacement was suggested".to_string()
                                    };
                                    let package_warning = format!(
                                        "Package {} is abandoned, you should avoid using it. {}.",
                                        package.get_pretty_name(),
                                        replacement
                                    );
                                    package_view_data.insert(
                                        "warning".to_string(),
                                        PhpMixed::String(package_warning),
                                    );
                                    package_is_abandoned = match replacement_package_name {
                                        Some(rp) => PhpMixed::String(rp),
                                        None => PhpMixed::Bool(true),
                                    };
                                }
                            }
                        }

                        package_view_data.insert("abandoned".to_string(), package_is_abandoned);
                    } else if let PackageOrName::Name(name) = package_or_name {
                        package_view_data
                            .insert("name".to_string(), PhpMixed::String(name.clone()));
                        name_length = name_length.max(name.len());
                    }
                    view_type.push(package_view_data);
                }
                view_data.insert(r#type.to_string(), view_type);
                view_meta_data.insert(
                    r#type.to_string(),
                    ViewMetaData {
                        name_length,
                        version_length,
                        latest_length,
                        release_date_length,
                        write_latest,
                        write_release_date,
                    },
                );
                if input.borrow().get_option("strict").as_bool() == Some(true)
                    && has_outdated_packages
                {
                    exit_code = 1;
                    break;
                }
            }
        }

        if format == "json" {
            let mut json_map: IndexMap<String, PhpMixed> = IndexMap::new();
            for (k, v) in view_data.iter() {
                json_map.insert(
                    k.clone(),
                    PhpMixed::List(
                        v.iter()
                            .map(|m| {
                                Box::new(PhpMixed::Array(
                                    m.iter()
                                        .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                                        .collect(),
                                ))
                            })
                            .collect(),
                    ),
                );
            }
            let io = self.get_io();
            io.write(&JsonFile::encode(&PhpMixed::Array(
                json_map
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            )));
        } else {
            if input.borrow().get_option("latest").as_bool() == Some(true)
                && view_data.values().any(|v| !v.is_empty())
            {
                let io = self.get_io();
                if !io.is_decorated() {
                    io.write_error("Legend:");
                    io.write_error("! patch or minor release available - update recommended");
                    io.write_error("~ major release available - update possible");
                    if input.borrow().get_option("outdated").as_bool() != Some(true) {
                        io.write_error("= up to date version");
                    }
                } else {
                    io.write_error("<info>Color legend:</info>");
                    io.write_error("- <highlight>patch or minor</highlight> release available - update recommended");
                    io.write_error(
                        "- <comment>major</comment> release available - update possible",
                    );
                    if input.borrow().get_option("outdated").as_bool() != Some(true) {
                        io.write_error("- <info>up to date</info> version");
                    }
                }
            }

            let width = self.get_terminal_width();

            for (r#type, packages) in view_data.iter() {
                let meta = match view_meta_data.get(r#type) {
                    Some(m) => m.clone(),
                    None => continue,
                };
                let name_length = meta.name_length;
                let version_length = meta.version_length;
                let mut latest_length = meta.latest_length;
                let release_date_length = meta.release_date_length;
                let write_latest = meta.write_latest;
                let write_release_date = meta.write_release_date;

                let width_usize = width as usize;
                let version_fits = name_length + version_length + 3 <= width_usize;
                let latest_fits = name_length + version_length + latest_length + 3 <= width_usize;
                let release_date_fits =
                    name_length + version_length + latest_length + release_date_length + 3
                        <= width_usize;
                let description_fits =
                    name_length + version_length + latest_length + release_date_length + 24
                        <= width_usize;

                if latest_fits && !self.get_io().is_decorated() {
                    latest_length += 2;
                }

                if show_all_types {
                    if r#type == "available" {
                        self.get_io()
                            .write(&format!("<comment>{}</comment>:", r#type));
                    } else {
                        self.get_io().write(&format!("<info>{}</info>:", r#type));
                    }
                }

                if write_latest && input.borrow().get_option("direct").as_bool() != Some(true) {
                    let mut direct_deps: Vec<IndexMap<String, PhpMixed>> = Vec::new();
                    let mut transitive_deps: Vec<IndexMap<String, PhpMixed>> = Vec::new();
                    for pkg in packages.iter() {
                        let is_direct = pkg
                            .get("direct-dependency")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if is_direct {
                            direct_deps.push(pkg.clone());
                        } else {
                            transitive_deps.push(pkg.clone());
                        }
                    }

                    self.get_io().write_error("");
                    self.get_io()
                        .write_error("<info>Direct dependencies required in composer.json:</>");
                    if !direct_deps.is_empty() {
                        self.print_packages(
                            &direct_deps,
                            indent,
                            write_version && version_fits,
                            latest_fits,
                            write_description && description_fits,
                            width_usize,
                            version_length,
                            name_length,
                            latest_length,
                            write_release_date && release_date_fits,
                            release_date_length,
                        );
                    } else {
                        self.get_io().write_error("Everything up to date");
                    }
                    self.get_io().write_error("");
                    self.get_io().write_error(
                        "<info>Transitive dependencies not required in composer.json:</>",
                    );
                    if !transitive_deps.is_empty() {
                        self.print_packages(
                            &transitive_deps,
                            indent,
                            write_version && version_fits,
                            latest_fits,
                            write_description && description_fits,
                            width_usize,
                            version_length,
                            name_length,
                            latest_length,
                            write_release_date && release_date_fits,
                            release_date_length,
                        );
                    } else {
                        self.get_io().write_error("Everything up to date");
                    }
                } else {
                    if write_latest && packages.is_empty() {
                        self.get_io()
                            .write_error("All your direct dependencies are up to date");
                    } else {
                        self.print_packages(
                            packages,
                            indent,
                            write_version && version_fits,
                            write_latest && latest_fits,
                            write_description && description_fits,
                            width_usize,
                            version_length,
                            name_length,
                            latest_length,
                            write_release_date && release_date_fits,
                            release_date_length,
                        );
                    }
                }

                if show_all_types {
                    self.get_io().write("");
                }
            }
        }

        Ok(exit_code)
    }

    fn print_packages(
        &mut self,
        packages: &[IndexMap<String, PhpMixed>],
        indent: &str,
        write_version: bool,
        write_latest: bool,
        write_description: bool,
        width: usize,
        version_length: usize,
        name_length: usize,
        latest_length: usize,
        write_release_date: bool,
        release_date_length: usize,
    ) {
        let io = self.get_io();
        let pad_name = write_version || write_latest || write_release_date || write_description;
        let pad_version = write_latest || write_release_date || write_description;
        let pad_latest = write_description || write_release_date;
        let pad_release_date = write_description;
        for package in packages.iter() {
            let link = package
                .get("source")
                .and_then(|v| v.as_string())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .or_else(|| {
                    package
                        .get("homepage")
                        .and_then(|v| v.as_string())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            let name = package
                .get("name")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            if !link.is_empty() {
                let pad = if pad_name && name_length > name.len() {
                    name_length - name.len()
                } else {
                    0
                };
                io.write_no_newline(&format!(
                    "{}<href={}>{}</>{}",
                    indent,
                    OutputFormatter::escape(&link),
                    name,
                    " ".repeat(pad)
                ));
            } else {
                let width_pad = if pad_name { name_length } else { 0 };
                io.write_no_newline(&format!("{}{:<width$}", indent, name, width = width_pad));
            }
            if let Some(version) = package.get("version").and_then(|v| v.as_string()) {
                if write_version {
                    let width_pad = if pad_version { version_length } else { 0 };
                    io.write_no_newline(&format!(" {:<width$}", version, width = width_pad));
                }
            }
            if let (Some(latest_version), Some(update_status)) = (
                package.get("latest").and_then(|v| v.as_string()),
                package.get("latest-status").and_then(|v| v.as_string()),
            ) {
                if write_latest {
                    let mut latest_version = latest_version.to_string();
                    let style = Self::update_status_to_version_style(update_status);
                    if !io.is_decorated() {
                        let marker = update_status
                            .replace("up-to-date", "=")
                            .replace("semver-safe-update", "!")
                            .replace("update-possible", "~");
                        latest_version = format!("{} {}", marker, latest_version);
                    }
                    let width_pad = if pad_latest { latest_length } else { 0 };
                    io.write_no_newline(&format!(
                        " <{}>{:<width$}</{}>",
                        style,
                        latest_version,
                        style,
                        width = width_pad
                    ));
                    if write_release_date {
                        if let Some(age) = package.get("release-age").and_then(|v| v.as_string()) {
                            let width_pad = if pad_release_date {
                                release_date_length
                            } else {
                                0
                            };
                            io.write_no_newline(&format!(" {:<width$}", age, width = width_pad));
                        }
                    }
                }
            }
            if let Some(description) = package.get("description").and_then(|v| v.as_string()) {
                if write_description {
                    let mut description = description
                        .split(|c| c == '\r' || c == '\n')
                        .next()
                        .unwrap_or("")
                        .to_string();

                    // Compute remaining width available for the description.
                    let mut remaining = (width as i64)
                        - (name_length as i64)
                        - (version_length as i64)
                        - (release_date_length as i64)
                        - 4;
                    if write_latest {
                        remaining -= latest_length as i64;
                    }

                    // If nothing fits, clear the description.
                    if remaining <= 0 {
                        description = String::new();
                    } else if extension_loaded("mbstring") {
                        // Use mb_strwidth/mb_strimwidth to measure and trim by display width
                        // (CJK characters count as width 2). mb_strimwidth counts the trim
                        // marker ('...') in the width parameter, so pass $remaining directly.
                        if description.chars().count() > remaining as usize {
                            description = format!(
                                "{}...",
                                description
                                    .chars()
                                    .take((remaining as usize).saturating_sub(3))
                                    .collect::<String>()
                            );
                        }
                    } else {
                        // Fallback when mbstring is not available: do a conservative byte-based cut.
                        // Ensure cut length is non-negative and leave room for the ellipsis.
                        let cut = (remaining as i64 - 3).max(0) as usize;
                        if description.len() > cut {
                            description = format!("{}...", &description[..cut]);
                        }
                    }

                    io.write_no_newline(&format!(" {}", description));
                }
            }
            if package.contains_key("path") {
                let path_str = match package.get("path") {
                    Some(PhpMixed::String(s)) => s.clone(),
                    _ => "null".to_string(),
                };
                io.write_no_newline(&format!(" {}", path_str));
            }
            io.write("");
            if let Some(warning) = package.get("warning").and_then(|v| v.as_string()) {
                io.write(&format!("<warning>{}</warning>", warning));
            }
        }
    }

    pub(crate) fn get_root_requires(&mut self) -> Vec<String> {
        let composer_rc = self.try_composer(None, None);
        let composer_rc = match composer_rc {
            None => return vec![],
            Some(c) => c,
        };
        let composer = crate::command::composer_full(&composer_rc);

        let root_package = composer.get_package();

        let mut combined: IndexMap<String, Link> = IndexMap::new();
        for (k, v) in root_package.get_requires().iter() {
            combined.insert(k.clone(), v.clone());
        }
        for (k, v) in root_package.get_dev_requires().iter() {
            combined.insert(k.clone(), v.clone());
        }
        combined.keys().map(|k| strtolower(k)).collect()
    }

    /// @return array|string|string[]
    pub(crate) fn get_version_style(
        &self,
        latest_package: PackageInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> String {
        Self::update_status_to_version_style(&Self::get_update_status(latest_package, package))
            .to_string()
    }

    /// finds a package by name and version if provided
    pub(crate) fn get_package(
        &mut self,
        installed_repo: &dyn RepositoryInterface,
        repos: &RepositoryInterfaceHandle,
        name: &str,
        version: PhpMixed,
    ) -> anyhow::Result<(
        Option<crate::package::CompletePackageInterfaceHandle>,
        IndexMap<String, String>,
    )> {
        let name = strtolower(name);
        let constraint: Option<AnyConstraint> = match &version {
            PhpMixed::String(s) => Some(self.version_parser.parse_constraints(s)?),
            PhpMixed::Null => None,
            _ => None, // already a ConstraintInterface
        };

        // TODO(phase-b): DefaultPolicy::new() requires (bool, bool, Option<IndexMap>) — using placeholder values.
        let policy = DefaultPolicy::new(false, false, None);
        let _ = &policy;
        // TODO(phase-b): RepositorySet::with_stability("dev") — using new() with placeholder args.
        let mut repository_set = RepositorySet::new(
            "dev",
            IndexMap::new(),
            Vec::new(),
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );
        repository_set.allow_installed_repositories(true);
        repository_set.add_repository(repos.clone())?;

        let mut matched_package: Option<crate::package::PackageInterfaceHandle> = None;
        let mut versions: IndexMap<String, String> = IndexMap::new();
        let mut pool = if PlatformRepository::is_platform_package(&name) {
            repository_set.create_pool_with_all_packages()?
        } else {
            repository_set.create_pool_for_package(&name, None)?
        };
        let matches = pool.what_provides(&name, constraint.as_ref());
        let mut literals: Vec<i64> = Vec::new();
        for package in matches.iter() {
            // avoid showing the 9999999-dev alias if the default branch has no branch-alias set
            let mut p: crate::package::PackageInterfaceHandle = package.clone().into();
            if let Some(alias) = p.as_alias() {
                if p.get_version() == VersionParser::DEFAULT_BRANCH_ALIAS {
                    p = alias.get_alias_of().into();
                }
            }

            // select an exact match if it is in the installed repo and no specific version was required
            if version.is_null() && installed_repo.has_package(p.clone()) {
                matched_package = Some(p.clone());
            }

            versions.insert(p.get_pretty_version(), p.get_version());
            literals.push(p.get_id());
        }

        // select preferred package according to policy rules
        if matched_package.is_none() && !literals.is_empty() {
            let preferred = policy.select_preferred_packages(&pool, literals.clone(), None);
            matched_package = Some(pool.literal_to_package(preferred[0]).into());
        }

        if let Some(ref mp) = matched_package {
            if mp.as_complete().is_none() {
                return Err(LogicException {
                    message: format!(
                        "ShowCommand::getPackage can only work with CompletePackageInterface, but got {}",
                        shirabe_php_shim::get_class(&PhpMixed::Null)
                    ),
                    code: 0,
                }
                .into());
            }
        }

        let matched_package = matched_package.and_then(|mp| mp.as_complete());
        Ok((matched_package, versions))
    }

    /// Prints package info.
    pub(crate) fn print_package_info(
        &mut self,
        package: CompletePackageInterfaceHandle,
        versions: &IndexMap<String, String>,
        installed_repo: &mut dyn RepositoryInterface,
        latest_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        self.print_meta(package.clone(), versions, installed_repo, latest_package)?;
        self.print_links(package.clone(), Link::TYPE_REQUIRE, None);
        self.print_links(
            package.clone(),
            Link::TYPE_DEV_REQUIRE,
            Some("requires (dev)"),
        );

        if !package.get_suggests().is_empty() {
            self.get_io().write("\n<info>suggests</info>");
            for (suggested, reason) in package.get_suggests().iter() {
                self.get_io()
                    .write(&format!("{} <comment>{}</comment>", suggested, reason));
            }
        }

        self.print_links(package.clone(), Link::TYPE_PROVIDE, None);
        self.print_links(package.clone(), Link::TYPE_CONFLICT, None);
        self.print_links(package, Link::TYPE_REPLACE, None);
        Ok(())
    }

    /// Prints package metadata.
    pub(crate) fn print_meta(
        &mut self,
        package: CompletePackageInterfaceHandle,
        versions: &IndexMap<String, String>,
        installed_repo: &mut dyn RepositoryInterface,
        latest_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        let is_installed_package = !PlatformRepository::is_platform_package(&package.get_name())
            && installed_repo.has_package(package.clone().into());

        self.get_io().write(&format!(
            "<info>name</info>     : {}",
            package.get_pretty_name()
        ));
        self.get_io().write(&format!(
            "<info>descrip.</info> : {}",
            package.get_description().unwrap_or_default()
        ));
        let keywords = package.get_keywords();
        self.get_io()
            .write(&format!("<info>keywords</info> : {}", keywords.join(", ")));
        self.print_versions(package.clone(), versions, installed_repo)?;
        if is_installed_package {
            if let Some(rd) = package.get_release_date() {
                let rel = self.get_relative_time(&rd);
                self.get_io().write(&format!(
                    "<info>released</info> : {}, {}",
                    rd.format("%Y-%m-%d"),
                    rel
                ));
            }
        }
        let latest: PackageInterfaceHandle = if let Some(latest) = latest_package {
            let style = self.get_version_style(latest.clone(), package.clone().into());
            let released_time = match latest.get_release_date() {
                None => String::new(),
                Some(rd) => {
                    let rel = self.get_relative_time(&rd);
                    format!(" released {}, {}", rd.format("%Y-%m-%d"), rel)
                }
            };
            self.get_io().write(&format!(
                "<info>latest</info>   : <{}>{}</{}>{}",
                style,
                latest.get_pretty_version(),
                style,
                released_time
            ));
            latest
        } else {
            package.clone().into()
        };
        self.get_io()
            .write(&format!("<info>type</info>     : {}", package.get_type()));
        self.print_licenses(package.clone());
        self.get_io().write(&format!(
            "<info>homepage</info> : {}",
            package.get_homepage().unwrap_or_default()
        ));
        self.get_io().write(&format!(
            "<info>source</info>   : [{}] <comment>{}</comment> {}",
            package.get_source_type().unwrap_or_default(),
            package.get_source_url().unwrap_or_default(),
            package.get_source_reference().unwrap_or_default()
        ));
        self.get_io().write(&format!(
            "<info>dist</info>     : [{}] <comment>{}</comment> {}",
            package.get_dist_type().unwrap_or_default(),
            package.get_dist_url().unwrap_or_default(),
            package.get_dist_reference().unwrap_or_default()
        ));
        if is_installed_package {
            let path: Option<String> = self.require_composer(None, None).ok().and_then(|c| {
                let installation_manager = c.borrow_partial().get_installation_manager();
                let p = installation_manager
                    .borrow_mut()
                    .get_install_path(package.clone().into());
                p
            });
            if let Some(p) = path {
                self.get_io().write(&format!(
                    "<info>path</info>     : {}",
                    realpath(&p).unwrap_or_default()
                ));
            } else {
                self.get_io().write("<info>path</info>     : null");
            }
        }
        self.get_io().write(&format!(
            "<info>names</info>    : {}",
            package.get_names(true).join(", ")
        ));

        if let Some(c) = latest.as_complete() {
            if c.is_abandoned() {
                let replacement = match c.get_replacement_package() {
                    Some(rp) => format!(" The author suggests using the {} package instead.", rp),
                    None => String::new(),
                };

                self.get_io().write_error(&format!(
                    "<warning>Attention: This package is abandoned and no longer maintained.{}</warning>",
                    replacement
                ));
            }
        }

        let support = package.get_support();
        if !support.is_empty() {
            self.get_io().write("\n<info>support</info>");
            for (r#type, value) in support.iter() {
                self.get_io()
                    .write(&format!("<comment>{}</comment> : {}", r#type, value));
            }
        }

        let autoload_config = package.get_autoload();
        if !autoload_config.is_empty() {
            self.get_io().write("\n<info>autoload</info>");
            for (r#type, autoloads) in autoload_config.iter() {
                self.get_io()
                    .write(&format!("<comment>{}</comment>", r#type));

                if r#type == "psr-0" || r#type == "psr-4" {
                    if let PhpMixed::Array(map) = autoloads {
                        for (name, path) in map.iter() {
                            let path_str = match &**path {
                                PhpMixed::List(l) => l
                                    .iter()
                                    .filter_map(|p| p.as_string().map(|s| s.to_string()))
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                PhpMixed::String(s) if !s.is_empty() => s.clone(),
                                _ => ".".to_string(),
                            };
                            let name_disp = if name.is_empty() { "*" } else { name };
                            self.get_io()
                                .write(&format!("{} => {}", name_disp, path_str));
                        }
                    }
                } else if r#type == "classmap" {
                    if let PhpMixed::List(l) = autoloads {
                        let joined: Vec<String> = l
                            .iter()
                            .filter_map(|v| v.as_string().map(|s| s.to_string()))
                            .collect();
                        self.get_io().write(&joined.join(", "));
                    }
                }
            }
            let include_paths = package.get_include_paths();
            if !include_paths.is_empty() {
                self.get_io().write("<comment>include-path</comment>");
                self.get_io().write(&include_paths.join(", "));
            }
        }

        Ok(())
    }

    /// Prints all available versions of this package and highlights the installed one if any.
    pub(crate) fn print_versions(
        &mut self,
        package: CompletePackageInterfaceHandle,
        versions: &IndexMap<String, String>,
        installed_repo: &mut dyn RepositoryInterface,
    ) -> anyhow::Result<()> {
        let mut versions_keys: Vec<String> = versions.keys().cloned().collect();
        versions_keys = Semver::rsort(versions_keys);

        // highlight installed version
        let installed_packages = installed_repo.find_packages(&package.get_name(), None)?;
        if !installed_packages.is_empty() {
            for installed_package in installed_packages.iter() {
                let installed_version = installed_package.get_pretty_version();
                let key_map: IndexMap<String, String> = versions_keys
                    .iter()
                    .map(|v| (v.clone(), v.clone()))
                    .collect();
                if let Some(found) = array_search(&installed_version, &key_map) {
                    if let Some(idx) = versions_keys.iter().position(|v| v == &found) {
                        versions_keys[idx] = format!("<info>* {}</info>", installed_version);
                    }
                }
            }
        }

        let versions_str = versions_keys.join(", ");

        self.get_io()
            .write(&format!("<info>versions</info> : {}", versions_str));

        Ok(())
    }

    /// print link objects
    pub(crate) fn print_links(
        &mut self,
        package: CompletePackageInterfaceHandle,
        link_type: &str,
        title: Option<&str>,
    ) {
        let title = title.unwrap_or(link_type);
        let io = self.get_io();
        let links = package.get_links_for_type(link_type);
        if !links.is_empty() {
            io.write(&format!("\n<info>{}</info>", title));

            for link in links.iter() {
                io.write(&format!(
                    "{} <comment>{}</comment>",
                    link.1.get_target(),
                    link.1.get_pretty_constraint(),
                ));
            }
        }
    }

    /// Prints the licenses of a package with metadata
    pub(crate) fn print_licenses(&mut self, package: CompletePackageInterfaceHandle) {
        let spdx_licenses = SpdxLicenses::new();

        let licenses = package.get_license();
        let io = self.get_io();

        for license_id in licenses.iter() {
            let license = spdx_licenses.get_license_by_identifier(license_id);

            let out = match license {
                None => license_id.clone(),
                Some(license) => {
                    // TODO(phase-b): SpdxLicenses returns PhpMixed; field access (osi/fullname/url)
                    // is placeholder until PHP array offsets are wired.
                    let _ = &license;
                    let fullname = String::new();
                    let url = String::new();
                    let is_osi = false;
                    if is_osi {
                        format!("{} ({}) (OSI approved) {}", fullname, license_id, url)
                    } else {
                        format!("{} ({}) {}", fullname, license_id, url)
                    }
                }
            };

            io.write(&format!("<info>license</info>  : {}", out));
        }
    }

    /// Prints package info in JSON format.
    pub(crate) fn print_package_info_as_json(
        &mut self,
        package: CompletePackageInterfaceHandle,
        versions: &IndexMap<String, String>,
        installed_repo: &dyn RepositoryInterface,
        latest_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<()> {
        let mut json: IndexMap<String, PhpMixed> = IndexMap::new();
        json.insert(
            "name".to_string(),
            PhpMixed::String(package.get_pretty_name().to_string()),
        );
        json.insert(
            "description".to_string(),
            PhpMixed::String(package.get_description().unwrap_or_default()),
        );
        let keywords: Vec<PhpMixed> = package
            .get_keywords()
            .into_iter()
            .map(PhpMixed::String)
            .collect();
        json.insert(
            "keywords".to_string(),
            PhpMixed::List(keywords.into_iter().map(Box::new).collect()),
        );
        json.insert(
            "type".to_string(),
            PhpMixed::String(package.get_type().to_string()),
        );
        json.insert(
            "homepage".to_string(),
            match package.get_homepage() {
                Some(h) => PhpMixed::String(h),
                None => PhpMixed::Null,
            },
        );
        json.insert(
            "names".to_string(),
            PhpMixed::List(
                package
                    .get_names(true)
                    .into_iter()
                    .map(|n| Box::new(PhpMixed::String(n)))
                    .collect(),
            ),
        );

        json = Self::append_versions(json, versions);
        json = Self::append_licenses(json, package.clone());

        let latest: PackageInterfaceHandle = if let Some(latest) = latest_package {
            json.insert(
                "latest".to_string(),
                PhpMixed::String(latest.get_pretty_version().to_string()),
            );
            latest
        } else {
            package.clone().into()
        };

        if package.get_source_type().is_some() {
            let mut src: IndexMap<String, PhpMixed> = IndexMap::new();
            src.insert(
                "type".to_string(),
                PhpMixed::String(package.get_source_type().unwrap_or_default()),
            );
            src.insert(
                "url".to_string(),
                PhpMixed::String(package.get_source_url().unwrap_or_default()),
            );
            src.insert(
                "reference".to_string(),
                PhpMixed::String(package.get_source_reference().unwrap_or_default()),
            );
            json.insert(
                "source".to_string(),
                PhpMixed::Array(src.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
            );
        }

        if package.get_dist_type().is_some() {
            let mut dst: IndexMap<String, PhpMixed> = IndexMap::new();
            dst.insert(
                "type".to_string(),
                PhpMixed::String(package.get_dist_type().unwrap_or_default()),
            );
            dst.insert(
                "url".to_string(),
                PhpMixed::String(package.get_dist_url().unwrap_or_default()),
            );
            dst.insert(
                "reference".to_string(),
                PhpMixed::String(package.get_dist_reference().unwrap_or_default()),
            );
            json.insert(
                "dist".to_string(),
                PhpMixed::Array(dst.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
            );
        }

        if !PlatformRepository::is_platform_package(&package.get_name())
            && installed_repo.has_package(package.clone().into())
        {
            let composer = self.require_composer(None, None)?;
            let installation_manager = composer.borrow_partial().get_installation_manager();
            let path: Option<String> = installation_manager
                .borrow_mut()
                .get_install_path(package.clone().into());
            match path {
                Some(p) => {
                    if let Some(r) = realpath(&p) {
                        json.insert("path".to_string(), PhpMixed::String(r));
                    }
                }
                None => {
                    json.insert("path".to_string(), PhpMixed::Null);
                }
            }

            if let Some(rd) = package.get_release_date() {
                json.insert("released".to_string(), PhpMixed::String(rd.to_rfc3339()));
            }
        }

        if let Some(c) = latest.as_complete() {
            if c.is_abandoned() {
                json.insert(
                    "replacement".to_string(),
                    match c.get_replacement_package() {
                        Some(rp) => PhpMixed::String(rp.to_string()),
                        None => PhpMixed::Null,
                    },
                );
            }
        }

        if !package.get_suggests().is_empty() {
            let mut s: IndexMap<String, PhpMixed> = IndexMap::new();
            for (k, v) in package.get_suggests().iter() {
                s.insert(k.clone(), PhpMixed::String(v.clone()));
            }
            json.insert(
                "suggests".to_string(),
                PhpMixed::Array(s.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
            );
        }

        if !package.get_support().is_empty() {
            let mut s: IndexMap<String, PhpMixed> = IndexMap::new();
            for (k, v) in package.get_support().iter() {
                s.insert(k.clone(), PhpMixed::String(v.clone()));
            }
            json.insert(
                "support".to_string(),
                PhpMixed::Array(s.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
            );
        }

        json = Self::append_autoload(json, package.clone());

        if !package.get_include_paths().is_empty() {
            json.insert(
                "include_path".to_string(),
                PhpMixed::List(
                    package
                        .get_include_paths()
                        .into_iter()
                        .map(|p| Box::new(PhpMixed::String(p)))
                        .collect(),
                ),
            );
        }

        json = Self::append_links(json, package);

        self.get_io().write(&JsonFile::encode(&PhpMixed::Array(
            json.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
        )));
        Ok(())
    }

    fn append_versions(
        mut json: IndexMap<String, PhpMixed>,
        versions: &IndexMap<String, String>,
    ) -> IndexMap<String, PhpMixed> {
        let mut versions_pairs: Vec<(String, String)> = versions
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // uasort($versions, 'version_compare');
        versions_pairs.sort_by(|a, b| {
            if version_compare(&a.1, &b.1, "<") {
                std::cmp::Ordering::Less
            } else if version_compare(&a.1, &b.1, ">") {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        });
        versions_pairs.reverse();
        let keys: Vec<PhpMixed> = versions_pairs
            .into_iter()
            .map(|(k, _)| PhpMixed::String(k))
            .collect();
        json.insert(
            "versions".to_string(),
            PhpMixed::List(keys.into_iter().map(Box::new).collect()),
        );

        json
    }

    fn append_licenses(
        mut json: IndexMap<String, PhpMixed>,
        package: CompletePackageInterfaceHandle,
    ) -> IndexMap<String, PhpMixed> {
        let licenses = package.get_license();
        if !licenses.is_empty() {
            let spdx_licenses = SpdxLicenses::new();

            let mapped: Vec<PhpMixed> = licenses
                .into_iter()
                .map(|license_id| {
                    let license = spdx_licenses.get_license_by_identifier(&license_id);
                    match license {
                        None => PhpMixed::String(license_id),
                        Some(l) => {
                            // TODO(phase-b): SpdxLicenses returns PhpMixed; field access placeholder.
                            let _ = &l;
                            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                            m.insert("name".to_string(), PhpMixed::String(String::new()));
                            m.insert("osi".to_string(), PhpMixed::String(license_id));
                            m.insert("url".to_string(), PhpMixed::String(String::new()));
                            PhpMixed::Array(m.into_iter().map(|(k, v)| (k, Box::new(v))).collect())
                        }
                    }
                })
                .collect();
            json.insert(
                "licenses".to_string(),
                PhpMixed::List(mapped.into_iter().map(Box::new).collect()),
            );
        }

        json
    }

    fn append_autoload(
        mut json: IndexMap<String, PhpMixed>,
        package: CompletePackageInterfaceHandle,
    ) -> IndexMap<String, PhpMixed> {
        let autoload_config = package.get_autoload();
        if !autoload_config.is_empty() {
            let mut autoload: IndexMap<String, PhpMixed> = IndexMap::new();

            for (r#type, autoloads) in autoload_config.iter() {
                if r#type == "psr-0" || r#type == "psr-4" {
                    let mut psr: IndexMap<String, PhpMixed> = IndexMap::new();

                    if let PhpMixed::Array(map) = autoloads {
                        for (name, path) in map.iter() {
                            let mut path_val = (**path).clone();
                            let is_empty_path = match &path_val {
                                PhpMixed::String(s) => s.is_empty(),
                                PhpMixed::Null => true,
                                _ => false,
                            };
                            if is_empty_path {
                                path_val = PhpMixed::String(".".to_string());
                            }

                            let key = if name.is_empty() {
                                "*".to_string()
                            } else {
                                name.clone()
                            };
                            psr.insert(key, path_val);
                        }
                    }

                    autoload.insert(
                        r#type.clone(),
                        PhpMixed::Array(psr.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
                    );
                } else if r#type == "classmap" {
                    autoload.insert("classmap".to_string(), autoloads.clone());
                }
            }

            json.insert(
                "autoload".to_string(),
                PhpMixed::Array(
                    autoload
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                ),
            );
        }

        json
    }

    fn append_links(
        mut json: IndexMap<String, PhpMixed>,
        package: CompletePackageInterfaceHandle,
    ) -> IndexMap<String, PhpMixed> {
        for link_type in Link::types().iter() {
            json = Self::append_link(json, package.clone(), link_type);
        }

        json
    }

    fn append_link(
        mut json: IndexMap<String, PhpMixed>,
        package: CompletePackageInterfaceHandle,
        link_type: &str,
    ) -> IndexMap<String, PhpMixed> {
        let links = package.get_links_for_type(link_type);

        if !links.is_empty() {
            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
            for link in links.iter() {
                m.insert(
                    link.1.get_target().to_string(),
                    PhpMixed::String(link.1.get_pretty_constraint().to_string()),
                );
            }
            json.insert(
                link_type.to_string(),
                PhpMixed::Array(m.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
            );
        }

        json
    }

    /// Init styles for tree
    pub(crate) fn init_styles(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
        self.colors = vec![
            "green".to_string(),
            "yellow".to_string(),
            "cyan".to_string(),
            "magenta".to_string(),
            "blue".to_string(),
        ];

        for color in self.colors.iter() {
            let _style = OutputFormatterStyle::new(Some(color.as_str()), None, None);
            // TODO(phase-b): OutputInterface::get_formatter returns &OutputFormatter, but
            // set_style requires &mut. Resolution requires interior-mutability refactor of
            // OutputFormatter wiring across symfony shim.
            let _ = (output.borrow().get_formatter(), color);
        }
    }

    /// Display the tree
    pub(crate) fn display_package_tree(&mut self, array_tree: Vec<IndexMap<String, PhpMixed>>) {
        for package in array_tree.iter() {
            let name = package
                .get("name")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            self.get_io()
                .write_no_newline(&format!("<info>{}</info>", name));
            let version = package
                .get("version")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            self.get_io().write_no_newline(&format!(" {}", version));
            if let Some(description) = package.get("description").and_then(|v| v.as_string()) {
                let trimmed = description
                    .split(|c| c == '\r' || c == '\n')
                    .next()
                    .unwrap_or("");
                self.get_io().write(&format!(" {}", trimmed));
            } else {
                // output newline
                self.get_io().write("");
            }

            if let Some(requires) = package.get("requires").and_then(|v| v.as_list()).cloned() {
                let mut tree_bar = "├".to_string();
                let mut j = 0_usize;
                let total = requires.len();
                for require_mixed in requires.iter() {
                    let require = match require_mixed.as_array() {
                        Some(a) => a,
                        None => continue,
                    };
                    let require_name = require
                        .get("name")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    j += 1;
                    if j == total {
                        tree_bar = "└".to_string();
                    }
                    let level: usize = 1;
                    let color = self.colors.get(level).cloned().unwrap_or_default();
                    let info = format!(
                        "{}──<{}>{}</{}> {}",
                        tree_bar,
                        color,
                        require_name,
                        color,
                        require
                            .get("version")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                    );
                    self.write_tree_line(&info);

                    tree_bar = tree_bar.replace('└', " ");
                    let packages_in_tree: Vec<PhpMixed> = vec![
                        PhpMixed::String(name.clone()),
                        PhpMixed::String(require_name.clone()),
                    ];

                    self.display_tree(
                        &PhpMixed::Array(
                            require
                                .iter()
                                .map(|(k, v)| (k.clone(), Box::new((**v).clone())))
                                .collect(),
                        ),
                        &packages_in_tree,
                        &tree_bar,
                        level + 1,
                    );
                }
            }
        }
    }

    /// Generate the package tree
    pub(crate) fn generate_package_tree(
        &mut self,
        package: PackageInterfaceHandle,
        installed_repo: &dyn RepositoryInterface,
        remote_repos: &RepositoryInterfaceHandle,
    ) -> IndexMap<String, PhpMixed> {
        let requires = {
            let mut r: IndexMap<String, Link> = package.get_requires().clone();
            r.sort_keys();
            r
        };
        let mut children: Vec<PhpMixed> = Vec::new();
        for (require_name, require) in requires.iter() {
            let packages_in_tree: Vec<PhpMixed> = vec![
                PhpMixed::String(package.get_name().to_string()),
                PhpMixed::String(require_name.clone()),
            ];

            let mut tree_child_desc: IndexMap<String, PhpMixed> = IndexMap::new();
            tree_child_desc.insert("name".to_string(), PhpMixed::String(require_name.clone()));
            tree_child_desc.insert(
                "version".to_string(),
                PhpMixed::String(require.get_pretty_constraint().to_string()),
            );

            let deep_children = self
                .add_tree(
                    require_name,
                    require,
                    installed_repo,
                    remote_repos,
                    &packages_in_tree,
                )
                .unwrap_or_default();

            if !deep_children.is_empty() {
                tree_child_desc.insert(
                    "requires".to_string(),
                    PhpMixed::List(
                        deep_children
                            .into_iter()
                            .map(|m| {
                                Box::new(PhpMixed::Array(
                                    m.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                                ))
                            })
                            .collect(),
                    ),
                );
            }

            children.push(PhpMixed::Array(
                tree_child_desc
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ));
        }
        let mut tree: IndexMap<String, PhpMixed> = IndexMap::new();
        tree.insert(
            "name".to_string(),
            PhpMixed::String(package.get_pretty_name().to_string()),
        );
        tree.insert(
            "version".to_string(),
            PhpMixed::String(package.get_pretty_version().to_string()),
        );
        tree.insert(
            "description".to_string(),
            PhpMixed::String(
                package
                    .as_complete()
                    .map(|c| c.get_description().unwrap_or_default())
                    .unwrap_or_default(),
            ),
        );

        if !children.is_empty() {
            tree.insert(
                "requires".to_string(),
                PhpMixed::List(children.into_iter().map(Box::new).collect()),
            );
        }

        tree
    }

    /// Display a package tree
    pub(crate) fn display_tree(
        &mut self,
        package: &PhpMixed,
        packages_in_tree: &[PhpMixed],
        previous_tree_bar: &str,
        level: usize,
    ) {
        let previous_tree_bar = previous_tree_bar.replace('├', "│");
        let arr = match package.as_array() {
            Some(a) => a,
            None => return,
        };
        let requires = match arr.get("requires").and_then(|v| v.as_list()).cloned() {
            Some(l) => l,
            None => return,
        };
        let mut tree_bar = format!("{}  ├", previous_tree_bar);
        let mut i = 0_usize;
        let total = requires.len();
        for require_mixed in requires.iter() {
            let mut current_tree = packages_in_tree.to_vec();
            i += 1;
            if i == total {
                tree_bar = format!("{}  └", previous_tree_bar);
            }
            let color_ident = level % self.colors.len();
            let color = self.colors.get(color_ident).cloned().unwrap_or_default();

            let require = match require_mixed.as_array() {
                Some(a) => a,
                None => continue,
            };
            let require_name = require
                .get("name")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            let require_version = require
                .get("version")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();

            let circular_warn = if in_array(
                PhpMixed::String(require_name.clone()),
                &PhpMixed::List(current_tree.iter().map(|v| Box::new(v.clone())).collect()),
                true,
            ) {
                "(circular dependency aborted here)"
            } else {
                ""
            };
            let info = format!(
                "{}──<{}>{}</{}> {} {}",
                tree_bar, color, require_name, color, require_version, circular_warn
            )
            .trim_end()
            .to_string();
            self.write_tree_line(&info);

            tree_bar = tree_bar.replace('└', " ");

            current_tree.push(PhpMixed::String(require_name.clone()));
            self.display_tree(require_mixed, &current_tree, &tree_bar, level + 1);
        }
    }

    /// Display a package tree
    pub(crate) fn add_tree(
        &mut self,
        name: &str,
        link: &Link,
        installed_repo: &dyn RepositoryInterface,
        remote_repos: &RepositoryInterfaceHandle,
        packages_in_tree: &[PhpMixed],
    ) -> anyhow::Result<Vec<IndexMap<String, PhpMixed>>> {
        let mut children: Vec<IndexMap<String, PhpMixed>> = Vec::new();
        let version_arg: PhpMixed = if link.get_pretty_constraint() == "self.version" {
            // pass the ConstraintInterface object — signal via Null in this scalar shape
            PhpMixed::Null
        } else {
            PhpMixed::String(link.get_pretty_constraint().to_string())
        };
        let (package, _) = self.get_package(installed_repo, remote_repos, name, version_arg)?;
        if let Some(package) = package {
            let mut requires = package.get_requires().clone();
            requires.sort_keys();
            for (require_name, require) in requires.iter() {
                let mut current_tree = packages_in_tree.to_vec();

                let mut tree_child_desc: IndexMap<String, PhpMixed> = IndexMap::new();
                tree_child_desc.insert("name".to_string(), PhpMixed::String(require_name.clone()));
                tree_child_desc.insert(
                    "version".to_string(),
                    PhpMixed::String(require.get_pretty_constraint().to_string()),
                );

                if !in_array(
                    PhpMixed::String(require_name.clone()),
                    &PhpMixed::List(current_tree.iter().map(|v| Box::new(v.clone())).collect()),
                    true,
                ) {
                    current_tree.push(PhpMixed::String(require_name.clone()));
                    let deep_children = self.add_tree(
                        require_name,
                        require,
                        installed_repo,
                        remote_repos,
                        &current_tree,
                    )?;
                    if !deep_children.is_empty() {
                        tree_child_desc.insert(
                            "requires".to_string(),
                            PhpMixed::List(
                                deep_children
                                    .into_iter()
                                    .map(|m| {
                                        Box::new(PhpMixed::Array(
                                            m.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                                        ))
                                    })
                                    .collect(),
                            ),
                        );
                    }
                }

                children.push(tree_child_desc);
            }
        }

        Ok(children)
    }

    fn update_status_to_version_style(update_status: &str) -> &'static str {
        // 'up-to-date' is printed green
        // 'semver-safe-update' is printed red
        // 'update-possible' is printed yellow
        match update_status {
            "up-to-date" => "info",
            "semver-safe-update" => "highlight",
            "update-possible" => "comment",
            _ => "comment",
        }
    }

    fn get_update_status(
        latest_package: PackageInterfaceHandle,
        package: PackageInterfaceHandle,
    ) -> String {
        if latest_package.get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
            == package.get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
        {
            return "up-to-date".to_string();
        }

        let mut constraint = package.get_version().to_string();
        if !constraint.starts_with("dev-") {
            constraint = format!("^{}", constraint);
        }
        if !latest_package.get_version().is_empty()
            && Semver::satisfies(&latest_package.get_version(), &constraint)
        {
            // it needs an immediate semver-compliant upgrade
            return "semver-safe-update".to_string();
        }

        // it needs an upgrade but has potential BC breaks so is not urgent
        "update-possible".to_string()
    }

    fn write_tree_line(&mut self, line: &str) {
        let io = self.get_io();
        let mut line = line.to_string();
        if !io.is_decorated() {
            line = line
                .replace('└', "`-")
                .replace('├', "|-")
                .replace("──", "-")
                .replace('│', "|");
        }

        io.write(&line);
    }

    /// Given a package, this finds the latest package matching it
    fn find_latest_package(
        &mut self,
        package: PackageInterfaceHandle,
        composer: &PartialComposerHandle,
        platform_repo: &PlatformRepositoryHandle,
        major_only: bool,
        minor_only: bool,
        patch_only: bool,
        platform_req_filter: &dyn PlatformRequirementFilterInterface,
    ) -> anyhow::Result<Option<crate::package::PackageInterfaceHandle>> {
        // find the latest version allowed in this repo set
        let name = package.get_name();
        let repo_set = self.get_repository_set(composer)?;
        let composer_ref = crate::command::composer_full(composer);
        let mut version_selector =
            VersionSelector::new(repo_set, Some(&mut *platform_repo.borrow_mut()))?;
        let mut stability = composer_ref
            .get_package()
            .get_minimum_stability()
            .to_string();
        let flags = composer_ref.get_package().get_stability_flags();
        if let Some(flag_value) = flags.get(&name) {
            let key_map: IndexMap<String, String> = base_package::STABILITIES
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let needle = flag_value.to_string();
            if let Some(found_key) = array_search(&needle, &key_map) {
                stability = found_key;
            }
        }

        let mut best_stability = stability.clone();
        if composer_ref.get_package().get_prefer_stable() {
            best_stability = package.get_stability().to_string();
        }

        let mut target_version: Option<String> = None;
        if package.get_version().starts_with("dev-") {
            target_version = Some(package.get_version());

            // dev-x branches are considered to be on the latest major version always, do not look up for a new commit as that is deemed a minor upgrade (albeit risky)
            if major_only {
                return Ok(None);
            }
        }

        if target_version.is_none() {
            let mut groups: IndexMap<CaptureKey, String> = IndexMap::new();
            if major_only
                && Preg::is_match3(
                    r"{^(?P<zero_major>(?:0\.)+)?(?P<first_meaningful>\d+)\.}",
                    &package.get_version(),
                    Some(&mut groups),
                )?
            {
                let zero_major = groups
                    .get(&CaptureKey::ByName("zero_major".to_string()))
                    .cloned()
                    .unwrap_or_default();
                let first_meaningful = groups
                    .get(&CaptureKey::ByName("first_meaningful".to_string()))
                    .cloned()
                    .unwrap_or_default()
                    .parse::<i64>()
                    .unwrap_or(0);
                target_version = Some(format!(
                    ">={}{},<9999999-dev",
                    zero_major,
                    first_meaningful + 1
                ));
            }

            if minor_only {
                target_version = Some(format!("^{}", package.get_version()));
            }

            if patch_only {
                let trimmed_version = Preg::replace(r"{(\.0)+$}D", "", &package.get_version())?;
                let parts_needed = if trimmed_version.starts_with('0') {
                    4
                } else {
                    3
                };
                let mut trimmed_version = trimmed_version;
                while trimmed_version.chars().filter(|&c| c == '.').count() + 1 < parts_needed {
                    trimmed_version.push_str(".0");
                }
                target_version = Some(format!("~{}", trimmed_version));
            }
        }

        let show_warnings_box: Box<dyn Fn(PackageInterfaceHandle) -> bool>;
        if self.get_io().is_verbose() {
            show_warnings_box = Box::new(|_p: PackageInterfaceHandle| -> bool { true });
        } else {
            let package_version = package.get_version();
            show_warnings_box = Box::new(move |candidate: PackageInterfaceHandle| -> bool {
                if candidate.get_version().starts_with("dev-")
                    || package_version.starts_with("dev-")
                {
                    return false;
                }

                version_compare(&candidate.get_version(), &package_version, "<=")
            });
        }
        // TODO(phase-b): platform_req_filter needs to be Option<Box<dyn ...>>; current code holds &dyn.
        let _ = platform_req_filter;
        let _ = show_warnings_box;
        let mut candidate = version_selector.find_best_candidate(
            &name,
            target_version.as_deref(),
            &best_stability,
            None,
            0,
            Some(self.get_io().clone()),
            PhpMixed::Bool(true),
        )?;
        while let Some(ref c) = candidate {
            if let Some(alias) = c.as_alias() {
                candidate = Some(alias.get_alias_of().into());
            } else {
                break;
            }
        }

        Ok(candidate)
    }

    fn get_repository_set(
        &mut self,
        composer: &PartialComposerHandle,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<RepositorySet>>> {
        let composer = crate::command::composer_full(composer);
        if self.repository_set.is_none() {
            // TODO(phase-b): RepositorySet::with_stability_and_flags — using new() placeholder.
            let mut rs = RepositorySet::new(
                &composer.get_package().get_minimum_stability(),
                composer.get_package().get_stability_flags().clone(),
                Vec::new(),
                IndexMap::new(),
                IndexMap::new(),
                IndexMap::new(),
            );
            rs.add_repository(RepositoryInterfaceHandle::new(CompositeRepository::new(
                composer
                    .get_repository_manager()
                    .borrow()
                    .get_repositories()
                    .iter()
                    .map(|r| r.clone())
                    .collect(),
            )))?;
            self.repository_set = Some(std::rc::Rc::new(std::cell::RefCell::new(rs)));
        }

        Ok(self.repository_set.as_ref().unwrap().clone())
    }

    fn get_relative_time(&self, release_date: &chrono::DateTime<chrono::Utc>) -> String {
        if release_date.format("%Y%m%d").to_string() == date("Ymd", None) {
            return "today".to_string();
        }

        let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
        let diff = now.signed_duration_since(*release_date);
        let days = diff.num_days();
        if days < 7 {
            return "this week".to_string();
        }

        if days < 14 {
            return "last week".to_string();
        }

        let months = days / 30;
        if months < 1 && days < 31 {
            return format!("{} weeks ago", days / 7);
        }

        let years = days / 365;
        if years < 1 {
            return format!("{} month{} ago", months, if months > 1 { "s" } else { "" });
        }

        format!("{} year{} ago", years, if years > 1 { "s" } else { "" })
    }

    fn same_repository<T, U>(a: &T, b: &U) -> bool
    where
        T: Into<RepositoryInterfaceHandle> + Clone,
        U: Into<RepositoryInterfaceHandle> + Clone,
    {
        let a = a.clone().into();
        let b = b.clone().into();
        Self::same_repository_handle(&a, &b)
    }

    fn same_repository_handle(
        a: &RepositoryInterfaceHandle,
        b: &RepositoryInterfaceHandle,
    ) -> bool {
        a.ptr_eq(b)
    }
}

#[derive(Debug)]
pub enum PackageOrName {
    Pkg(crate::package::PackageInterfaceHandle),
    Name(String),
}

#[derive(Debug, Clone)]
struct ViewMetaData {
    name_length: usize,
    version_length: usize,
    latest_length: usize,
    release_date_length: usize,
    write_latest: bool,
    write_release_date: bool,
}

impl HasBaseCommandData for ShowCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
