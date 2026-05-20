//! ref: composer/src/Composer/Installer.php

mod binary_installer;
mod binary_presence_interface;
mod installation_manager;
mod installer_event;
mod installer_events;
mod installer_interface;
mod library_installer;
mod metapackage_installer;
mod noop_installer;
mod package_event;
mod package_events;
mod plugin_installer;
mod project_installer;
mod suggested_packages_reporter;

pub use binary_installer::*;
pub use binary_presence_interface::*;
pub use installation_manager::*;
pub use installer_event::*;
pub use installer_events::*;
pub use installer_interface::*;
pub use library_installer::*;
pub use metapackage_installer::*;
pub use noop_installer::*;
pub use package_event::*;
pub use package_events::*;
pub use plugin_installer::*;
pub use project_installer::*;
pub use suggested_packages_reporter::*;

use crate::io::io_interface;
use indexmap::IndexMap;

use shirabe_external_packages::seld::json_lint::ParsingException;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, array_flip, array_map, array_merge, array_unique, array_values,
    clone, count, defined, gc_collect_cycles, gc_disable, gc_enable, get_class, implode, in_array,
    intval, is_dir, is_numeric, is_string, max_i64, sprintf, strcmp, strpos, strtolower, touch,
    trigger_error, usort,
};
use shirabe_semver;

use crate::advisory::AuditConfig;
use crate::advisory::Auditor;
use crate::autoload::AutoloadGenerator;
use crate::composer::Composer;
use crate::config::Config;
use crate::console::GithubActionError;
use crate::dependency_resolver::DefaultPolicy;
use crate::dependency_resolver::LocalRepoTransaction;
use crate::dependency_resolver::LockTransaction;
use crate::dependency_resolver::PolicyInterface;
use crate::dependency_resolver::Pool;
use crate::dependency_resolver::PoolOptimizer;
use crate::dependency_resolver::Request;
use crate::dependency_resolver::SecurityAdvisoryPoolFilter;
use crate::dependency_resolver::Solver;
use crate::dependency_resolver::SolverProblemsException;
use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::OperationInterface;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::dependency_resolver::operation::UpdateOperation;
use crate::downloader::DownloadManager;
use crate::downloader::TransportException;
use crate::event_dispatcher::EventDispatcher;
use crate::filter::platform_requirement_filter::IgnoreListPlatformRequirementFilter;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::io::IOInterface;
use crate::package::AliasPackage;
use crate::package::CompletePackage;
use crate::package::CompletePackageInterface;
use crate::package::Link;
use crate::package::Locker;
use crate::package::Package;
use crate::package::PackageInterface;
use crate::package::RootAliasPackage;
use crate::package::RootPackageInterface;
use crate::package::base_package::{self, BasePackage};
use crate::package::dumper::ArrayDumper;
use crate::package::loader::ArrayLoader;
use crate::package::loader::LoaderInterface;
use crate::package::version::VersionParser;
use crate::repository::ArrayRepository;
use crate::repository::CanonicalPackagesTrait;
use crate::repository::CompositeRepository;
use crate::repository::InstalledArrayRepository;
use crate::repository::InstalledRepository;
use crate::repository::InstalledRepositoryInterface;
use crate::repository::LockArrayRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryManager;
use crate::repository::RepositorySet;
use crate::repository::RootPackageRepository;
use crate::script::ScriptEvents;
use crate::util::Platform;
use shirabe_semver::constraint::Constraint;
use shirabe_semver::constraint::ConstraintInterface;

#[derive(Debug)]
pub struct Installer {
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) config: std::rc::Rc<std::cell::RefCell<Config>>,
    pub(crate) package: Box<dyn RootPackageInterface>,
    // TODO can we get rid of the below and just use the package itself?
    pub(crate) fixed_root_package: Box<dyn RootPackageInterface>,
    pub(crate) download_manager: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
    pub(crate) repository_manager: RepositoryManager,
    pub(crate) locker: Locker,
    pub(crate) installation_manager: InstallationManager,
    pub(crate) event_dispatcher: std::rc::Rc<std::cell::RefCell<EventDispatcher>>,
    pub(crate) autoload_generator: AutoloadGenerator,
    pub(crate) prefer_source: bool,
    pub(crate) prefer_dist: bool,
    pub(crate) optimize_autoloader: bool,
    pub(crate) class_map_authoritative: bool,
    pub(crate) apcu_autoloader: bool,
    pub(crate) apcu_autoloader_prefix: Option<String>,
    pub(crate) dev_mode: bool,
    pub(crate) dry_run: bool,
    pub(crate) download_only: bool,
    pub(crate) verbose: bool,
    pub(crate) update: bool,
    pub(crate) install: bool,
    pub(crate) dump_autoloader: bool,
    pub(crate) run_scripts: bool,
    pub(crate) prefer_stable: bool,
    pub(crate) prefer_lowest: bool,
    pub(crate) minimal_update: bool,
    pub(crate) write_lock: bool,
    pub(crate) execute_operations: bool,
    pub(crate) audit: bool,
    pub(crate) error_on_audit: bool,
    pub(crate) audit_format: String,
    audit_config: Option<AuditConfig>,
    ignored_types: Vec<String>,
    allowed_types: Option<Vec<String>>,
    pub(crate) update_mirrors: bool,
    pub(crate) update_allow_list: Option<Vec<String>>,
    pub(crate) update_allow_transitive_dependencies: i64,
    pub(crate) suggested_packages_reporter: SuggestedPackagesReporter,
    pub(crate) platform_requirement_filter: Box<dyn PlatformRequirementFilterInterface>,
    pub(crate) additional_fixed_repository: Option<Box<dyn RepositoryInterface>>,
    pub(crate) temporary_constraints: IndexMap<String, Box<dyn ConstraintInterface>>,
}

impl Installer {
    pub const ERROR_NONE: i64 = 0; // no error/success state
    pub const ERROR_GENERIC_FAILURE: i64 = 1;
    pub const ERROR_NO_LOCK_FILE_FOR_PARTIAL_UPDATE: i64 = 3;
    pub const ERROR_LOCK_FILE_INVALID: i64 = 4;
    // used/declared in SolverProblemsException, carried over here for completeness
    pub const ERROR_DEPENDENCY_RESOLUTION_FAILED: i64 = 2;
    pub const ERROR_AUDIT_FAILED: i64 = 5;
    // technically exceptions are thrown with various status codes >400, but the process exit code is normalized to 100
    pub const ERROR_TRANSPORT_EXCEPTION: i64 = 100;

    pub fn new(
        io: Box<dyn IOInterface>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        package: Box<dyn RootPackageInterface>,
        download_manager: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
        repository_manager: RepositoryManager,
        locker: Locker,
        installation_manager: InstallationManager,
        event_dispatcher: std::rc::Rc<std::cell::RefCell<EventDispatcher>>,
        autoload_generator: AutoloadGenerator,
    ) -> Self {
        let suggested_packages_reporter = SuggestedPackagesReporter::new(io.clone_box());
        let platform_requirement_filter = PlatformRequirementFilterFactory::ignore_nothing();
        let write_lock = config.borrow_mut().get("lock").as_bool().unwrap_or(false);

        Self {
            io,
            config,
            package: package.clone_box(),
            fixed_root_package: package.clone_box(),
            download_manager,
            repository_manager,
            locker,
            installation_manager,
            event_dispatcher,
            autoload_generator,
            prefer_source: false,
            prefer_dist: false,
            optimize_autoloader: false,
            class_map_authoritative: false,
            apcu_autoloader: false,
            apcu_autoloader_prefix: None,
            dev_mode: false,
            dry_run: false,
            download_only: false,
            verbose: false,
            update: false,
            install: true,
            dump_autoloader: true,
            run_scripts: true,
            prefer_stable: false,
            prefer_lowest: false,
            minimal_update: false,
            write_lock,
            execute_operations: true,
            audit: true,
            error_on_audit: false,
            audit_format: Auditor::FORMAT_SUMMARY.to_string(),
            audit_config: None,
            ignored_types: vec!["php-ext".to_string(), "php-ext-zend".to_string()],
            allowed_types: None,
            update_mirrors: false,
            update_allow_list: None,
            update_allow_transitive_dependencies: Request::UPDATE_ONLY_LISTED,
            suggested_packages_reporter,
            platform_requirement_filter,
            additional_fixed_repository: None,
            temporary_constraints: IndexMap::new(),
        }
    }

