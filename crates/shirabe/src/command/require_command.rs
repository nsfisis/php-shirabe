//! ref: composer/src/Composer/Command/RequireCommand.php sha1:a02d20cec157fdeaea438599245710b76345bb8e

use crate::advisory::Auditor;
use crate::command::base_command::{base_command_complete, base_command_initialize};
use crate::command::{BaseCommand, BaseCommandData, CompletionTrait, PackageDiscoveryTrait};
use crate::composer::composer_full;
use crate::console::input::{InputArgument, InputOption, SuggestedValues};
use crate::delegate_base_command_trait_impls_to_inner;
use crate::dependency_resolver::UpdateAllowTransitiveDeps;
use crate::event_dispatcher::Callable;
use crate::factory::Factory;
use crate::installer::{Installer, InstallerEvents};
use crate::io::{IOInterface, IOInterfaceImmutable, IOInterfaceWrite, io_interface};
use crate::json::{JsonFile, JsonManipulator};
use crate::package::loader::{ArrayLoader, RootPackageLoader};
use crate::package::version::{VersionParser, VersionSelector};
use crate::package::{CompletePackageInterfaceHandle, Link, PackageInterfaceHandle, base_package};
use crate::plugin::{CommandEvent, PluginEvents};
use crate::repository::{
    CompositeRepository, FindPackageConstraint, PlatformRepository, PlatformRepositoryHandle,
    RepositoryInterfaceHandle, RepositorySet,
};
use crate::signal::SignalSubscription;
use crate::util::{Filesystem, PackageSorter};
use bstr::{B, BStr, BString, ByteSlice, ByteVec};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, array_fill_keys, array_intersect, array_keys, array_merge,
    array_unique, bconcat, empty, file_exists, file_get_contents, file_put_contents, filesize,
    impl_php_class, implode, is_writable, strtolower, unlink,
};
use shirabe_symfony_console::input::InputInterfaceByName;
use shirabe_symfony_console::{
    command::Command,
    input::{InputInterface, InputValue},
    output::OutputInterface,
};
use std::path::PathBuf;

#[derive(Debug)]
pub struct RequireCommand {
    base_command_data: BaseCommandData,

    newly_created: std::cell::Cell<bool>,
    first_require: std::cell::Cell<bool>,
    json: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<JsonFile>>>>,
    file: std::cell::RefCell<PathBuf>,
    composer_backup: std::cell::RefCell<Vec<u8>>,
    /// file name
    lock: std::cell::RefCell<PathBuf>,
    /// contents before modification if the lock file exists
    lock_backup: std::cell::RefCell<Option<Vec<u8>>>,
    dependency_resolution_completed: std::rc::Rc<std::cell::Cell<bool>>,
    repos: std::cell::RefCell<Option<RepositoryInterfaceHandle>>,
    repository_sets:
        std::cell::RefCell<IndexMap<BString, std::rc::Rc<std::cell::RefCell<RepositorySet>>>>,
}

