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
    usort,
};
use shirabe_semver;

use crate::advisory::AuditConfig;
use crate::advisory::Auditor;
use crate::autoload::AutoloadGenerator;
use crate::composer::PartialComposerHandle;
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
use crate::dependency_resolver::UpdateAllowTransitiveDeps;
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
use crate::io::IOInterfaceImmutable;
use crate::package::AliasPackageHandle;
use crate::package::CompleteAliasPackageHandle;
use crate::package::CompletePackage;
use crate::package::CompletePackageInterface;
use crate::package::Link;
use crate::package::Locker;
use crate::package::Package;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::package::RootPackageInterface;
use crate::package::RootPackageInterfaceHandle;
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
use crate::repository::PlatformRepository;
use crate::repository::PlatformRepositoryHandle;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryManager;
use crate::repository::RepositorySet;
use crate::repository::RootPackageRepository;
use crate::script::ScriptEvents;
use crate::util::Platform;
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;

#[derive(Debug)]
pub struct Installer {
    pub(crate) io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    pub(crate) config: std::rc::Rc<std::cell::RefCell<Config>>,
    pub(crate) package: RootPackageInterfaceHandle,
    // TODO can we get rid of the below and just use the package itself?
    pub(crate) fixed_root_package: RootPackageInterfaceHandle,
    pub(crate) download_manager: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
    pub(crate) repository_manager: std::rc::Rc<std::cell::RefCell<RepositoryManager>>,
    pub(crate) locker: std::rc::Rc<std::cell::RefCell<Locker>>,
    pub(crate) installation_manager: std::rc::Rc<std::cell::RefCell<InstallationManager>>,
    pub(crate) event_dispatcher: std::rc::Rc<std::cell::RefCell<EventDispatcher>>,
    pub(crate) autoload_generator: std::rc::Rc<std::cell::RefCell<AutoloadGenerator>>,
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
    pub(crate) update_allow_transitive_dependencies: UpdateAllowTransitiveDeps,
    pub(crate) suggested_packages_reporter:
        std::rc::Rc<std::cell::RefCell<SuggestedPackagesReporter>>,
    pub(crate) platform_requirement_filter: std::rc::Rc<dyn PlatformRequirementFilterInterface>,
    pub(crate) additional_fixed_repository: Option<crate::repository::RepositoryInterfaceHandle>,
    pub(crate) temporary_constraints: IndexMap<String, AnyConstraint>,
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
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        package: RootPackageInterfaceHandle,
        download_manager: std::rc::Rc<std::cell::RefCell<DownloadManager>>,
        repository_manager: std::rc::Rc<std::cell::RefCell<RepositoryManager>>,
        locker: std::rc::Rc<std::cell::RefCell<Locker>>,
        installation_manager: std::rc::Rc<std::cell::RefCell<InstallationManager>>,
        event_dispatcher: std::rc::Rc<std::cell::RefCell<EventDispatcher>>,
        autoload_generator: std::rc::Rc<std::cell::RefCell<AutoloadGenerator>>,
    ) -> Self {
        let suggested_packages_reporter = std::rc::Rc::new(std::cell::RefCell::new(
            SuggestedPackagesReporter::new(io.clone()),
        ));
        let platform_requirement_filter = PlatformRequirementFilterFactory::ignore_nothing();
        let write_lock = config.borrow_mut().get("lock").as_bool().unwrap_or(false);

        Self {
            io,
            config,
            package: package.clone(),
            fixed_root_package: package.clone(),
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
            update_allow_transitive_dependencies: UpdateAllowTransitiveDeps::UpdateOnlyListed,
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

        let is_fresh_install = self
            .repository_manager
            .borrow()
            .get_local_repository()
            .is_fresh();

        // Force update if there is no lock file present
        if !self.update && !self.locker.borrow_mut().is_locked() {
            self.io.write_error("<warning>No composer.lock file present. Updating dependencies to latest instead of installing from lock file. See https://getcomposer.org/install for more information.</warning>");
            self.update = true;
        }

        if self.dry_run {
            self.verbose = true;
            self.run_scripts = false;
            self.execute_operations = false;
            self.write_lock = false;
            self.dump_autoloader = false;
            let repository_manager = self.repository_manager.clone();
            self.mock_local_repositories(&mut repository_manager.borrow_mut())?;
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

        let local_repo = self.repository_manager.borrow().get_local_repository();

        let install = self.install;
        let res_result: anyhow::Result<i64> = if self.update {
            self.do_update(local_repo, install)
        } else {
            self.do_install(local_repo, false)
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
                    self.installation_manager
                        .borrow_mut()
                        .notify_installs(self.io.clone());
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
            self.installation_manager
                .borrow_mut()
                .notify_installs(self.io.clone());
        }

        if self.update {
            let locked_repository_handle: crate::repository::RepositoryInterfaceHandle = self
                .locker
                .borrow_mut()
                .get_locked_repository(self.dev_mode)?
                .into();
            let mut installed_repo = InstalledRepository::new(vec![
                locked_repository_handle,
                self.create_platform_repo(false)?.into(),
                crate::repository::RepositoryInterfaceHandle::new(RootPackageRepository::new(
                    RootPackageInterfaceHandle::dup(&self.package),
                )),
            ]);
            if is_fresh_install {
                self.suggested_packages_reporter
                    .borrow_mut()
                    .add_suggestions_from_package(self.package.clone().into());
            }
            self.suggested_packages_reporter
                .borrow()
                .output_minimalistic(Some(&mut installed_repo), None)?;
        }

        // Find abandoned packages and warn user
        let locked_repository = self.locker.borrow_mut().get_locked_repository(true)?;
        let packages = locked_repository.borrow_mut().get_canonical_packages()?;
        for package in packages {
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
                .borrow_mut()
                .set_class_map_authoritative(self.class_map_authoritative);
            self.autoload_generator
                .borrow_mut()
                .set_apcu(self.apcu_autoloader, self.apcu_autoloader_prefix.clone());
            self.autoload_generator
                .borrow_mut()
                .set_run_scripts(self.run_scripts);
            self.autoload_generator
                .borrow_mut()
                .set_platform_requirement_filter(self.platform_requirement_filter.clone());
            let local_repo_handle = self.repository_manager.borrow().get_local_repository();
            let mut local_repo_ref = local_repo_handle.borrow_mut();
            self.autoload_generator.borrow_mut().dump(
                &*self.config.borrow(),
                local_repo_ref
                    .as_installed_repository_interface_mut()
                    .unwrap(),
                self.package.clone(),
                &mut *self.installation_manager.borrow_mut(),
                "composer",
                self.optimize_autoloader,
                None,
                Some(&mut *self.locker.borrow_mut()),
                false,
            )?;
        }

        if self.install && self.execute_operations {
            // force binaries re-generation in case they are missing
            let repository_manager = self.repository_manager.clone();
            let repository_manager = repository_manager.borrow();
            for package in repository_manager.get_local_repository().get_packages()? {
                self.installation_manager
                    .borrow_mut()
                    .ensure_binaries_presence(package.clone());
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
            let repository_manager = self.repository_manager.clone();
            let repository_manager = repository_manager.borrow();
            for package in repository_manager.get_local_repository().get_packages()? {
                if let Some(cp) = package.as_complete() {
                    if package.as_alias().is_none() && !cp.get_funding().is_empty() {
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

        let audit_config = self.get_audit_config()?.clone();

        if audit_config.audit {
            let (packages, target) = if self.update && !self.install {
                (
                    locked_repository.borrow_mut().get_canonical_packages()?,
                    "locked",
                )
            } else {
                (
                    self.repository_manager
                        .borrow()
                        .get_local_repository()
                        .get_canonical_packages()?,
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
                let repository_manager = self.repository_manager.clone();
                let repository_manager = repository_manager.borrow();
                for repo in repository_manager.get_repositories() {
                    repo_set.add_repository(repo.clone())?;
                }

                let audit_result = auditor.audit(
                    &mut *self.io.borrow_mut(),
                    &repo_set,
                    packages,
                    &audit_config.audit_format,
                    true,
                    audit_config.ignore_list_for_audit.clone(),
                    &audit_config.audit_abandoned,
                    audit_config.ignore_severity_for_audit.clone(),
                    audit_config.ignore_unreachable,
                    audit_config.ignore_abandoned_for_audit.clone(),
                );
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
        local_repo: crate::repository::RepositoryInterfaceHandle,
        do_install: bool,
    ) -> anyhow::Result<i64> {
        let platform_repo = self.create_platform_repo(true)?;
        let aliases = self.get_root_aliases(true);

        let mut locked_repository: Option<crate::repository::LockArrayRepositoryHandle> = None;

        let mut try_load_locked = || -> anyhow::Result<
            Result<Option<crate::repository::LockArrayRepositoryHandle>, ParsingException>,
        > {
                if self.locker.borrow_mut().is_locked() {
                    match self.locker.borrow_mut().get_locked_repository(true) {
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
        let policy = self.create_policy(true, locked_repository.as_ref())?;
        let mut repository_set =
            self.create_repository_set(true, &platform_repo, &aliases, None)?;
        let repository_manager = self.repository_manager.clone();
        let repository_manager = repository_manager.borrow();
        let repositories = repository_manager.get_repositories();
        for repository in repositories {
            repository_set.add_repository(repository.clone())?;
        }
        if let Some(ref lr) = locked_repository {
            repository_set.add_repository(lr.clone().into())?;
        }

        let fixed_root_package = self.fixed_root_package.clone();
        let mut request = self.create_request(
            fixed_root_package,
            &platform_repo,
            locked_repository.as_ref(),
        )?;
        self.require_packages_for_update(&mut request, locked_repository.as_ref(), true)?;

        // pass the allow list into the request, so the pool builder can apply it
        if let Some(ref allow_list) = self.update_allow_list {
            request.set_update_allow_list(
                allow_list.clone(),
                self.update_allow_transitive_dependencies,
            );
        }

        let pool = std::rc::Rc::new(std::cell::RefCell::new(repository_set.create_pool(
            &mut request,
            self.io.clone(),
            Some(self.event_dispatcher.clone()),
            self.create_pool_optimizer(policy.clone()),
            self.ignored_types.clone(),
            self.allowed_types.clone(),
            self.create_security_audit_pool_filter()?,
        )?));

        self.io.write_error("<info>Updating dependencies</info>");

        // solve dependencies
        let mut solver: Option<Solver> =
            Some(Solver::new(policy.clone(), pool.clone(), self.io.clone()));
        let mut lock_transaction: LockTransaction;
        let rule_set_size;
        match solver
            .as_mut()
            .unwrap()
            .solve(&request, Some(self.platform_requirement_filter.clone()))
        {
            Ok(t) => {
                lock_transaction = t;
                rule_set_size = solver.as_ref().unwrap().get_rule_set_size();
                solver = None;
            }
            Err(e) => {
                // TODO(phase-c): SolverProblemsException contains dyn Rule which isn't Send+Sync
                // so anyhow::Error::downcast_ref can't extract it. Skipping detection until the
                // solver error path moves off anyhow (see solver.rs).
                let _ = (&repository_set, &request, &pool);
                return Err(e);
            }
        }
        let _ = solver;

        self.io.write_error3(
            &format!(
                "Analyzed {} packages to resolve dependencies",
                pool.borrow().get_packages().len()
            ),
            true,
            io_interface::VERBOSE,
        );
        self.io.write_error3(
            &format!("Analyzed {} rules to resolve dependencies", rule_set_size),
            true,
            io_interface::VERBOSE,
        );

        drop(pool);

        if lock_transaction.get_operations().is_empty() {
            self.io.write_error("Nothing to modify in lock file");

            if self.minimal_update
                && self.update_allow_list.is_none()
                && self.locker.borrow_mut().is_fresh().unwrap_or(false)
            {
                self.io.write_error("<warning>The --minimal-changes option should be used with package arguments or after modifying composer.json requirements, otherwise it will likely not yield any dependency changes.</warning>");
            }
        }

        let exit_code = self.extract_dev_packages(
            &mut lock_transaction,
            &platform_repo,
            &aliases,
            policy.clone(),
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

        let mut installs_updates: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];
        let mut uninstalls: Vec<std::rc::Rc<dyn OperationInterface>> = vec![];
        if !lock_transaction.get_operations().is_empty() {
            let mut install_names: Vec<String> = vec![];
            let mut update_names: Vec<String> = vec![];
            let mut uninstall_names: Vec<String> = vec![];
            for operation in lock_transaction.get_operations() {
                if let Some(io) = operation.as_install_operation() {
                    installs_updates.push(operation.clone());
                    install_names.push(format!(
                        "{}:{}",
                        io.get_package().get_pretty_name(),
                        io.get_package().get_full_pretty_version(
                            true,
                            crate::package::DisplayMode::SourceRefIfDev
                        )
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

                    installs_updates.push(operation.clone());
                    update_names.push(format!(
                        "{}:{}",
                        uo.get_target_package().get_pretty_name(),
                        uo.get_target_package().get_full_pretty_version(
                            true,
                            crate::package::DisplayMode::SourceRefIfDev
                        )
                    ));
                } else if let Some(uo) = operation.as_uninstall_operation() {
                    uninstalls.push(operation.clone());
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

        let sort_by_name = |a: &std::rc::Rc<dyn OperationInterface>,
                            b: &std::rc::Rc<dyn OperationInterface>|
         -> i64 {
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

        let mut merged: Vec<std::rc::Rc<dyn OperationInterface>> = uninstalls;
        merged.extend(installs_updates);
        for operation in &merged {
            // collect suggestions
            if let Some(io) = operation.as_install_operation() {
                self.suggested_packages_reporter
                    .borrow_mut()
                    .add_suggestions_from_package(io.get_package());
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
                    let operation_pkg = match operation.as_update_operation() {
                        Some(uo) => uo.get_target_package(),
                        None => operation.get_package(),
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
        let updated_lock = self.locker.borrow_mut().set_lock_data(
            lock_transaction.get_new_lock_packages(false, self.update_mirrors),
            Some(lock_transaction.get_new_lock_packages(true, self.update_mirrors)),
            platform_reqs,
            platform_dev_reqs,
            aliases_php_mixed,
            &self.package.get_minimum_stability(),
            self.package.get_stability_flags(),
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
        platform_repo: &PlatformRepositoryHandle,
        aliases: &Vec<IndexMap<String, String>>,
        policy: std::rc::Rc<dyn PolicyInterface>,
        locked_repository: Option<&crate::repository::LockArrayRepositoryHandle>,
    ) -> anyhow::Result<i64> {
        if self.package.get_dev_requires().is_empty() {
            return Ok(0);
        }

        let mut result_repo = ArrayRepository::new(vec![])?;
        let loader = ArrayLoader::new(None, true);
        let dumper = ArrayDumper::new();
        for pkg in lock_transaction.get_new_lock_packages(false, false) {
            let loaded = loader.load(
                dumper.dump(pkg.clone()),
                Some("Composer\\Package\\CompletePackage".to_string()),
            )?;
            result_repo.add_package(loaded)?;
        }

        let mut repository_set = self.create_repository_set(true, platform_repo, aliases, None)?;
        repository_set.add_repository(crate::repository::RepositoryInterfaceHandle::new(
            result_repo,
        ))?;

        let mut request =
            self.create_request(self.fixed_root_package.clone(), platform_repo, None)?;
        self.require_packages_for_update(&mut request, locked_repository, false)?;

        let pool = std::rc::Rc::new(std::cell::RefCell::new(
            repository_set.create_pool_with_all_packages()?,
        ));

        let mut solver: Option<Solver> = Some(Solver::new(policy, pool.clone(), self.io.clone()));
        let non_dev_lock_transaction: LockTransaction;
        match solver
            .as_mut()
            .unwrap()
            .solve(&request, Some(self.platform_requirement_filter.clone()))
        {
            Ok(t) => {
                non_dev_lock_transaction = t;
                solver = None;
            }
            Err(e) => {
                // TODO(phase-c): SolverProblemsException can't be downcast (dyn Rule not Send+Sync); see solver.rs
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
        local_repo: crate::repository::RepositoryInterfaceHandle,
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

        let locked_repository = self
            .locker
            .borrow_mut()
            .get_locked_repository(self.dev_mode)?;

        // verify that the lock file works with the current platform repository
        // we can skip this part if we're doing this as the second step after an update
        if !already_solved {
            self.io.write_error(
                "<info>Verifying lock file contents can be installed on current platform.</info>",
            );

            let platform_repo = self.create_platform_repo(false)?;
            // creating repository set
            let policy = self.create_policy(false, None)?;
            // use aliases from lock file only, so empty root aliases here
            let mut locked_repo_borrow = locked_repository.borrow_mut();
            let mut repository_set = self.create_repository_set(
                false,
                &platform_repo,
                &vec![],
                Some(&mut *locked_repo_borrow),
            )?;
            drop(locked_repo_borrow);
            repository_set.add_repository(locked_repository.clone().into())?;

            // creating requirements request
            let fixed_root_package = self.fixed_root_package.clone();
            let mut request =
                self.create_request(fixed_root_package, &platform_repo, Some(&locked_repository))?;

            if !self.locker.borrow_mut().is_fresh()? {
                self.io.write_error3(
                    "<warning>Warning: The lock file is not up to date with the latest changes in composer.json. You may be getting outdated dependencies. It is recommended that you run `composer update` or `composer update <package name>`.</warning>",
                    true,
                    io_interface::QUIET,
                );
            }

            let package_for_missing = self.package.clone();
            let missing_requirement_info = self
                .locker
                .borrow_mut()
                .get_missing_requirement_info(package_for_missing, self.dev_mode)?;
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

            for package in RepositoryInterface::get_packages(&mut *locked_repository.borrow_mut())?
            {
                request.fix_locked_package(package.clone());
            }

            let mut root_requires = self.package.get_requires();
            if self.dev_mode {
                for (k, v) in self.package.get_dev_requires() {
                    root_requires.insert(k, v);
                }
            }
            for (_key, link) in &root_requires {
                if PlatformRepository::is_platform_package(link.get_target()) {
                    request.require_name(link.get_target(), Some(link.get_constraint().clone()))?;
                }
            }

            for link in self
                .locker
                .borrow_mut()
                .get_platform_requirements(self.dev_mode)?
            {
                if !root_requires.contains_key(link.get_target()) {
                    request.require_name(link.get_target(), Some(link.get_constraint().clone()))?;
                }
            }
            drop(root_requires);

            let pool = std::rc::Rc::new(std::cell::RefCell::new(repository_set.create_pool(
                &mut request,
                self.io.clone(),
                Some(self.event_dispatcher.clone()),
                None,
                self.ignored_types.clone(),
                self.allowed_types.clone(),
                None,
            )?));

            // solve dependencies
            let mut solver: Option<Solver> =
                Some(Solver::new(policy, pool.clone(), self.io.clone()));
            match solver
                .as_mut()
                .unwrap()
                .solve(&request, Some(self.platform_requirement_filter.clone()))
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
                    // TODO(phase-c): SolverProblemsException can't be downcast (dyn Rule not Send+Sync); see solver.rs
                    let _ = (&repository_set, &request, &pool);
                    return Err(e);
                }
            }
            let _ = solver;
        }

        // TODO in how far do we need to do anything here to ensure dev packages being updated to latest in lock without version change are treated correctly?
        let local_repo_transaction = {
            let mut local_repo_ref = local_repo.borrow_mut();
            let mut locked_repo_ref = locked_repository.borrow_mut();
            LocalRepoTransaction::new(
                &mut *locked_repo_ref,
                local_repo_ref
                    .as_installed_repository_interface_mut()
                    .unwrap(),
            )?
        };
        self.event_dispatcher
            .borrow_mut()
            .dispatch_installer_event(
                InstallerEvents::PRE_OPERATIONS_EXEC,
                self.dev_mode,
                self.execute_operations,
                local_repo_transaction.to_transaction(),
            )?;

        let mut installs: Vec<String> = vec![];
        let mut updates: Vec<String> = vec![];
        let mut uninstalls: Vec<String> = vec![];
        for operation in local_repo_transaction.get_operations() {
            if let Some(io) = operation.as_install_operation() {
                installs.push(format!(
                    "{}:{}",
                    io.get_package().get_pretty_name(),
                    io.get_package()
                        .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
                ));
            } else if let Some(uo) = operation.as_update_operation() {
                updates.push(format!(
                    "{}:{}",
                    uo.get_target_package().get_pretty_name(),
                    uo.get_target_package()
                        .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
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
            local_repo.set_dev_package_names(self.locker.borrow_mut().get_dev_package_names()?);
            let mut local_repo_ref = local_repo.borrow_mut();
            self.installation_manager.borrow_mut().execute(
                local_repo_ref
                    .as_installed_repository_interface_mut()
                    .unwrap(),
                local_repo_transaction.get_operations().clone(),
                self.dev_mode,
                self.run_scripts,
                self.download_only,
            )?;
            drop(local_repo_ref);

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

    pub(crate) fn create_platform_repo(
        &mut self,
        for_update: bool,
    ) -> anyhow::Result<PlatformRepositoryHandle> {
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
                .borrow_mut()
                .get_platform_overrides()
                .unwrap_or_default()
                .into_iter()
                .map(|(k, v)| (k, PhpMixed::String(v)))
                .collect()
        };

        Ok(PlatformRepositoryHandle::new(PlatformRepository::new(
            vec![],
            platform_overrides,
        )?))
    }

    fn create_repository_set(
        &mut self,
        for_update: bool,
        platform_repo: &PlatformRepositoryHandle,
        root_aliases: &Vec<IndexMap<String, String>>,
        locked_repository: Option<&mut dyn RepositoryInterface>,
    ) -> anyhow::Result<RepositorySet> {
        let minimum_stability: String;
        let mut stability_flags: IndexMap<String, i64>;

        let requires: IndexMap<String, AnyConstraint>;
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
            let mut tmp: IndexMap<String, AnyConstraint> = IndexMap::new();
            for (k, link) in req_links {
                tmp.insert(k, link.get_constraint().clone());
            }
            requires = tmp;
        } else {
            minimum_stability = self
                .locker
                .borrow_mut()
                .get_minimum_stability()
                .unwrap_or_else(|_| String::new());
            // locker stores stability flags as stringified ints; recover the int form here.
            stability_flags = self
                .locker
                .borrow_mut()
                .get_stability_flags()
                .map(|m| {
                    m.into_iter()
                        .map(|(k, v)| (k, v.parse::<i64>().unwrap_or(0)))
                        .collect()
                })
                .unwrap_or_default();

            let mut tmp: IndexMap<String, AnyConstraint> = IndexMap::new();
            for package in locked_repository.unwrap().get_packages()? {
                let constraint = SimpleConstraint::new(
                    "=".to_string(),
                    package.get_version().to_string(),
                    Some(package.get_pretty_version().to_string()),
                );
                tmp.insert(package.get_name().to_string(), constraint.into());
            }
            requires = tmp;
        }

        let mut root_requires: IndexMap<String, AnyConstraint> = IndexMap::new();
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
                    .unwrap_or_else(|_| {
                        SimpleConstraint::new("=".to_string(), String::new().to_string(), None)
                            .into()
                    });
            }
            root_requires.insert(req, constraint);
        }

        self.fixed_root_package = RootPackageInterfaceHandle::dup(&self.package);
        self.fixed_root_package.set_requires(IndexMap::new());
        self.fixed_root_package.set_dev_requires(IndexMap::new());

        stability_flags.insert(
            self.package.get_name(),
            base_package::STABILITIES
                [VersionParser::parse_stability(&self.package.get_version()).as_str()],
        );

        let root_aliases_input: Vec<crate::repository::RootAliasInput> = root_aliases
            .into_iter()
            .map(|alias| crate::repository::RootAliasInput {
                package: alias.get("package").cloned().unwrap_or_default(),
                version: alias.get("version").cloned().unwrap_or_default(),
                alias: alias.get("alias").cloned().unwrap_or_default(),
                alias_normalized: alias.get("alias_normalized").cloned().unwrap_or_default(),
            })
            .collect();
        let temporary_constraints: IndexMap<String, AnyConstraint> = IndexMap::new();
        let mut repository_set = RepositorySet::new(
            &minimum_stability,
            stability_flags,
            root_aliases_input,
            self.package.get_references().clone(),
            root_requires,
            temporary_constraints,
        );
        repository_set.add_repository(crate::repository::RepositoryInterfaceHandle::new(
            RootPackageRepository::new(RootPackageInterfaceHandle::dup(&self.fixed_root_package)),
        ))?;
        repository_set.add_repository(platform_repo.clone().into())?;
        if let Some(ref additional_fixed_repository) = self.additional_fixed_repository {
            // allow using installed repos if needed to avoid warnings about installed repositories being used in the RepositorySet
            // see https://github.com/composer/composer/pull/9574
            let additional_fixed_repositories: Vec<crate::repository::RepositoryInterfaceHandle> = {
                let repo_ref = additional_fixed_repository.borrow();
                if let Some(composite) = repo_ref.as_any().downcast_ref::<CompositeRepository>() {
                    composite.get_repositories().clone()
                } else {
                    drop(repo_ref);
                    vec![additional_fixed_repository.clone()]
                }
            };
            for additional_fixed_repository in &additional_fixed_repositories {
                if additional_fixed_repository.is::<InstalledRepository>() {
                    repository_set.allow_installed_repositories(true);
                    break;
                }
            }

            let _ = repository_set.add_repository(additional_fixed_repository.clone());
        }

        Ok(repository_set)
    }

    fn create_policy(
        &mut self,
        for_update: bool,
        locked_repo: Option<&crate::repository::LockArrayRepositoryHandle>,
    ) -> anyhow::Result<std::rc::Rc<dyn PolicyInterface>> {
        let mut prefer_stable: Option<bool> = None;
        let mut prefer_lowest: Option<bool> = None;
        if !for_update {
            prefer_stable = self.locker.borrow_mut().get_prefer_stable().unwrap_or(None);
            prefer_lowest = self.locker.borrow_mut().get_prefer_lowest().unwrap_or(None);
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
            let pkgs = locked_repo.unwrap().borrow_mut().get_canonical_packages()?;
            for pkg in pkgs {
                if pkg.as_alias().is_some()
                    || (self.update_allow_list.is_some()
                        && self
                            .update_allow_list
                            .as_ref()
                            .unwrap()
                            .iter()
                            .any(|s| s == &pkg.get_name()))
                {
                    continue;
                }
                versions.insert(pkg.get_name(), pkg.get_version());
            }
            preferred_versions = Some(versions);
        }

        Ok(std::rc::Rc::new(DefaultPolicy::new(
            prefer_stable.unwrap(),
            prefer_lowest.unwrap(),
            preferred_versions,
        )))
    }

    fn create_request(
        &self,
        root_package: RootPackageInterfaceHandle,
        platform_repo: &PlatformRepositoryHandle,
        locked_repository: Option<&crate::repository::LockArrayRepositoryHandle>,
    ) -> anyhow::Result<Request> {
        let mut request = Request::new(locked_repository.cloned());

        let root_package_handle: PackageInterfaceHandle = root_package.clone().into();
        request.fix_package(root_package_handle.clone());
        if let Some(alias) = root_package_handle.as_root_alias_package() {
            request.fix_package(alias.get_alias_of().into());
        }

        let mut fixed_packages = platform_repo.borrow_mut().get_packages()?;
        if let Some(ref additional_fixed_repository) = self.additional_fixed_repository {
            fixed_packages.extend(additional_fixed_repository.get_packages()?);
        }

        // fix the version of all platform packages + additionally installed packages
        // to prevent the solver trying to remove or update those
        // TODO why not replaces?
        let provided = root_package.get_provides();
        for package in fixed_packages {
            // skip platform packages that are provided by the root package
            let pkg_repo_is_platform = package
                .get_repository()
                .map_or(false, |r| r.is::<PlatformRepository>());
            let name = package.get_name();
            if !pkg_repo_is_platform
                || !provided.contains_key(&name)
                || !provided.get(&name).unwrap().get_constraint().matches(
                    &SimpleConstraint::new(
                        "=".to_string(),
                        package.get_version().to_string(),
                        None,
                    )
                    .into(),
                )
            {
                request.fix_package(package);
            }
        }

        Ok(request)
    }

    fn require_packages_for_update(
        &mut self,
        request: &mut Request,
        locked_repository: Option<&crate::repository::LockArrayRepositoryHandle>,
        include_dev_requires: bool,
    ) -> anyhow::Result<()> {
        // if we're updating mirrors we want to keep exactly the same versions installed which are in the lock file, but we want current remote metadata
        if self.update_mirrors {
            let excluded_packages: indexmap::IndexSet<String> = if !include_dev_requires {
                let names = self.locker.borrow_mut().get_dev_package_names()?;
                names.into_iter().collect()
            } else {
                indexmap::IndexSet::new()
            };

            for locked_package in locked_repository
                .unwrap()
                .borrow_mut()
                .get_canonical_packages()?
            {
                // exclude alias packages here as for root aliases, both alias and aliased are
                // present in the lock repo and we only want to require the aliased version
                if locked_package.as_alias().is_none()
                    && !excluded_packages.contains(&locked_package.get_name())
                {
                    request.require_name(
                        &locked_package.get_name(),
                        Some(
                            SimpleConstraint::new(
                                "==".to_string(),
                                locked_package.get_version(),
                                None,
                            )
                            .into(),
                        ),
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
                request.require_name(link.get_target(), Some(link.get_constraint().clone()))?;
            }
        }
        Ok(())
    }

    fn get_root_aliases(&mut self, for_update: bool) -> Vec<IndexMap<String, String>> {
        if for_update {
            self.package.get_aliases().to_vec()
        } else {
            self.locker.borrow_mut().get_aliases().unwrap_or_default()
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
                    link.get_pretty_constraint().to_string(),
                );
            }
        }

        platform_reqs
    }

    /// Replace local repositories with InstalledArrayRepository instances
    ///
    /// This is to prevent any accidental modification of the existing repos on disk
    fn mock_local_repositories(&self, rm: &mut RepositoryManager) -> anyhow::Result<()> {
        let mut packages: IndexMap<String, PackageInterfaceHandle> = IndexMap::new();
        for package in rm.get_local_repository().get_packages()? {
            packages.insert(package.to_string(), PackageInterfaceHandle::dup(&package));
        }
        let keys: Vec<String> = packages.keys().cloned().collect();
        for key in keys {
            let package_clone = packages.get(&key).unwrap().clone();
            if let Some(alias_pkg) = package_clone.as_alias() {
                let alias_key = alias_pkg.get_alias_of().to_string();
                let aliased = packages.get(&alias_key).unwrap().clone();
                let version = alias_pkg.get_version();
                let pretty_version = alias_pkg.get_pretty_version();
                // PHP: $packages[$key] = new $className($packages[$alias], $package->getVersion(), $package->getPrettyVersion());
                let new_alias_package: PackageInterfaceHandle =
                    if package_clone.as_complete_alias_package().is_some() {
                        let complete = aliased.as_complete_package().expect(
                            "CompleteAliasPackage requires aliasOf to be a real CompletePackage",
                        );
                        CompleteAliasPackageHandle::new(complete, version, pretty_version).into()
                    } else {
                        let real = aliased
                            .as_package()
                            .expect("AliasPackage requires aliasOf to be a real Package");
                        AliasPackageHandle::new(real, version, pretty_version).into()
                    };
                packages.insert(key, new_alias_package);
            }
        }
        rm.set_local_repository(crate::repository::RepositoryInterfaceHandle::new(
            InstalledArrayRepository::new_with_packages(packages.into_values().collect())
                .expect("InstalledArrayRepository::new_with_packages should not fail"),
        ));

        Ok(())
    }

    fn create_pool_optimizer(
        &self,
        policy: std::rc::Rc<dyn PolicyInterface>,
    ) -> Option<PoolOptimizer> {
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

        Some(PoolOptimizer::new(policy))
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
    pub fn create(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        composer: &PartialComposerHandle,
    ) -> Self {
        let composer = crate::composer::composer_full(composer);
        Self::new(
            io,
            composer.get_config(),
            composer.get_package().clone(),
            composer.get_download_manager(),
            composer.get_repository_manager(),
            composer.get_locker(),
            composer.get_installation_manager(),
            composer.get_event_dispatcher(),
            composer.get_autoload_generator(),
        )
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
        additional_fixed_repository: crate::repository::RepositoryInterfaceHandle,
    ) -> &mut Self {
        self.additional_fixed_repository = Some(additional_fixed_repository);

        self
    }

    pub fn set_temporary_constraints(
        &mut self,
        constraints: IndexMap<String, AnyConstraint>,
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

    pub fn set_platform_requirement_filter(
        &mut self,
        platform_requirement_filter: std::rc::Rc<dyn PlatformRequirementFilterInterface>,
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
        update_allow_transitive_dependencies: UpdateAllowTransitiveDeps,
    ) -> anyhow::Result<&mut Self> {
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

    /// Should exit with status code 5 on audit error
    pub fn set_error_on_audit(&mut self, error_on_audit: bool) -> &mut Self {
        self.error_on_audit = error_on_audit;

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
        self.installation_manager.borrow_mut().disable_plugins();

        self
    }

    pub fn set_suggested_packages_reporter(
        &mut self,
        suggested_packages_reporter: std::rc::Rc<std::cell::RefCell<SuggestedPackagesReporter>>,
    ) -> &mut Self {
        self.suggested_packages_reporter = suggested_packages_reporter;

        self
    }
}