    /// Run installation (or update)
    pub fn run(&mut self) -> anyhow::Result<i64> {
        // Disable GC to save CPU cycles, as the dependency solver can create hundreds of thousands
        // of PHP objects, the GC can spend quite some time walking the tree of references looking
        // for stuff to collect while there is nothing to collect. This slows things down dramatically
        // and turning it off results in much better performance. Do not try this at home however.
        gc_collect_cycles();
        gc_disable();

        if self.update_allow_list.is_some() && self.update_mirrors {
            return Err(RuntimeException {
                message: "The installer options updateMirrors and updateAllowList are mutually exclusive.".to_string(),
                code: 0,
            }.into());
        }

        let is_fresh_install = self.repository_manager.get_local_repository().is_fresh();

        // Force update if there is no lock file present
        if !self.update && !self.locker.is_locked() {
            self.io.write_error("<warning>No composer.lock file present. Updating dependencies to latest instead of installing from lock file. See https://getcomposer.org/install for more information.</warning>");
            self.update = true;
        }

        if self.dry_run {
            self.verbose = true;
            self.run_scripts = false;
            self.execute_operations = false;
            self.write_lock = false;
            self.dump_autoloader = false;
            // TODO(phase-b): borrow conflict: passing &mut self.repository_manager while &self
            // is implicit. Refactor mock_local_repositories or split borrow.
            // self.mock_local_repositories(&mut self.repository_manager);
        }

        if self.download_only {
            self.dump_autoloader = false;
        }

        if self.update && !self.install {
            self.dump_autoloader = false;
        }

        if self.run_scripts {
            Platform::put_env("COMPOSER_DEV_MODE", if self.dev_mode { "1" } else { "0" });

            // dispatch pre event
            // should we treat this more strictly as running an update and then running an install, triggering events multiple times?
            let event_name = if self.update {
                ScriptEvents::PRE_UPDATE_CMD
            } else {
                ScriptEvents::PRE_INSTALL_CMD
            };
            self.event_dispatcher.borrow_mut().dispatch_script(
                event_name,
                self.dev_mode,
                vec![],
                IndexMap::new(),
            );
        }

        self.download_manager
            .borrow_mut()
            .set_prefer_source(self.prefer_source);
        self.download_manager
            .borrow_mut()
            .set_prefer_dist(self.prefer_dist);

        let local_repo_box = self
            .repository_manager
            .get_local_repository()
            .clone_installed_repository_box();

        let install = self.install;
        let res_result: anyhow::Result<i64> = if self.update {
            self.do_update(local_repo_box, install)
        } else {
            self.do_install(local_repo_box, false)
        };

        let res = match res_result {
            Ok(r) => {
                if r != 0 {
                    return Ok(r);
                }
                r
            }
            Err(e) => {
                if self.execute_operations
                    && self.install
                    && self
                        .config
                        .borrow_mut()
                        .get("notify-on-install")
                        .as_bool()
                        .unwrap_or(false)
                {
                    self.installation_manager.notify_installs(&*self.io);
                }
                return Err(e);
            }
        };
        let _ = res;

        if self.execute_operations
            && self.install
            && self
                .config
                .borrow_mut()
                .get("notify-on-install")
                .as_bool()
                .unwrap_or(false)
        {
            self.installation_manager.notify_installs(&*self.io);
        }

        if self.update {
            let installed_repo = InstalledRepository::new(vec![
                self.locker
                    .get_locked_repository(self.dev_mode)?
                    .clone_box(),
                Box::new(self.create_platform_repo(false)),
                Box::new(RootPackageRepository::new(self.package.clone_box())),
            ]);
            if is_fresh_install {
                self.suggested_packages_reporter
                    .add_suggestions_from_package(&*self.package);
            }
            self.suggested_packages_reporter
                .output_minimalistic(Some(&installed_repo), None);
        }

        // Find abandoned packages and warn user
        let locked_repository = self.locker.get_locked_repository(true)?;
        for package in CanonicalPackagesTrait::get_packages(&locked_repository) {
            let complete = match package.as_complete_package() {
                Some(p) if p.is_abandoned() => p,
                _ => continue,
            };

            let replacement = if let Some(repl) = complete.get_replacement_package() {
                format!("Use {} instead", repl)
            } else {
                "No replacement was suggested".to_string()
            };

            self.io.write_error(&sprintf(
                "<warning>Package %s is abandoned, you should avoid using it. %s.</warning>",
                &[complete.get_pretty_name().into(), replacement.into()],
            ));
        }

        if self.dump_autoloader {
            // write autoloader
            if self.optimize_autoloader {
                self.io
                    .write_error("<info>Generating optimized autoload files</info>");
            } else {
                self.io
                    .write_error("<info>Generating autoload files</info>");
            }

            self.autoload_generator
                .set_class_map_authoritative(self.class_map_authoritative);
            self.autoload_generator
                .set_apcu(self.apcu_autoloader, self.apcu_autoloader_prefix.clone());
            self.autoload_generator.set_run_scripts(self.run_scripts);
            self.autoload_generator
                .set_platform_requirement_filter(self.platform_requirement_filter.clone_box());
            self.autoload_generator.dump(
                &*self.config.borrow(),
                self.repository_manager.get_local_repository(),
                &*self.package,
                &mut self.installation_manager,
                "composer",
                self.optimize_autoloader,
                None,
                Some(&mut self.locker),
                false,
            )?;
        }

        if self.install && self.execute_operations {
            // force binaries re-generation in case they are missing
            for package in self
                .repository_manager
                .get_local_repository()
                .get_packages()
            {
                self.installation_manager
                    .ensure_binaries_presence(&*package);
            }
        }

        let fund_env = Platform::get_env("COMPOSER_FUND");
        let mut show_funding = true;
        if let Some(ref s) = fund_env {
            let mixed = PhpMixed::String(s.to_string());
            if is_numeric(&mixed) {
                show_funding = intval(&mixed) != 0;
            }
        }

        if show_funding {
            let mut funding_count: i64 = 0;
            for package in self
                .repository_manager
                .get_local_repository()
                .get_packages()
            {
                if let Some(cp) = package.as_complete_package_interface() {
                    if package.as_alias_package().is_none() && !cp.get_funding().is_empty() {
                        funding_count += 1;
                    }
                }
            }
            if funding_count > 0 {
                self.io.write_error(&sprintf(
                    "<info>%d package%s you are using %s looking for funding.</info>",
                    &[
                        funding_count.into(),
                        (if 1 == funding_count { "" } else { "s" }).into(),
                        (if 1 == funding_count { "is" } else { "are" }).into(),
                    ],
                ));
                self.io
                    .write_error("<info>Use the `composer fund` command to find out more!</info>");
            }
        }

        if self.run_scripts {
            // dispatch post event
            let event_name = if self.update {
                ScriptEvents::POST_UPDATE_CMD
            } else {
                ScriptEvents::POST_INSTALL_CMD
            };
            self.event_dispatcher.borrow_mut().dispatch_script(
                event_name,
                self.dev_mode,
                vec![],
                IndexMap::new(),
            );
        }

        // re-enable GC except on HHVM which triggers a warning here
        if !defined("HHVM_VERSION") {
            gc_enable();
        }

        let audit_config = self.get_audit_config()?;

        if audit_config.audit {
            let (packages, target) = if self.update && !self.install {
                (locked_repository.get_canonical_packages(), "locked")
            } else {
                (
                    self.repository_manager
                        .get_local_repository()
                        .get_canonical_packages(),
                    "installed",
                )
            };
            if packages.len() > 0 {
                let auditor = Auditor;
                let mut repo_set = RepositorySet::new(
                    "stable",
                    IndexMap::new(),
                    vec![],
                    IndexMap::new(),
                    IndexMap::new(),
                    IndexMap::new(),
                );
                for repo in self.repository_manager.get_repositories() {
                    repo_set.add_repository(repo.clone_box())?;
                }

                // TODO(phase-b): Auditor::audit takes owned packages/ignore lists; need cloning
                // strategy. PHP shares these (copy semantics for arrays). Cloning for now is
                // safe because arrays use copy semantics, but trait objects (packages) cannot
                // be cloned trivially.
                let audit_result: anyhow::Result<i64> = todo!();
                let _ = (&auditor, &repo_set, &packages, &audit_config);
                match audit_result {
                    Ok(n) => {
                        return Ok(if n > 0 && self.error_on_audit {
                            Self::ERROR_AUDIT_FAILED
                        } else {
                            0
                        });
                    }
                    Err(e) => {
                        if let Some(te) = e.downcast_ref::<TransportException>() {
                            self.io
                                .error(&format!("Failed to audit {} packages.", target), &[]);
                            if self.io.is_verbose() {
                                self.io.error(
                                    &format!("[{}] {}", "TransportException", te.get_message()),
                                    &[],
                                );
                            }
                        } else {
                            return Err(e);
                        }
                    }
                }
            } else {
                self.io
                    .write_error(&format!("No {} packages - skipping audit.", target));
            }
        }

        Ok(0)
    }