impl_php_class!(RequireCommand, r"Composer\Command\RequireCommand");

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
            file: std::cell::RefCell::new(PathBuf::new()),
            composer_backup: std::cell::RefCell::new(Vec::new()),
            lock: std::cell::RefCell::new(PathBuf::new()),
            lock_backup: std::cell::RefCell::new(None),
            dependency_resolution_completed: std::rc::Rc::new(std::cell::Cell::new(false)),
            repos: std::cell::RefCell::new(None),
            repository_sets: std::cell::RefCell::new(IndexMap::new()),
        };
        command
            .configure()
            .expect("RequireCommand::configure uses static, valid metadata");
        command
    }

    fn get_inconsistent_require_keys(
        &self,
        new_requirements: &IndexMap<BString, BString>,
        require_key: &BStr,
    ) -> Vec<BString> {
        let require_keys = self.get_packages_by_require_key();
        let mut inconsistent_requirements: Vec<BString> = vec![];
        for (package, package_require_key) in &require_keys {
            if !new_requirements.contains_key(package) {
                continue;
            }
            if package_require_key != require_key {
                inconsistent_requirements.push(package.clone());
            }
        }

        inconsistent_requirements
    }

    fn get_packages_by_require_key(&self) -> IndexMap<BString, BString> {
        let json = self.json.borrow().as_ref().unwrap().clone();
        let composer_definition = json.borrow_mut().read().unwrap_or_default();
        let mut require: IndexMap<BString, PhpMixed> = IndexMap::new();
        let mut require_dev: IndexMap<BString, PhpMixed> = IndexMap::new();

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
                PhpMixed::string(BString::from("require")),
            ),
            array_fill_keys(
                PhpMixed::List(
                    array_keys(&require_dev)
                        .into_iter()
                        .map(PhpMixed::String)
                        .collect(),
                ),
                PhpMixed::string(BString::from("require-dev")),
            ),
        )
        .as_array()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_string().map(|s| (k.clone(), BString::from(s))))
                .collect()
        })
        .unwrap_or_default()
    }

    async fn do_update(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        requirements: &IndexMap<BString, BString>,
        require_key: &BStr,
        _remove_key: &BStr,
    ) -> anyhow::Result<i64> {
        // Update packages
        self.reset_composer()?;
        let composer_handle = self.require_composer(None, None)?;
        let composer = composer_full(&composer_handle);

        self.dependency_resolution_completed.set(false);
        // PHP: $composer->getEventDispatcher()->addListener(InstallerEvents::PRE_OPERATIONS_EXEC,
        //   function () use (&$dependencyResolutionCompleted) { $dependencyResolutionCompleted = true; }, 10000);
        let dependency_resolution_completed = self.dependency_resolution_completed.clone();
        composer.get_event_dispatcher().borrow_mut().add_listener(
            InstallerEvents::PRE_OPERATIONS_EXEC,
            Callable::Closure(std::rc::Rc::new(move |_event| {
                dependency_resolution_completed.set(true);
                PhpMixed::Null
            })),
            10000,
        );

        if input
            .borrow()
            .get_option("dry-run")?
            .as_bool()
            .unwrap_or(false)
        {
            let root_package = composer.get_package();
            let mut links: IndexMap<BString, IndexMap<BString, Link>> = IndexMap::new();
            links.insert(
                BString::from("require"),
                (*root_package.get_requires()).clone(),
            );
            links.insert(
                BString::from("require-dev"),
                (*root_package.get_dev_requires()).clone(),
            );
            let loader = ArrayLoader::new(None, false);
            let requirements_mixed: IndexMap<BString, PhpMixed> = requirements
                .iter()
                .map(|(k, v)| (k.clone(), PhpMixed::string(v.clone())))
                .collect();
            let new_links = loader.parse_links(
                bstr::BStr::new(&root_package.get_name()),
                bstr::BStr::new(&root_package.get_pretty_version()),
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
                    section.shift_remove(package.as_bytes());
                }
            }
            root_package.set_requires(links[B("require")].clone())?;
            root_package.set_dev_requires(links[B("require-dev")].clone())?;

            // extract stability flags & references as they weren't present when loading the unmodified composer.json
            let references =
                RootPackageLoader::extract_references(requirements, root_package.get_references());
            root_package.set_references(references);
            let stability_flags = RootPackageLoader::extract_stability_flags(
                requirements,
                bstr::BStr::new(&root_package.get_minimum_stability()),
                root_package.get_stability_flags(),
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
                .get_truthy("optimize-autoloader")?;
        let authoritative = input
            .borrow()
            .get_option("classmap-authoritative")?
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get_truthy("classmap-authoritative")?;
        let apcu_prefix = input
            .borrow()
            .get_option("apcu-autoloader-prefix")?
            .as_string()
            .map(BStr::to_owned);
        let apcu = apcu_prefix.is_some()
            || input
                .borrow()
                .get_option("apcu-autoloader")?
                .as_bool()
                .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get_truthy("apcu-autoloader")?;
        let minimal_changes = input
            .borrow()
            .get_option("minimal-changes")?
            .as_bool()
            .unwrap_or(false)
            || composer
                .get_config()
                .borrow()
                .get_truthy("update-with-minimal-changes")?;

        let mut update_allow_transitive_dependencies = UpdateAllowTransitiveDeps::UpdateOnlyListed;
        let mut flags = BString::default();
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
            flags.push_str(" --with-all-dependencies");
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
            flags.push_str(" --with-dependencies");
        }

        io.write_error(bconcat!(
            "<info>Running shirabe update ",
            implode(" ", &array_keys(requirements)),
            flags,
            "</info>"
        ));

        let command_event =
            CommandEvent::new(PluginEvents::COMMAND, "require", input.clone(), output);
        let command_event_name = command_event.get_name().to_owned();
        composer.get_event_dispatcher().borrow_mut().dispatch(
            Some(command_event_name.as_ref()),
            Some(std::rc::Rc::new(std::cell::RefCell::new(command_event))),
        )?;

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
            .set_apcu_autoloader(apcu, apcu_prefix)
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
            install.set_update_allow_list(array_keys(requirements));
        }

        let status = install.run().await?;
        if status != 0 && status != Installer::ERROR_AUDIT_FAILED {
            if status == Installer::ERROR_DEPENDENCY_RESOLUTION_FAILED {
                for req in BaseCommand::normalize_requirements(
                    self,
                    input
                        .borrow()
                        .get_argument("packages")?
                        .as_array()
                        .map(<[bstr::BString]>::to_vec)
                        .unwrap_or_default(),
                )? {
                    if !req.contains_key(B("version")) {
                        io.write_error(bconcat!("You can also try re-running shirabe require with an explicit version constraint, e.g. \"shirabe require ", req.get(B("name")).cloned().unwrap_or_default(), ":*\" to figure out if any version is installable, or \"shirabe require ", req.get(B("name")).cloned().unwrap_or_default(), ":^2.1\" if you know which you need."));
                        break;
                    }
                }
            }
            self.revert_composer_file();
        }

        Ok(status)
    }

    fn update_requirements_after_resolution(
        &self,
        requirements_to_update: &[BString],
        require_key: &BStr,
        remove_key: &BStr,
        sort_packages: bool,
        dry_run: bool,
        fixed: bool,
    ) -> anyhow::Result<i64> {
        let composer = self.require_composer(None, None)?;
        let composer = composer_full(&composer);
        let locker_is_locked = composer.get_locker().borrow_mut().is_locked();
        let mut requirements: IndexMap<BString, BString> = IndexMap::new();
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
        let repo: RepositoryInterfaceHandle = if locker_is_locked {
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
                bstr::BStr::new(package_name),
                FindPackageConstraint::String(BString::from("*")),
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
            self.get_io().write_error(bconcat!(
                "Using version <info>",
                requirements.get(package_name).cloned().unwrap_or_default(),
                "</info> for <info>",
                package_name,
                "</info>"
            ));

            // Regex pattern compatibility:
            // PCRE `{^dev-(?!main$|master$|trunk$|latest$)}` uses a negative lookahead,
            // which the `regex` crate does not support. Decomposed into hand-written logic.
            let requirement_str = requirements.get(package_name).cloned().unwrap_or_default();
            if requirement_str.starts_with(b"dev-")
                && !matches!(
                    &requirement_str[4..],
                    b"main" | b"master" | b"trunk" | b"latest"
                )
            {
                self.get_io().warning(
                    bconcat!("Version ", requirement_str, " looks like it may be a feature branch which is unlikely to keep working in the long run and may be in an unstable state"),
                    &[],
                );
                if self.get_io().is_interactive()
                    && !self.get_io().ask_confirmation(
                        "Are you sure you want to use this constraint (<comment>y</comment>) or would you rather abort (<comment>n</comment>) the whole operation [<comment>y,n</comment>]? ",
                        true,
                    )?
                {
                    self.revert_composer_file();

                    return Ok(1);
                }
            }
        }

        if !dry_run {
            let json = self.json.borrow().as_ref().unwrap().clone();
            self.update_file(&json, &requirements, require_key, remove_key, sort_packages);
            if locker_is_locked && composer.get_config().borrow().get_bool("lock") {
                let stability_flags = RootPackageLoader::extract_stability_flags(
                    &(requirements),
                    bstr::BStr::new(&composer.get_package().get_minimum_stability()),
                    IndexMap::new(),
                );
                composer.get_locker().borrow_mut().update_hash(
                    &json.borrow(),
                    Some(Box::new(move |mut lock_data| {
                        let section = lock_data
                            .entry("stability-flags".into())
                            .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
                        if let Some(section) = section.as_array_mut() {
                            for (package_name, flag) in &stability_flags {
                                section.insert(package_name.as_bstr().into(), PhpMixed::Int(*flag));
                            }
                        }
                        lock_data
                    })),
                )?;
            }
        }

        Ok(0)
    }

    fn update_file(
        &self,
        json: &std::rc::Rc<std::cell::RefCell<JsonFile>>,
        new: &IndexMap<BString, BString>,
        require_key: &BStr,
        remove_key: &BStr,
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
                .entry(require_key.to_owned())
                .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
            if let Some(section) = section.as_array_mut() {
                section.insert(package.clone(), PhpMixed::string(version.clone()));
            }
            if let Some(section) = composer_definition
                .get_mut(B(remove_key))
                .and_then(|v| v.as_array_mut())
            {
                section.shift_remove(B(package));
            }
            let remove_empty = composer_definition
                .get(B(remove_key))
                .and_then(|v| v.as_array())
                .map(|m| m.is_empty())
                .unwrap_or(false);
            if remove_empty && composer_definition.contains_key(B(remove_key)) {
                composer_definition.shift_remove(B(remove_key));
            }
        }
        let _ = json.borrow().write(PhpMixed::Array(composer_definition));
    }

    fn update_file_cleanly(
        &self,
        json: &std::rc::Rc<std::cell::RefCell<JsonFile>>,
        new: &IndexMap<BString, BString>,
        require_key: &BStr,
        remove_key: &BStr,
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
            let msg = bconcat!(
                "\n<error>Installation failed, deleting ",
                &*self.file.borrow(),
                ".</error>"
            );
            self.get_io().write_error(&msg);
            unlink(json.borrow().get_path());
            if file_exists(&lock) {
                unlink(&lock);
            }
        } else {
            let extra = if self.lock_backup.borrow().is_some() {
                bconcat!(" and ", lock, " to their ")
            } else {
                BString::from(" to its ")
            };
            let msg = bconcat!(
                "\n<error>Installation failed, reverting ",
                &*self.file.borrow(),
                extra,
                "original content.</error>"
            );
            self.get_io().write_error(&msg);
            file_put_contents(json.borrow().get_path(), &*self.composer_backup.borrow());
            if let Some(ref lock_backup) = *self.lock_backup.borrow() {
                file_put_contents(&lock, lock_backup);
            }
        }
    }
}

impl PackageDiscoveryTrait for RequireCommand {
    fn get_repos_mut(&self) -> std::cell::RefMut<'_, Option<RepositoryInterfaceHandle>> {
        self.repos.borrow_mut()
    }

    fn get_repository_sets_mut(
        &self,
    ) -> std::cell::RefMut<'_, IndexMap<BString, std::rc::Rc<std::cell::RefCell<RepositorySet>>>>
    {
        self.repository_sets.borrow_mut()
    }
}

#[async_trait::async_trait(?Send)]
impl Command for RequireCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("require".into())?;
        self.set_aliases(vec!["r".into()])?;
        self.set_description(
            "Adds required packages to your composer.json and installs them".into(),
        );
        self.set_definition(&[
            InputArgument::with_suggested_values("packages", Some(InputArgument::IS_ARRAY | InputArgument::OPTIONAL), "Optional package name can also include a version constraint, e.g. foo/bar or foo/bar:1.0.0 or foo/bar=1.0.0 or \"foo/bar 1.0.0\"", None, self.suggest_available_package_incl_platform()).unwrap().into(),
            InputOption::new("dev", None, Some(InputOption::VALUE_NONE), "Add requirement to require-dev.", None).unwrap().into(),
            InputOption::new("dry-run", None, Some(InputOption::VALUE_NONE), "Outputs the operations but will not execute anything (implicitly enables --verbose).", None).unwrap().into(),
            InputOption::new("prefer-source", None, Some(InputOption::VALUE_NONE), "Forces installation from package sources when possible, including VCS information.", None).unwrap().into(),
            InputOption::new("prefer-dist", None, Some(InputOption::VALUE_NONE), "Forces installation from package dist (default behavior).", None).unwrap().into(),
            InputOption::with_suggested_values("prefer-install", None, Some(InputOption::VALUE_REQUIRED), "Forces installation from package dist|source|auto (auto chooses source for dev versions, dist for the rest).", None, self.suggest_prefer_install()).unwrap().into(),
            InputOption::new("fixed", None, Some(InputOption::VALUE_NONE), "Write fixed version to the composer.json.", None).unwrap().into(),
            InputOption::new("no-suggest", None, Some(InputOption::VALUE_NONE), "DEPRECATED: This flag does not exist anymore.", None).unwrap().into(),
            InputOption::new("no-progress", None, Some(InputOption::VALUE_NONE), "Do not output download progress.", None).unwrap().into(),
            InputOption::new("no-update", None, Some(InputOption::VALUE_NONE), "Disables the automatic update of the dependencies (implies --no-install).", None).unwrap().into(),
            InputOption::new("no-install", None, Some(InputOption::VALUE_NONE), "Skip the install step after updating the composer.lock file.", None).unwrap().into(),
            InputOption::new("no-audit", None, Some(InputOption::VALUE_NONE), "Skip the audit step after updating the composer.lock file (can also be set via the COMPOSER_NO_AUDIT=1 env var).", None).unwrap().into(),
            InputOption::with_suggested_values("audit-format", None, Some(InputOption::VALUE_REQUIRED), "Audit output format. Must be \"table\", \"plain\", \"json\", or \"summary\".", Some(InputValue::from(Auditor::FORMAT_SUMMARY)), SuggestedValues::List(Auditor::FORMATS.iter().map(|s| (*s).to_owned()).collect())).unwrap().into(),
            InputOption::new("no-security-blocking", None, Some(InputOption::VALUE_NONE), "Allows installing packages with security advisories or that are abandoned (can also be set via the COMPOSER_NO_SECURITY_BLOCKING=1 env var).", None).unwrap().into(),
            InputOption::new("update-no-dev", None, Some(InputOption::VALUE_NONE), "Run the dependency update with the --no-dev option.", None).unwrap().into(),
            InputOption::new("update-with-dependencies", Some("w".into()), Some(InputOption::VALUE_NONE), "Allows inherited dependencies to be updated, except those that are root requirements (can also be set via the COMPOSER_WITH_DEPENDENCIES=1 env var).", None).unwrap().into(),
            InputOption::new("update-with-all-dependencies", Some("W".into()), Some(InputOption::VALUE_NONE), "Allows all inherited dependencies to be updated, including those that are root requirements (can also be set via the COMPOSER_WITH_ALL_DEPENDENCIES=1 env var).", None).unwrap().into(),
            InputOption::new("with-dependencies", None, Some(InputOption::VALUE_NONE), "Alias for --update-with-dependencies", None).unwrap().into(),
            InputOption::new("with-all-dependencies", None, Some(InputOption::VALUE_NONE), "Alias for --update-with-all-dependencies", None).unwrap().into(),
            InputOption::new("ignore-platform-req", None, Some(InputOption::VALUE_REQUIRED | InputOption::VALUE_IS_ARRAY), "Ignore a specific platform requirement (php & ext- packages).", None).unwrap().into(),
            InputOption::new("ignore-platform-reqs", None, Some(InputOption::VALUE_NONE), "Ignore all platform requirements (php & ext- packages).", None).unwrap().into(),
            InputOption::new("prefer-stable", None, Some(InputOption::VALUE_NONE), "Prefer stable versions of dependencies (can also be set via the COMPOSER_PREFER_STABLE=1 env var).", None).unwrap().into(),
            InputOption::new("prefer-lowest", None, Some(InputOption::VALUE_NONE), "Prefer lowest versions of dependencies (can also be set via the COMPOSER_PREFER_LOWEST=1 env var).", None).unwrap().into(),
            InputOption::new("minimal-changes", Some("m".into()), Some(InputOption::VALUE_NONE), "During an update with -w/-W, only perform absolutely necessary changes to transitive dependencies (can also be set via the COMPOSER_MINIMAL_CHANGES=1 env var).", None).unwrap().into(),
            InputOption::new("sort-packages", None, Some(InputOption::VALUE_NONE), "Sorts packages when adding/updating a new dependency", None).unwrap().into(),
            InputOption::new("optimize-autoloader", Some("o".into()), Some(InputOption::VALUE_NONE), "Optimize autoloader during autoloader dump", None).unwrap().into(),
            InputOption::new("classmap-authoritative", Some("a".into()), Some(InputOption::VALUE_NONE), "Autoload classes from the classmap only. Implicitly enables `--optimize-autoloader`.", None).unwrap().into(),
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
            Read more at https://getcomposer.org/doc/03-cli.md#require-r".into()
        );
        Ok(())
    }

    async fn execute(
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
            self.get_io().write_error("<warning>You are using the deprecated option \"--no-suggest\". It has no effect and will break in Composer 3.</warning>");
        }

        let file = self.file.borrow().clone();
        self.newly_created.set(!file_exists(&file));
        let write_failed =
            self.newly_created.get() && file_put_contents(&file, b"{\n}\n").is_none();
        if write_failed {
            let msg = bconcat!("<error>", file, " could not be created.</error>");
            self.get_io().write_error(&msg);

            return Ok(1);
        }
        if !Filesystem::is_readable(&file) {
            let msg = bconcat!("<error>", file, " is not readable.</error>");
            self.get_io().write_error(&msg);

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
            file_get_contents(&lock).ok()
        } else {
            None
        };

        let signals = SignalSubscription::new();
        let abort_on_signal = |signals: &SignalSubscription| {
            self.get_io().write_error_with_verbosity(
                bconcat!("Received ", signals.last_signal().as_str(), ", aborting"),
                io_interface::DEBUG,
            );
            self.revert_composer_file();
            signals.exit_with_last_signal();
        };

        // check for writability by writing to the file as is_writable can not be trusted on network-mounts
        // see https://github.com/composer/composer/issues/8231 and https://bugs.php.net/bug.php?id=68926
        let backup_contents = self.composer_backup.borrow().clone();
        if !is_writable(&file) && file_put_contents(&file, &backup_contents).is_none() {
            let msg = bconcat!("<error>", file, " is not writable.</error>");
            self.get_io().write_error(&msg);

            return Ok(1);
        }

        if input.borrow().get_option("fixed")?.as_bool() == Some(true) {
            let config = json.borrow_mut().read()?;

            let package_type = if empty(&config.get("type").cloned().unwrap_or(PhpMixed::Null)) {
                BString::from("library")
            } else {
                BString::from(
                    config
                        .get("type")
                        .and_then(|v| v.as_string())
                        .unwrap_or_default(),
                )
            };

            // @see https://github.com/composer/composer/pull/8313#issuecomment-532637955
            if package_type != "project"
                && !input.borrow().get_option("dev")?.as_bool().unwrap_or(false)
            {
                self.get_io().write_error("<error>The \"--fixed\" option is only allowed for packages with a \"project\" type or for dev dependencies to prevent possible misuses.</error>");

                if config.get("type").is_none() {
                    self.get_io().write_error("<error>If your package is not a library, you can explicitly specify the \"type\" by using \"shirabe config type project\".</error>");
                }

                return Ok(1);
            }
        }

        let composer = self.require_composer(None, None)?;
        let composer = composer_full(&composer);
        let repository_manager = composer.get_repository_manager().clone();
        let repository_manager = repository_manager.borrow();
        let repos = repository_manager.get_repositories();

        let platform_overrides_map = composer.get_config().borrow().get_map("platform")?;
        // initialize self.repos as it is used by the PackageDiscoveryTrait
        let platform_repo = PlatformRepositoryHandle::new(PlatformRepository::new(
            vec![],
            platform_overrides_map,
            None,
            None,
        )?);
        let mut combined: Vec<RepositoryInterfaceHandle> = vec![platform_repo.clone().into()];
        for repo in repos {
            combined.push(repo.clone());
        }
        *self.get_repos_mut() = Some(RepositoryInterfaceHandle::new(CompositeRepository::new(
            combined,
        )));

        let preferred_stability = if composer.get_package().get_prefer_stable() {
            BString::from("stable")
        } else {
            composer.get_package().get_minimum_stability()
        };

        let packages: Vec<bstr::BString> = input
            .borrow()
            .get_argument("packages")?
            .as_array()
            .map(<[bstr::BString]>::to_vec)
            .unwrap_or_default();
        // if there is no update, we need to use the best possible version constraint directly as we cannot rely on the solver to guess the best constraint
        let no_update = input
            .borrow()
            .get_option("no-update")?
            .as_bool()
            .unwrap_or(false);
        let fixed = input
            .borrow()
            .get_option("fixed")?
            .as_bool()
            .unwrap_or(false);
        let requirements_result = self.determine_requirements(
            input.clone(),
            output.clone(),
            packages,
            Some(&platform_repo),
            bstr::BStr::new(&preferred_stability),
            no_update,
            fixed,
        );

        if signals.is_triggered() {
            abort_on_signal(&signals);
        }

        let requirements = match requirements_result {
            Ok(r) => r,
            Err(e) => {
                if self.newly_created.get() {
                    self.revert_composer_file();

                    return Err(RuntimeException::with_code_and_previous(
                        bconcat!(
                            "No composer.json present in the current directory (",
                            &*self.file.borrow(),
                            "), this may be the cause of the following exception."
                        ),
                        0,
                        Some(std::sync::Arc::new(e)),
                    )
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
            let mut dev_packages: Vec<Vec<BString>> = vec![];
            let dev_tags: Vec<BString> =
                vec!["dev".into(), "testing".into(), "static analysis".into()];
            let current_requires_by_key = self.get_packages_by_require_key();
            for (name, _version) in &requirements {
                // skip packages which are already in the composer.json as those have already been decided
                if current_requires_by_key.contains_key(name) {
                    continue;
                }

                let found_packages: Vec<PackageInterfaceHandle> = self
                    .get_repos()
                    .find_packages(bstr::BStr::new(name), None)?
                    .into_iter()
                    .collect();
                let pkg: Option<PackageInterfaceHandle> =
                    PackageSorter::get_most_current_version(found_packages);
                let pkg_as_complete: Option<CompletePackageInterfaceHandle> =
                    pkg.as_ref().and_then(|p| p.as_complete());
                if let Some(pkg_complete) = pkg_as_complete {
                    let lowered: Vec<BString> = pkg_complete
                        .get_keywords()
                        .iter()
                        .map(strtolower)
                        .collect::<Vec<_>>();
                    let pkg_dev_tags: Vec<BString> = array_intersect(&dev_tags, &lowered);
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
                let merged: Vec<BString> = dev_packages.iter().flatten().cloned().collect();
                let pkg_dev_tags: Vec<BString> = array_unique(&merged);
                let warn_msg = bconcat!(
                    "The package",
                    plural,
                    " you required ",
                    plural2,
                    " recommended to be placed in require-dev (because ",
                    plural3,
                    " tagged as \"",
                    implode("\", \"", &pkg_dev_tags),
                    "\") but you did not use --dev."
                );
                self.get_io().warning(&warn_msg, &[]);
                if self.get_io().ask_confirmation(
                    "<info>Do you want to re-run the command with --dev?</> [<comment>yes</>]? ",
                    true,
                )? {
                    input
                        .borrow_mut()
                        .set_option("dev", InputValue::Bool(true))?;
                }
            }

            // unset($devPackages, $pkgDevTags);
        }

        let mut require_key = if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
            BStr::new("require-dev")
        } else {
            BStr::new("require")
        };
        let mut remove_key = if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
            BStr::new("require")
        } else {
            BStr::new("require-dev")
        };

        // check which requirements need the version guessed
        let mut requirements_to_guess: Vec<BString> = vec![];
        for (package, constraint) in requirements.clone().iter() {
            if constraint == "guess" {
                requirements.insert(package.clone(), "*".into());
                requirements_to_guess.push(package.clone());
            }
        }

        // validate requirements format
        let version_parser = VersionParser::new();
        for (package, constraint) in &requirements {
            if strtolower(package) == composer.get_package().get_name() {
                let msg = bconcat!(
                    "<error>Root package '",
                    package,
                    "' cannot require itself in its composer.json</error>"
                );
                self.get_io().write_error(&msg);

                return Ok(1);
            }
            if constraint == "self.version" {
                continue;
            }
            version_parser.parse_constraints(constraint)?;
        }

        let inconsistent_require_keys =
            self.get_inconsistent_require_keys(&requirements, BStr::new(require_key));
        if (inconsistent_require_keys.len() as i64) > 0 {
            for package in &inconsistent_require_keys {
                let warn_msg = bconcat!(
                    package,
                    " is currently present in the ",
                    remove_key,
                    " key and you ran the command ",
                    if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
                        "with"
                    } else {
                        "without"
                    },
                    " the --dev flag, which will move it to the ",
                    require_key,
                    " key."
                );
                self.get_io().warning(&warn_msg, &[]);
            }

            if self.get_io().is_interactive() {
                let q1 = bconcat!(
                    "<info>Do you want to move ",
                    if (inconsistent_require_keys.len() as i64) > 1 {
                        "these requirements"
                    } else {
                        "this requirement"
                    },
                    "?</info> [<comment>no</comment>]? "
                );
                if !self.get_io().ask_confirmation(q1, false)? {
                    let q2 = bconcat!(
                        "<info>Do you want to re-run the command ",
                        if input.borrow().get_option("dev")?.as_bool().unwrap_or(false) {
                            "without"
                        } else {
                            "with"
                        },
                        " --dev?</info> [<comment>yes</comment>]? "
                    );
                    if !self.get_io().ask_confirmation(q2, true)? {
                        return Ok(0);
                    }

                    input
                        .borrow_mut()
                        .set_option("dev", InputValue::Bool(true))?;
                    std::mem::swap(&mut require_key, &mut remove_key);
                }
            }
        }

        let sort_packages = input
            .borrow()
            .get_option("sort-packages")?
            .as_bool()
            .unwrap_or(false)
            || composer.get_config().borrow().get_truthy("sort-packages")?;

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

        if signals.is_triggered() {
            abort_on_signal(&signals);
        }

        let updated_msg = bconcat!(
            "<info>",
            file,
            " has been ",
            if self.newly_created.get() {
                "created"
            } else {
                "updated"
            },
            "</info>"
        );
        self.get_io().write_error(&updated_msg);

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
        let do_update_result = self
            .do_update(
                input.clone(),
                output,
                io,
                &requirements,
                require_key,
                remove_key,
            )
            .await;
        if signals.is_triggered() {
            abort_on_signal(&signals);
        }
        let dry_run = input
            .borrow()
            .get_option("dry-run")?
            .as_bool()
            .unwrap_or(false);

        let result = match do_update_result {
            Ok(result) => {
                let final_result = if result == 0 && (requirements_to_guess.len() as i64) > 0 {
                    let fixed = input
                        .borrow()
                        .get_option("fixed")?
                        .as_bool()
                        .unwrap_or(false);
                    self.update_requirements_after_resolution(
                        &requirements_to_guess,
                        require_key,
                        remove_key,
                        sort_packages,
                        dry_run,
                        fixed,
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

        if signals.is_triggered() {
            abort_on_signal(&signals);
        }

        // finally
        if dry_run && self.newly_created.get() {
            // @unlink($this->json->getPath());
            unlink(json.borrow().get_path());
        }

        result
    }

    fn interact(
        &self,
        _input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) {
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        base_command_initialize(self, input, output)
    }

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for RequireCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    delegate_base_command_trait_impls_to_inner!(base_command_data);
}