    pub(crate) fn do_update(
        &mut self,
        local_repo: Box<dyn InstalledRepositoryInterface>,
        do_install: bool,
    ) -> anyhow::Result<i64> {
        let platform_repo = self.create_platform_repo(true);
        let aliases = self.get_root_aliases(true);

        let mut locked_repository: Option<LockArrayRepository> = None;

        let mut try_load_locked =
            || -> anyhow::Result<Result<Option<LockArrayRepository>, ParsingException>> {
                if self.locker.is_locked() {
                    match self.locker.get_locked_repository(true) {
                        Ok(r) => Ok(Ok(Some(r))),
                        Err(e) => match e.downcast::<ParsingException>() {
                            Ok(p) => Ok(Err(p)),
                            Err(other) => Err(other),
                        },
                    }
                } else {
                    Ok(Ok(None))
                }
            };

        match try_load_locked()? {
            Ok(r) => locked_repository = r,
            Err(e) => {
                if self.update_allow_list.is_some() || self.update_mirrors {
                    // in case we are doing a partial update or updating mirrors, the lock file is needed so we error
                    return Err(e.into());
                }
                // otherwise, ignoring parse errors as the lock file will be regenerated from scratch when
                // doing a full update
            }
        }

        if (self.update_allow_list.is_some() || self.update_mirrors) && locked_repository.is_none()
        {
            self.io.write_error3(
                &format!(
                    "<error>Cannot update {} without a lock file present. Run `composer update` to generate a lock file.</error>",
                    if self.update_mirrors {
                        "lock file information"
                    } else {
                        "only a partial set of packages"
                    }
                ),
                true,
                io_interface::QUIET,
            );

            return Ok(Self::ERROR_NO_LOCK_FILE_FOR_PARTIAL_UPDATE);
        }

        self.io
            .write_error("<info>Loading composer repositories with package information</info>");

        // creating repository set
        let policy = self.create_policy(true, locked_repository.as_ref());
        let mut repository_set = self.create_repository_set(true, &platform_repo, &aliases, None);
        let repositories = self.repository_manager.get_repositories();
        for repository in repositories {
            repository_set.add_repository(repository.clone_box())?;
        }
        if let Some(ref lr) = locked_repository {
            repository_set.add_repository(lr.clone_box())?;
        }

        let mut request = self.create_request(
            &*self.fixed_root_package,
            &platform_repo,
            locked_repository.as_ref(),
        );
        self.require_packages_for_update(&mut request, locked_repository.as_ref(), true)?;

        // pass the allow list into the request, so the pool builder can apply it
        if let Some(ref allow_list) = self.update_allow_list {
            // TODO(phase-b): convert i64 self.update_allow_transitive_dependencies into the enum
            let _ = allow_list;
        }

        // TODO(phase-b): create_pool takes owned Request, Box<dyn IOInterface>, Option<Rc<...>>
        // but locally we only have refs. PHP classes (IO, dispatcher) shouldn't Clone.
        let mut pool: Option<Pool> = {
            let _ = (&request, &self.event_dispatcher, &policy, &repository_set);
            todo!()
        };

        self.io.write_error("<info>Updating dependencies</info>");

        // solve dependencies
        // TODO(phase-b): Solver::new takes owned policy/pool/io; refactor needed
        let mut solver: Option<Solver> = {
            let _ = (&policy, pool.as_ref(), &self.io);
            todo!()
        };
        let mut lock_transaction: LockTransaction;
        let rule_set_size;
        match solver
            .as_mut()
            .unwrap()
            .solve(&request, Some(self.platform_requirement_filter.clone_box()))
        {
            Ok(t) => {
                lock_transaction = t;
                rule_set_size = solver.as_ref().unwrap().get_rule_set_size();
                solver = None;
            }
            Err(e) => {
                // TODO(phase-b): SolverProblemsException contains dyn Rule which isn't Send+Sync
                // so anyhow::Error::downcast_ref can't extract it. Skipping detection.
                let _ = (&repository_set, &request, pool.as_ref());
                return Err(e);
            }
        }
        let _ = solver;

        self.io.write_error3(
            &format!(
                "Analyzed {} packages to resolve dependencies",
                pool.as_ref().unwrap().get_packages().len()
            ),
            true,
            io_interface::VERBOSE,
        );
        self.io.write_error3(
            &format!("Analyzed {} rules to resolve dependencies", rule_set_size),
            true,
            io_interface::VERBOSE,
        );

        pool = None;
        let _ = pool;

        if lock_transaction.get_operations().is_empty() {
            self.io.write_error("Nothing to modify in lock file");

            if self.minimal_update
                && self.update_allow_list.is_none()
                && self.locker.is_fresh().unwrap_or(false)
            {
                self.io.write_error("<warning>The --minimal-changes option should be used with package arguments or after modifying composer.json requirements, otherwise it will likely not yield any dependency changes.</warning>");
            }
        }

        let exit_code = self.extract_dev_packages(
            &mut lock_transaction,
            &platform_repo,
            &aliases,
            &policy,
            locked_repository.as_ref(),
        )?;
        if exit_code != 0 {
            return Ok(exit_code);
        }

        shirabe_semver::compiling_matcher::CompilingMatcher::clear();

        // write lock
        let platform_reqs = self.extract_platform_requirements(&self.package.get_requires());
        let platform_dev_reqs =
            self.extract_platform_requirements(&self.package.get_dev_requires());

        let mut installs_updates: Vec<Box<dyn OperationInterface>> = vec![];
        let mut uninstalls: Vec<Box<dyn OperationInterface>> = vec![];
        if !lock_transaction.get_operations().is_empty() {
            let mut install_names: Vec<String> = vec![];
            let mut update_names: Vec<String> = vec![];
            let mut uninstall_names: Vec<String> = vec![];
            for operation in lock_transaction.get_operations() {
                if let Some(io) = operation.as_install_operation() {
                    installs_updates.push(operation.clone_box());
                    install_names.push(format!(
                        "{}:{}",
                        io.get_package().get_pretty_name(),
                        io.get_package().get_full_pretty_version(true, 0)
                    ));
                } else if let Some(uo) = operation.as_update_operation() {
                    // when mirrors/metadata from a package gets updated we do not want to list it as an
                    // update in the output as it is only an internal lock file metadata update
                    if self.update_mirrors
                        && uo.get_initial_package().get_name() == uo.get_target_package().get_name()
                        && uo.get_initial_package().get_version()
                            == uo.get_target_package().get_version()
                    {
                        continue;
                    }

                    installs_updates.push(operation.clone_box());
                    update_names.push(format!(
                        "{}:{}",
                        uo.get_target_package().get_pretty_name(),
                        uo.get_target_package().get_full_pretty_version(true, 0)
                    ));
                } else if let Some(uo) = operation.as_uninstall_operation() {
                    uninstalls.push(operation.clone_box());
                    uninstall_names.push(uo.get_package().get_pretty_name().to_string());
                }
            }

            if self
                .config
                .borrow_mut()
                .get("lock")
                .as_bool()
                .unwrap_or(false)
            {
                self.io.write_error(&sprintf(
                    "<info>Lock file operations: %d install%s, %d update%s, %d removal%s</info>",
                    &[
                        (install_names.len() as i64).into(),
                        (if 1 == install_names.len() { "" } else { "s" }).into(),
                        (update_names.len() as i64).into(),
                        (if 1 == update_names.len() { "" } else { "s" }).into(),
                        (uninstalls.len() as i64).into(),
                        (if 1 == uninstalls.len() { "" } else { "s" }).into(),
                    ],
                ));
                if !install_names.is_empty() {
                    self.io.write_error3(
                        &format!("Installs: {}", implode(", ", &install_names)),
                        true,
                        io_interface::VERBOSE,
                    );
                }
                if !update_names.is_empty() {
                    self.io.write_error3(
                        &format!("Updates: {}", implode(", ", &update_names)),
                        true,
                        io_interface::VERBOSE,
                    );
                }
                if !uninstalls.is_empty() {
                    self.io.write_error3(
                        &format!("Removals: {}", implode(", ", &uninstall_names)),
                        true,
                        io_interface::VERBOSE,
                    );
                }
            }
        }

        let sort_by_name =
            |a: &Box<dyn OperationInterface>, b: &Box<dyn OperationInterface>| -> i64 {
                let a_name: String = if let Some(uo) = a.as_update_operation() {
                    uo.get_target_package().get_name().to_string()
                } else {
                    a.get_package().get_name().to_string()
                };
                let b_name: String = if let Some(uo) = b.as_update_operation() {
                    uo.get_target_package().get_name().to_string()
                } else {
                    b.get_package().get_name().to_string()
                };
                strcmp(&a_name, &b_name)
            };
        usort(&mut uninstalls, &sort_by_name);
        usort(&mut installs_updates, &sort_by_name);

        let mut merged: Vec<Box<dyn OperationInterface>> = uninstalls;
        merged.extend(installs_updates);
        for operation in &merged {
            // collect suggestions
            if let Some(io) = operation.as_install_operation() {
                self.suggested_packages_reporter
                    .add_suggestions_from_package(&*io.get_package());
            }

            // output op if lock file is enabled, but alias op only in debug verbosity
            if self
                .config
                .borrow_mut()
                .get("lock")
                .as_bool()
                .unwrap_or(false)
                && (strpos(&operation.get_operation_type(), "Alias").is_none()
                    || self.io.is_debug())
            {
                let mut source_repo = String::new();
                if self.io.is_very_verbose()
                    && strpos(&operation.get_operation_type(), "Alias").is_none()
                {
                    let operation_pkg: Box<dyn PackageInterface> =
                        if let Some(uo) = operation.as_update_operation() {
                            uo.get_target_package().clone_package_box()
                        } else {
                            operation.get_package().clone_package_box()
                        };
                    if let Some(repo) = operation_pkg.get_repository() {
                        source_repo = format!(" from {}", repo.get_repo_name());
                    }
                }
                self.io
                    .write_error(&format!("  - {}{}", operation.show(true), source_repo));
            }
        }

        // Convert aliases (Vec<IndexMap<String, String>>) into Vec<IndexMap<String, PhpMixed>>
        let aliases_php_mixed: Vec<IndexMap<String, PhpMixed>> = lock_transaction
            .get_aliases(aliases.clone())
            .into_iter()
            .map(|m| {
                m.into_iter()
                    .map(|(k, v)| (k, PhpMixed::String(v)))
                    .collect::<IndexMap<String, PhpMixed>>()
            })
            .collect();
        let platform_overrides: IndexMap<String, PhpMixed> = self
            .config
            .borrow_mut()
            .get("platform")
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, *v))
            .collect();
        let updated_lock = self.locker.set_lock_data(
            lock_transaction.get_new_lock_packages(false, self.update_mirrors),
            Some(lock_transaction.get_new_lock_packages(true, self.update_mirrors)),
            platform_reqs,
            platform_dev_reqs,
            aliases_php_mixed,
            self.package.get_minimum_stability(),
            self.package.get_stability_flags().clone(),
            self.prefer_stable || self.package.get_prefer_stable(),
            self.prefer_lowest,
            platform_overrides,
            self.write_lock && self.execute_operations,
        )?;
        if updated_lock && self.write_lock && self.execute_operations {
            self.io.write_error("<info>Writing lock file</info>");
        }

        if do_install {
            // TODO ensure lock is used from locker as-is, since it may not have been written to disk in case of executeOperations == false
            return self.do_install(local_repo, true);
        }

        Ok(0)
    }

    /// Run the solver a second time on top of the existing update result with only the current result set in the pool
    /// and see what packages would get removed if we only had the non-dev packages in the solver request
    pub(crate) fn extract_dev_packages(
        &mut self,
        lock_transaction: &mut LockTransaction,
        platform_repo: &PlatformRepository,
        aliases: &Vec<IndexMap<String, String>>,
        policy: &dyn PolicyInterface,
        locked_repository: Option<&LockArrayRepository>,
    ) -> anyhow::Result<i64> {
        if self.package.get_dev_requires().is_empty() {
            return Ok(0);
        }

        let mut result_repo = ArrayRepository::new(vec![])?;
        let loader = ArrayLoader::new(None, true);
        let dumper = ArrayDumper::new();
        for pkg in lock_transaction.get_new_lock_packages(false, false) {
            let loaded = loader.load(
                dumper.dump(&*pkg),
                Some("Composer\\Package\\CompletePackage".to_string()),
            )?;
            result_repo.add_package(loaded.clone_package_box())?;
        }

        let mut repository_set = self.create_repository_set(true, platform_repo, aliases, None);
        repository_set.add_repository(Box::new(result_repo))?;

        let mut request = self.create_request(&*self.fixed_root_package, platform_repo, None);
        self.require_packages_for_update(&mut request, locked_repository, false)?;

        let pool = repository_set.create_pool_with_all_packages()?;

        // TODO(phase-b): Solver::new takes owned policy/pool/io; refactor needed
        let mut solver: Option<Solver> = {
            let _ = (policy, &pool, &self.io);
            todo!()
        };
        let non_dev_lock_transaction: LockTransaction;
        match solver
            .as_mut()
            .unwrap()
            .solve(&request, Some(self.platform_requirement_filter.clone_box()))
        {
            Ok(t) => {
                non_dev_lock_transaction = t;
                solver = None;
            }
            Err(e) => {
                // TODO(phase-b): SolverProblemsException can't be downcast (dyn Rule not Send+Sync)
                let _ = (&repository_set, &request, &pool);
                return Err(e);
            }
        }
        let _ = solver;

        lock_transaction.set_non_dev_packages(&non_dev_lock_transaction);

        Ok(0)
    }

    /// Whether the function is called as part of an update command or independently
    pub(crate) fn do_install(
        &mut self,
        mut local_repo: Box<dyn InstalledRepositoryInterface>,
        already_solved: bool,
    ) -> anyhow::Result<i64> {
        if self
            .config
            .borrow_mut()
            .get("lock")
            .as_bool()
            .unwrap_or(false)
        {
            self.io.write_error(&format!(
                "<info>Installing dependencies from lock file{}</info>",
                if self.dev_mode {
                    " (including require-dev)"
                } else {
                    ""
                }
            ));
        }

        let locked_repository = self.locker.get_locked_repository(self.dev_mode)?;

        // verify that the lock file works with the current platform repository
        // we can skip this part if we're doing this as the second step after an update
        if !already_solved {
            self.io.write_error(
                "<info>Verifying lock file contents can be installed on current platform.</info>",
            );

            let platform_repo = self.create_platform_repo(false);
            // creating repository set
            let policy = self.create_policy(false, None);
            // use aliases from lock file only, so empty root aliases here
            let mut repository_set = self.create_repository_set(
                false,
                &platform_repo,
                &vec![],
                Some(&locked_repository),
            );
            repository_set.add_repository(locked_repository.clone_box())?;

            // creating requirements request
            let mut request = self.create_request(
                &*self.fixed_root_package,
                &platform_repo,
                Some(&locked_repository),
            );

            if !self.locker.is_fresh()? {
                self.io.write_error3(
                    "<warning>Warning: The lock file is not up to date with the latest changes in composer.json. You may be getting outdated dependencies. It is recommended that you run `composer update` or `composer update <package name>`.</warning>",
                    true,
                    io_interface::QUIET,
                );
            }

            let missing_requirement_info = self
                .locker
                .get_missing_requirement_info(&*self.package, self.dev_mode)?;
            if !missing_requirement_info.is_empty() {
                self.io.write_error(&missing_requirement_info.join("\n"));

                if !self
                    .config
                    .borrow_mut()
                    .get("allow-missing-requirements")
                    .as_bool()
                    .unwrap_or(false)
                {
                    return Ok(Self::ERROR_LOCK_FILE_INVALID);
                }
            }

            for package in RepositoryInterface::get_packages(&locked_repository) {
                request.fix_locked_package(package);
            }

            let mut root_requires = self.package.get_requires();
            if self.dev_mode {
                for (k, v) in self.package.get_dev_requires() {
                    root_requires.insert(k, v);
                }
            }
            for (_key, link) in &root_requires {
                if PlatformRepository::is_platform_package(link.get_target()) {
                    request
                        .require_name(link.get_target(), Some(link.get_constraint().clone_box()))?;
                }
            }

            for link in self.locker.get_platform_requirements(self.dev_mode)? {
                if !root_requires.contains_key(link.get_target()) {
                    request
                        .require_name(link.get_target(), Some(link.get_constraint().clone_box()))?;
                }
            }
            drop(root_requires);

            // TODO(phase-b): create_pool takes owned Request, Box<dyn IOInterface>, Option<Rc<...>>
            let pool: Pool = {
                let _ = (&request, &self.io, &self.event_dispatcher, &repository_set);
                todo!()
            };

            // solve dependencies
            // TODO(phase-b): Solver::new takes owned policy/pool/io
            let mut solver: Option<Solver> = {
                let _ = (&policy, &pool, &self.io);
                todo!()
            };
            match solver
                .as_mut()
                .unwrap()
                .solve(&request, Some(self.platform_requirement_filter.clone_box()))
            {
                Ok(lock_transaction) => {
                    solver = None;

                    // installing the locked packages on this platform resulted in lock modifying operations, there wasn't a conflict, but the lock file as-is seems to not work on this system
                    if 0 != lock_transaction.get_operations().len() {
                        self.io.write_error3(
                            "<error>Your lock file cannot be installed on this system without changes. Please run composer update.</error>",
                            true,
                            io_interface::QUIET,
                        );

                        return Ok(Self::ERROR_LOCK_FILE_INVALID);
                    }
                }
                Err(e) => {
                    // TODO(phase-b): SolverProblemsException can't be downcast (dyn Rule not Send+Sync)
                    let _ = (&repository_set, &request, &pool);
                    return Err(e);
                }
            }
            let _ = solver;
        }

        // TODO in how far do we need to do anything here to ensure dev packages being updated to latest in lock without version change are treated correctly?
        let local_repo_transaction = LocalRepoTransaction::new(&locked_repository, &*local_repo);
        // TODO(phase-b): dispatch_installer_event takes owned Transaction, not &LocalRepoTransaction
        // self.event_dispatcher.borrow_mut().dispatch_installer_event(
        //     InstallerEvents::PRE_OPERATIONS_EXEC,
        //     self.dev_mode,
        //     self.execute_operations,
        //     &local_repo_transaction,
        // );

        let mut installs: Vec<String> = vec![];
        let mut updates: Vec<String> = vec![];
        let mut uninstalls: Vec<String> = vec![];
        for operation in local_repo_transaction.get_operations() {
            if let Some(io) = operation.as_install_operation() {
                installs.push(format!(
                    "{}:{}",
                    io.get_package().get_pretty_name(),
                    io.get_package().get_full_pretty_version(true, 0)
                ));
            } else if let Some(uo) = operation.as_update_operation() {
                updates.push(format!(
                    "{}:{}",
                    uo.get_target_package().get_pretty_name(),
                    uo.get_target_package().get_full_pretty_version(true, 0)
                ));
            } else if let Some(uo) = operation.as_uninstall_operation() {
                uninstalls.push(uo.get_package().get_pretty_name().to_string());
            }
        }

        if installs.is_empty() && updates.is_empty() && uninstalls.is_empty() {
            self.io.write_error("Nothing to install, update or remove");
        } else {
            self.io.write_error(&sprintf(
                "<info>Package operations: %d install%s, %d update%s, %d removal%s</info>",
                &[
                    (installs.len() as i64).into(),
                    (if 1 == installs.len() { "" } else { "s" }).into(),
                    (updates.len() as i64).into(),
                    (if 1 == updates.len() { "" } else { "s" }).into(),
                    (uninstalls.len() as i64).into(),
                    (if 1 == uninstalls.len() { "" } else { "s" }).into(),
                ],
            ));
            if !installs.is_empty() {
                self.io.write_error3(
                    &format!("Installs: {}", implode(", ", &installs)),
                    true,
                    io_interface::VERBOSE,
                );
            }
            if !updates.is_empty() {
                self.io.write_error3(
                    &format!("Updates: {}", implode(", ", &updates)),
                    true,
                    io_interface::VERBOSE,
                );
            }
            if !uninstalls.is_empty() {
                self.io.write_error3(
                    &format!("Removals: {}", implode(", ", &uninstalls)),
                    true,
                    io_interface::VERBOSE,
                );
            }
        }

        if self.execute_operations {
            local_repo.set_dev_package_names(self.locker.get_dev_package_names()?);
            self.installation_manager.execute(
                &mut *local_repo,
                local_repo_transaction.get_operations(),
                self.dev_mode,
                self.run_scripts,
                self.download_only,
            )?;

            // see https://github.com/composer/composer/issues/2764
            if local_repo_transaction.get_operations().len() > 0 {
                let vendor_dir = self
                    .config
                    .borrow_mut()
                    .get("vendor-dir")
                    .as_string()
                    .unwrap_or("")
                    .to_string();
                if is_dir(&vendor_dir) {
                    // suppress errors as this fails sometimes on OSX for no apparent reason
                    // see https://github.com/composer/composer/issues/4070#issuecomment-129792748
                    let _ = touch(&vendor_dir);
                }
            }
        } else {
            for operation in local_repo_transaction.get_operations() {
                // output op, but alias op only in debug verbosity
                if strpos(&operation.get_operation_type(), "Alias").is_none() || self.io.is_debug()
                {
                    self.io
                        .write_error(&format!("  - {}", operation.show(false)));
                }
            }
        }

        Ok(0)
    }

    pub(crate) fn create_platform_repo(&mut self, for_update: bool) -> PlatformRepository {
        let platform_overrides: IndexMap<String, PhpMixed> = if for_update {
            self.config
                .borrow_mut()
                .get("platform")
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, *v))
                .collect()
        } else {
            self.locker
                .get_platform_overrides()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, PhpMixed::String(v)))
                .collect()
        };

        // TODO(phase-b): PlatformRepository::new returns Result, propagate
        PlatformRepository::new(vec![], platform_overrides)
            .expect("PlatformRepository::new should not fail")
    }

    fn create_repository_set(
        &mut self,
        for_update: bool,
        platform_repo: &PlatformRepository,
        root_aliases: &Vec<IndexMap<String, String>>,
        locked_repository: Option<&dyn RepositoryInterface>,
    ) -> RepositorySet {
        let minimum_stability: String;
        let mut stability_flags: IndexMap<String, i64>;

        let requires: IndexMap<String, Box<dyn ConstraintInterface>>;
        if for_update {
            minimum_stability = self.package.get_minimum_stability().to_string();
            stability_flags = self.package.get_stability_flags().clone();

            // Convert Link map merge into ConstraintInterface map for use later
            let mut req_links: IndexMap<String, Link> = IndexMap::new();
            for (k, v) in self.package.get_requires() {
                req_links.insert(k, v);
            }
            for (k, v) in self.package.get_dev_requires() {
                req_links.insert(k, v);
            }
            // Translate to constraint map for downstream uniform handling.
            let mut tmp: IndexMap<String, Box<dyn ConstraintInterface>> = IndexMap::new();
            for (k, link) in req_links {
                tmp.insert(k, link.get_constraint().clone_box());
            }
            requires = tmp;
        } else {
            minimum_stability = self
                .locker
                .get_minimum_stability()
                .unwrap_or_else(|_| String::new());
            // TODO(phase-b): locker.get_stability_flags returns IndexMap<String, String>; convert to i64
            stability_flags = self
                .locker
                .get_stability_flags()
                .map(|m| {
                    m.into_iter()
                        .map(|(k, v)| (k, v.parse::<i64>().unwrap_or(0)))
                        .collect()
                })
                .unwrap_or_default();

            let mut tmp: IndexMap<String, Box<dyn ConstraintInterface>> = IndexMap::new();
            for package in locked_repository.unwrap().get_packages() {
                let mut constraint = Constraint::new("=", package.get_version().to_string());
                constraint.set_pretty_string(Some(package.get_pretty_version().to_string()));
                tmp.insert(package.get_name().to_string(), Box::new(constraint));
            }
            requires = tmp;
        }

        let mut root_requires: IndexMap<String, Box<dyn ConstraintInterface>> = IndexMap::new();
        for (req, mut constraint) in requires {
            // skip platform requirements from the root package to avoid filtering out existing platform packages
            if self.platform_requirement_filter.is_ignored(&req) {
                continue;
            } else if let Some(filter) = self
                .platform_requirement_filter
                .as_any()
                .downcast_ref::<IgnoreListPlatformRequirementFilter>()
            {
                constraint = filter
                    .filter_constraint(&req, constraint, false)
                    .unwrap_or_else(|_| Box::new(Constraint::new("=", String::new())));
            }
            root_requires.insert(req, constraint);
        }

        // TODO(phase-b): self.package is Box<dyn RootPackageInterface>; cannot clone a trait
        // object without Clone. PHP shares the reference. Skipping fixed_root_package assignment.
        // self.fixed_root_package = clone(&self.package);
        self.fixed_root_package.set_requires(vec![]);
        self.fixed_root_package.set_dev_requires(vec![]);

        stability_flags.insert(
            self.package.get_name().to_string(),
            base_package::STABILITIES
                [VersionParser::parse_stability(self.package.get_version()).as_str()],
        );

        // TODO(phase-b): convert root_aliases (Vec<IndexMap<String, String>>) into Vec<RootAliasInput>
        let root_aliases_input: Vec<crate::repository::RootAliasInput> = vec![];
        let _ = root_aliases;
        // TODO(phase-b): temporary_constraints holds Box<dyn ConstraintInterface> which can't Clone
        let temporary_constraints: IndexMap<String, Box<dyn ConstraintInterface>> = IndexMap::new();
        let mut repository_set = RepositorySet::new(
            &minimum_stability,
            stability_flags,
            root_aliases_input,
            self.package.get_references().clone(),
            root_requires,
            temporary_constraints,
        );
        // TODO(phase-b): RootPackageRepository::new takes owned root package
        // repository_set.add_repository(Box::new(RootPackageRepository::new(clone(
        //     &self.fixed_root_package,
        // ))));
        let _ = platform_repo;
        // TODO(phase-b): PlatformRepository has no Clone impl (PHP class)
        // repository_set.add_repository(Box::new(platform_repo.clone()));
        if let Some(ref additional_fixed_repository) = self.additional_fixed_repository {
            // allow using installed repos if needed to avoid warnings about installed repositories being used in the RepositorySet
            // see https://github.com/composer/composer/pull/9574
            let additional_fixed_repositories: Vec<Box<dyn RepositoryInterface>> =
                if let Some(composite) = additional_fixed_repository
                    .as_any()
                    .downcast_ref::<CompositeRepository>()
                {
                    composite
                        .get_repositories()
                        .iter()
                        .map(|r| r.clone_box())
                        .collect()
                } else {
                    vec![additional_fixed_repository.clone_box()]
                };
            for additional_fixed_repository in &additional_fixed_repositories {
                // TODO(phase-b): as_installed_repository_interface not on RepositoryInterface trait
                if additional_fixed_repository
                    .as_any()
                    .downcast_ref::<InstalledRepository>()
                    .is_some()
                {
                    repository_set.allow_installed_repositories(true);
                    break;
                }
            }

            let _ = repository_set.add_repository(additional_fixed_repository.clone_box());
        }

        repository_set
    }

    fn create_policy(
        &mut self,
        for_update: bool,
        locked_repo: Option<&LockArrayRepository>,
    ) -> DefaultPolicy {
        let mut prefer_stable: Option<bool> = None;
        let mut prefer_lowest: Option<bool> = None;
        if !for_update {
            prefer_stable = self.locker.get_prefer_stable().unwrap_or(None);
            prefer_lowest = self.locker.get_prefer_lowest().unwrap_or(None);
        }
        // old lock file without prefer stable/lowest will return null
        // so in this case we use the composer.json info
        if prefer_stable.is_none() {
            prefer_stable = Some(self.prefer_stable || self.package.get_prefer_stable());
        }
        if prefer_lowest.is_none() {
            prefer_lowest = Some(self.prefer_lowest);
        }

        let mut preferred_versions: Option<IndexMap<String, String>> = None;
        if for_update && self.minimal_update && locked_repo.is_some() {
            let mut versions: IndexMap<String, String> = IndexMap::new();
            for pkg in CanonicalPackagesTrait::get_packages(locked_repo.unwrap()) {
                if pkg.as_alias_package().is_some()
                    || (self.update_allow_list.is_some()
                        && self
                            .update_allow_list
                            .as_ref()
                            .unwrap()
                            .iter()
                            .any(|s| s == pkg.get_name()))
                {
                    continue;
                }
                versions.insert(pkg.get_name().to_string(), pkg.get_version().to_string());
            }
            preferred_versions = Some(versions);
        }

        DefaultPolicy::new(
            prefer_stable.unwrap(),
            prefer_lowest.unwrap(),
            preferred_versions,
        )
    }

    fn create_request(
        &self,
        root_package: &dyn RootPackageInterface,
        platform_repo: &PlatformRepository,
        locked_repository: Option<&LockArrayRepository>,
    ) -> Request {
        // TODO(phase-b): Request::new takes Option<LockArrayRepository> (owned). PHP class
        // shouldn't Clone. Passing None for now.
        let _ = locked_repository;
        let mut request = Request::new(None);

        // TODO(phase-b): request.fix_package wants Box<dyn BasePackage>; root_package is &dyn RootPackageInterface
        let _ = root_package;
        // request.fix_package(root_package);
        if let Some(_alias) = root_package.as_any().downcast_ref::<RootAliasPackage>() {
            // request.fix_package(alias.get_alias_of());
        }

        let mut fixed_packages = platform_repo.get_packages();
        if let Some(ref additional_fixed_repository) = self.additional_fixed_repository {
            fixed_packages.extend(additional_fixed_repository.get_packages());
        }

        // fix the version of all platform packages + additionally installed packages
        // to prevent the solver trying to remove or update those
        // TODO why not replaces?
        let provided = root_package.get_provides();
        for package in fixed_packages {
            // skip platform packages that are provided by the root package
            let pkg_repo_is_platform = match package.get_repository() {
                Some(r) => std::ptr::eq(
                    r.as_any() as *const _ as *const u8,
                    platform_repo.as_any() as *const _ as *const u8,
                ),
                None => false,
            };
            if !pkg_repo_is_platform
                || !provided.contains_key(package.get_name())
                || !provided
                    .get(package.get_name())
                    .unwrap()
                    .get_constraint()
                    .matches(&Constraint::new("=", package.get_version().to_string()))
            {
                // TODO(phase-b): fix_package needs owned Box<dyn BasePackage>
                let _ = &package;
                // request.fix_package(&*package);
            }
        }

        request
    }

    fn require_packages_for_update(
        &mut self,
        request: &mut Request,
        locked_repository: Option<&LockArrayRepository>,
        include_dev_requires: bool,
    ) -> anyhow::Result<()> {
        // if we're updating mirrors we want to keep exactly the same versions installed which are in the lock file, but we want current remote metadata
        if self.update_mirrors {
            let excluded_packages: IndexMap<String, i64> = if !include_dev_requires {
                // TODO(phase-b): locker.get_dev_package_names returns Result<Vec<String>>
                let names = self.locker.get_dev_package_names().unwrap_or_default();
                names
                    .into_iter()
                    .enumerate()
                    .map(|(i, name)| (name, i as i64))
                    .collect()
            } else {
                IndexMap::new()
            };

            for locked_package in CanonicalPackagesTrait::get_packages(locked_repository.unwrap()) {
                // exclude alias packages here as for root aliases, both alias and aliased are
                // present in the lock repo and we only want to require the aliased version
                if locked_package.as_alias_package().is_none()
                    && !excluded_packages.contains_key(locked_package.get_name())
                {
                    request.require_name(
                        locked_package.get_name(),
                        Some(Box::new(Constraint::new(
                            "==",
                            locked_package.get_version().to_string(),
                        ))),
                    )?;
                }
            }
        } else {
            let mut links = self.package.get_requires();
            if include_dev_requires {
                for (k, v) in self.package.get_dev_requires() {
                    links.insert(k, v);
                }
            }
            for (_key, link) in &links {
                request.require_name(link.get_target(), Some(link.get_constraint().clone_box()))?;
            }
        }
        Ok(())
    }

    fn get_root_aliases(&mut self, for_update: bool) -> Vec<IndexMap<String, String>> {
        if for_update {
            self.package.get_aliases().to_vec()
        } else {
            self.locker.get_aliases().unwrap_or_default()
        }
    }

    fn extract_platform_requirements(
        &self,
        links: &IndexMap<String, Link>,
    ) -> IndexMap<String, String> {
        let mut platform_reqs: IndexMap<String, String> = IndexMap::new();
        for (_key, link) in links {
            if PlatformRepository::is_platform_package(link.get_target()) {
                platform_reqs.insert(
                    link.get_target().to_string(),
                    link.get_pretty_constraint()
                        .map(|s| s.to_string())
                        .unwrap_or_default(),
                );
            }
        }

        platform_reqs
    }

    /// Replace local repositories with InstalledArrayRepository instances
    ///
    /// This is to prevent any accidental modification of the existing repos on disk
    fn mock_local_repositories(&self, rm: &mut RepositoryManager) {
        let mut packages: IndexMap<String, Box<dyn PackageInterface>> = IndexMap::new();
        for package in rm.get_local_repository().get_packages() {
            packages.insert(package.to_string(), package.clone_box());
        }
        let keys: Vec<String> = packages.keys().cloned().collect();
        for key in keys {
            let package_clone = packages.get(&key).unwrap().clone_package_box();
            if let Some(alias_pkg) = package_clone.as_alias_package() {
                let alias_key = alias_pkg.get_alias_of().to_string();
                // TODO(phase-b): get_class on dyn PackageInterface; skipped because PhpMixed shim only
                let _class_name = "Composer\\Package\\AliasPackage".to_string();
                // PHP: $packages[$key] = new $className($packages[$alias], $package->getVersion(), $package->getPrettyVersion());
                // TODO(phase-b): AliasPackage::new expects Box<dyn BasePackage>; have Box<dyn PackageInterface>
                let _aliased = packages.get(&alias_key).unwrap().clone_package_box();
                let new_alias_package: Box<dyn PackageInterface> = todo!();
                packages.insert(key, new_alias_package);
            }
        }
        rm.set_local_repository(Box::new(
            InstalledArrayRepository::new_with_packages(packages.into_values().collect())
                .expect("InstalledArrayRepository::new_with_packages should not fail"),
        ));
    }

    fn create_pool_optimizer(&self, policy: &dyn PolicyInterface) -> Option<PoolOptimizer> {
        // Not the best architectural decision here, would need to be able
        // to configure from the outside of Installer but this is only
        // a debugging tool and should never be required in any other use case
        if Platform::get_env("COMPOSER_POOL_OPTIMIZER").as_deref() == Some("0") {
            self.io.write3(
                "Pool Optimizer was disabled for debugging purposes.",
                true,
                io_interface::DEBUG,
            );

            return None;
        }

        // TODO(phase-b): PoolOptimizer::new takes owned Box<dyn PolicyInterface>; have &dyn
        let _ = policy;
        todo!()
    }

    fn get_audit_config(&mut self) -> anyhow::Result<&AuditConfig> {
        if self.audit_config.is_none() {
            self.audit_config = Some(AuditConfig::from_config(
                &mut *self.config.borrow_mut(),
                self.audit,
                &self.audit_format,
            )?);
        }

        Ok(self.audit_config.as_ref().unwrap())
    }

    fn create_security_audit_pool_filter(
        &mut self,
    ) -> anyhow::Result<Option<SecurityAdvisoryPoolFilter>> {
        let update_mirrors = self.update_mirrors;
        let audit_config = self.get_audit_config()?;

        if audit_config.block_insecure && !update_mirrors {
            return Ok(Some(SecurityAdvisoryPoolFilter::new(
                Auditor,
                audit_config.clone(),
            )));
        }

        Ok(None)
    }

    /// Create Installer
    pub fn create(io: Box<dyn IOInterface>, composer: &Composer) -> Self {
        // TODO(phase-b): Installer::new takes owned manager/locker/etc., but Composer holds them
        // by value without Clone (correct for PHP class semantics). Requires refactoring
        // Installer to hold &/Rc references or moving ownership out of Composer.
        let _ = (io, composer);
        todo!()
    }

    /// Packages of those types are ignored, by default php-ext and php-ext-zend are ignored
    pub fn set_ignored_types(&mut self, types: Vec<String>) -> &mut Self {
        self.ignored_types = types;

        self
    }

    /// Only packages of those types are allowed if set to non-null
    pub fn set_allowed_types(&mut self, types: Option<Vec<String>>) -> &mut Self {
        self.allowed_types = types;

        self
    }

    pub fn set_additional_fixed_repository(
        &mut self,
        additional_fixed_repository: Box<dyn RepositoryInterface>,
    ) -> &mut Self {
        self.additional_fixed_repository = Some(additional_fixed_repository);

        self
    }

    pub fn set_temporary_constraints(
        &mut self,
        constraints: IndexMap<String, Box<dyn ConstraintInterface>>,
    ) -> &mut Self {
        self.temporary_constraints = constraints;

        self
    }

    /// Whether to run in drymode or not
    pub fn set_dry_run(&mut self, dry_run: bool) -> &mut Self {
        self.dry_run = dry_run;

        self
    }

    /// Checks, if this is a dry run (simulation mode).
    pub fn is_dry_run(&self) -> bool {
        self.dry_run
    }

    /// Whether to download only or not.
    pub fn set_download_only(&mut self, download_only: bool) -> &mut Self {
        self.download_only = download_only;

        self
    }

    /// prefer source installation
    pub fn set_prefer_source(&mut self, prefer_source: bool) -> &mut Self {
        self.prefer_source = prefer_source;

        self
    }

    /// prefer dist installation
    pub fn set_prefer_dist(&mut self, prefer_dist: bool) -> &mut Self {
        self.prefer_dist = prefer_dist;

        self
    }

    /// Whether or not generated autoloader are optimized
    pub fn set_optimize_autoloader(&mut self, optimize_autoloader: bool) -> &mut Self {
        self.optimize_autoloader = optimize_autoloader;
        if !self.optimize_autoloader {
            // Force classMapAuthoritative off when not optimizing the
            // autoloader
            self.set_class_map_authoritative(false);
        }

        self
    }

    /// Whether or not generated autoloader considers the class map
    /// authoritative.
    pub fn set_class_map_authoritative(&mut self, class_map_authoritative: bool) -> &mut Self {
        self.class_map_authoritative = class_map_authoritative;
        if self.class_map_authoritative {
            // Force optimizeAutoloader when classmap is authoritative
            self.set_optimize_autoloader(true);
        }

        self
    }

    /// Whether or not generated autoloader considers APCu caching.
    pub fn set_apcu_autoloader(
        &mut self,
        apcu_autoloader: bool,
        apcu_autoloader_prefix: Option<String>,
    ) -> &mut Self {
        self.apcu_autoloader = apcu_autoloader;
        self.apcu_autoloader_prefix = apcu_autoloader_prefix;

        self
    }

    /// update packages
    pub fn set_update(&mut self, update: bool) -> &mut Self {
        self.update = update;

        self
    }

    /// Allows disabling the install step after an update
    pub fn set_install(&mut self, install: bool) -> &mut Self {
        self.install = install;

        self
    }

    /// enables dev packages
    pub fn set_dev_mode(&mut self, dev_mode: bool) -> &mut Self {
        self.dev_mode = dev_mode;

        self
    }

    /// set whether to run autoloader or not
    ///
    /// This is disabled implicitly when enabling dryRun
    pub fn set_dump_autoloader(&mut self, dump_autoloader: bool) -> &mut Self {
        self.dump_autoloader = dump_autoloader;

        self
    }

    /// set whether to run scripts or not
    ///
    /// This is disabled implicitly when enabling dryRun
    ///
    /// Deprecated: Use setRunScripts(false) on the EventDispatcher instance being injected instead
    pub fn set_run_scripts(&mut self, run_scripts: bool) -> &mut Self {
        self.run_scripts = run_scripts;

        self
    }

    /// set the config instance
    pub fn set_config(&mut self, config: std::rc::Rc<std::cell::RefCell<Config>>) -> &mut Self {
        self.config = config;

        self
    }

    /// run in verbose mode
    pub fn set_verbose(&mut self, verbose: bool) -> &mut Self {
        self.verbose = verbose;

        self
    }

    /// Checks, if running in verbose mode.
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }

    /// set ignore Platform Package requirements
    ///
    /// If this is set to true, all platform requirements are ignored
    /// If this is set to false, no platform requirements are ignored
    /// If this is set to string[], those packages will be ignored
    ///
    /// Deprecated: use setPlatformRequirementFilter instead
    pub fn set_ignore_platform_requirements(
        &mut self,
        ignore_platform_reqs: shirabe_php_shim::PhpMixed,
    ) -> anyhow::Result<&mut Self> {
        trigger_error(
            "Installer::setIgnorePlatformRequirements is deprecated since Composer 2.2, use setPlatformRequirementFilter instead.",
            shirabe_php_shim::E_USER_DEPRECATED,
        );

        Ok(self.set_platform_requirement_filter(
            PlatformRequirementFilterFactory::from_bool_or_list(ignore_platform_reqs)?,
        ))
    }

    pub fn set_platform_requirement_filter(
        &mut self,
        platform_requirement_filter: Box<dyn PlatformRequirementFilterInterface>,
    ) -> &mut Self {
        self.platform_requirement_filter = platform_requirement_filter;

        self
    }

    /// Update the lock file to the exact same versions and references but use current remote metadata like URLs and mirror info
    pub fn set_update_mirrors(&mut self, update_mirrors: bool) -> &mut Self {
        self.update_mirrors = update_mirrors;

        self
    }

    /// restrict the update operation to a few packages, all other packages
    /// that are already installed will be kept at their current version
    pub fn set_update_allow_list(&mut self, packages: Vec<String>) -> &mut Self {
        if packages.len() == 0 {
            self.update_allow_list = None;
        } else {
            let lowered: Vec<String> = array_map(|s: &String| strtolower(s), &packages);
            let unique: Vec<String> = array_unique(&lowered);
            self.update_allow_list = Some(unique);
        }

        self
    }

    /// Should dependencies of packages marked for update be updated?
    ///
    /// Depending on the chosen constant this will either only update the directly named packages, all transitive
    /// dependencies which are not root requirement or all transitive dependencies including root requirements
    pub fn set_update_allow_transitive_dependencies(
        &mut self,
        update_allow_transitive_dependencies: i64,
    ) -> anyhow::Result<&mut Self> {
        let valid = [
            Request::UPDATE_ONLY_LISTED,
            Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS_NO_ROOT_REQUIRE,
            Request::UPDATE_LISTED_WITH_TRANSITIVE_DEPS,
        ];
        if !valid.contains(&update_allow_transitive_dependencies) {
            return Err(RuntimeException {
                message: "Invalid value for updateAllowTransitiveDependencies supplied".to_string(),
                code: 0,
            }
            .into());
        }

        self.update_allow_transitive_dependencies = update_allow_transitive_dependencies;

        Ok(self)
    }

    /// Should packages be preferred in a stable version when updating?
    pub fn set_prefer_stable(&mut self, prefer_stable: bool) -> &mut Self {
        self.prefer_stable = prefer_stable;

        self
    }

    /// Should packages be preferred in a lowest version when updating?
    pub fn set_prefer_lowest(&mut self, prefer_lowest: bool) -> &mut Self {
        self.prefer_lowest = prefer_lowest;

        self
    }

    /// Only relevant for partial updates (with setUpdateAllowList), if this is enabled currently locked versions will be preferred for packages which are not in the allowlist
    ///
    /// This reduces the update to
    pub fn set_minimal_update(&mut self, minimal_update: bool) -> &mut Self {
        self.minimal_update = minimal_update;

        self
    }

    /// Should the lock file be updated when updating?
    ///
    /// This is disabled implicitly when enabling dryRun
    pub fn set_write_lock(&mut self, write_lock: bool) -> &mut Self {
        self.write_lock = write_lock;

        self
    }

    /// Should the operations (package install, update and removal) be executed on disk?
    ///
    /// This is disabled implicitly when enabling dryRun
    pub fn set_execute_operations(&mut self, execute_operations: bool) -> &mut Self {
        self.execute_operations = execute_operations;

        self
    }

    /// Should an audit be run after installation is complete?
    ///
    /// Deprecated: Use setAuditConfig instead of calling this
    pub fn set_audit(&mut self, audit: bool) -> &mut Self {
        self.audit = audit;
        self.audit_config = None; // Invalidate cached config

        self
    }

    /// Should exit with status code 5 on audit error
    pub fn set_error_on_audit(&mut self, error_on_audit: bool) -> &mut Self {
        self.error_on_audit = error_on_audit;

        self
    }

    /// What format should be used for audit output?
    ///
    /// Deprecated: Use setAuditConfig instead of calling this
    pub fn set_audit_format(&mut self, audit_format: String) -> &mut Self {
        self.audit_format = audit_format;
        self.audit_config = None; // Invalidate cached config

        self
    }

    /// Sets a custom AuditConfig to override the default configuration from Config
    pub fn set_audit_config(&mut self, audit_config: AuditConfig) -> &mut Self {
        self.audit_config = Some(audit_config);

        self
    }

    /// Disables plugins.
    ///
    /// Call this if you want to ensure that third-party code never gets
    /// executed. The default is to automatically install, and execute
    /// custom third-party installers.
    pub fn disable_plugins(&mut self) -> &mut Self {
        // TODO(plugin): plugin disabling is part of the plugin API
        self.installation_manager.disable_plugins();

        self
    }

    pub fn set_suggested_packages_reporter(
        &mut self,
        suggested_packages_reporter: SuggestedPackagesReporter,
    ) -> &mut Self {
        self.suggested_packages_reporter = suggested_packages_reporter;

        self
    }
}
