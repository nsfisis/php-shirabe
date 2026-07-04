//! ref: composer/src/Composer/Installer/InstallationManager.php

use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::MarkAliasInstalledOperation;
use crate::dependency_resolver::operation::MarkAliasUninstalledOperation;
use crate::dependency_resolver::operation::OperationInterface;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::dependency_resolver::operation::UpdateOperation;
use crate::downloader::FileDownloader;
use crate::event_dispatcher::EventDispatcher;
use crate::installer::InstallerInterface;
use crate::installer::PackageEvents;
use crate::io::ConsoleIO;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::io_interface;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterface;
use crate::util::Platform;
use crate::util::r#loop::Loop;
use crate::util::sync_executor;
use indexmap::IndexMap;
use shirabe_external_packages::seld::signal::SignalHandler;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, array_splice, array_unshift, http_build_query, json_encode,
    str_contains, str_replace, strpos, strtolower,
};

/// Package operation manager.
#[derive(Debug)]
pub struct InstallationManager {
    installers: Vec<Box<dyn InstallerInterface>>,
    /// Maps a package type to the index of its installer in `installers`. PHP caches the installer
    /// instance itself; here we store an index instead to avoid sharing ownership of the boxed
    /// installer. The index never dangles because both `add_installer` and `remove_installer`
    /// clear the cache whenever `installers` changes.
    cache: IndexMap<String, usize>,
    notifiable_packages: IndexMap<String, Vec<PackageInterfaceHandle>>,
    loop_: std::rc::Rc<std::cell::RefCell<Loop>>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
    output_progress: bool,
    /// For testing only: present iff this manager behaves like
    /// `Composer\Test\Mock\InstallationManagerMock`, recording operations instead of executing
    /// them. `None` in production.
    mock: Option<InstallationManagerMockState>,
}

/// For testing only: recorded operations for the `InstallationManagerMock` behavior.
#[derive(Debug, Default)]
struct InstallationManagerMockState {
    installed: Vec<PackageInterfaceHandle>,
    updated: Vec<(PackageInterfaceHandle, PackageInterfaceHandle)>,
    uninstalled: Vec<PackageInterfaceHandle>,
    trace: Vec<String>,
}

impl InstallationManager {
    pub fn new(
        loop_: std::rc::Rc<std::cell::RefCell<Loop>>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
    ) -> Self {
        Self {
            installers: vec![],
            cache: IndexMap::new(),
            notifiable_packages: IndexMap::new(),
            loop_,
            io,
            event_dispatcher,
            output_progress: false,
            mock: None,
        }
    }

    /// For testing only: builds a manager that records operations instead of executing them,
    /// mirroring `Composer\Test\Mock\InstallationManagerMock`.
    pub fn __new_mock(
        loop_: std::rc::Rc<std::cell::RefCell<Loop>>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
    ) -> Self {
        Self {
            mock: Some(InstallationManagerMockState::default()),
            ..Self::new(loop_, io, event_dispatcher)
        }
    }

    /// For testing only: the trace of stringified operations recorded by the mock.
    pub fn __get_trace(&self) -> Vec<String> {
        self.mock
            .as_ref()
            .map(|m| m.trace.clone())
            .unwrap_or_default()
    }

    /// For testing only: packages passed to install (and markAliasInstalled) operations.
    pub fn __get_installed_packages(&self) -> Vec<PackageInterfaceHandle> {
        self.mock
            .as_ref()
            .map(|m| m.installed.clone())
            .unwrap_or_default()
    }

    /// For testing only: (initial, target) package pairs passed to update operations.
    pub fn __get_updated_packages(&self) -> Vec<(PackageInterfaceHandle, PackageInterfaceHandle)> {
        self.mock
            .as_ref()
            .map(|m| m.updated.clone())
            .unwrap_or_default()
    }

    /// For testing only: packages passed to uninstall (and markAliasUninstalled) operations.
    pub fn __get_uninstalled_packages(&self) -> Vec<PackageInterfaceHandle> {
        self.mock
            .as_ref()
            .map(|m| m.uninstalled.clone())
            .unwrap_or_default()
    }

    pub fn reset(&mut self) {
        self.notifiable_packages = IndexMap::new();
        FileDownloader::reset_download_metadata();
    }

    /// Adds installer
    pub fn add_installer(&mut self, installer: Box<dyn InstallerInterface>) {
        array_unshift(&mut self.installers, installer);
        self.cache = IndexMap::new();
    }

    /// Removes installer
    pub fn remove_installer(&mut self, installer: &dyn InstallerInterface) {
        let target = installer as *const dyn InstallerInterface as *const ();
        let key = self
            .installers
            .iter()
            .position(|inst| inst.as_ref() as *const dyn InstallerInterface as *const () == target);
        if let Some(k) = key {
            array_splice(&mut self.installers, k as i64, Some(1), vec![]);
            self.cache = IndexMap::new();
        }
    }

    /// Disables plugins.
    ///
    /// We prevent any plugins from being instantiated by
    /// disabling the PluginManager. This ensures that no third-party
    /// code is ever executed.
    pub fn disable_plugins(&mut self) {
        for installer in self.installers.iter_mut() {
            if let Some(plugin_installer) = installer.as_plugin_installer_mut() {
                plugin_installer.disable_plugins();
            }
        }
    }

    /// Returns installer for a specific package type.
    pub fn get_installer(&mut self, r#type: &str) -> anyhow::Result<&mut dyn InstallerInterface> {
        let r#type = strtolower(r#type);

        if let Some(&index) = self.cache.get(&r#type) {
            return Ok(self.installers[index].as_mut());
        }

        let index = self
            .installers
            .iter()
            .position(|installer| installer.supports(&r#type));
        if let Some(index) = index {
            self.cache.insert(r#type.clone(), index);
            return Ok(self.installers[index].as_mut());
        }

        Err(InvalidArgumentException {
            message: format!("Unknown installer type: {}", r#type),
            code: 0,
        }
        .into())
    }

    /// Checks whether provided package is installed in one of the registered installers.
    pub fn is_package_installed(
        &mut self,
        repo: &dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        // For testing only (ref InstallationManagerMock::isPackageInstalled).
        if self.mock.is_some() {
            return Ok(repo.has_package(package));
        }

        if let Some(alias) = package.as_alias() {
            let alias_of: PackageInterfaceHandle = alias.get_alias_of().into();
            return Ok(
                repo.has_package(package.clone()) && self.is_package_installed(repo, alias_of)?
            );
        }

        Ok(self
            .get_installer(&package.get_type())?
            .is_installed(repo, package.clone()))
    }

    /// Install binary for the given package.
    /// If the installer associated to this package doesn't handle that function, it'll do nothing.
    pub fn ensure_binaries_presence(&mut self, package: PackageInterfaceHandle) {
        let installer = self.get_installer(&package.get_type());
        let installer = match installer {
            Ok(i) => i,
            Err(_e) => {
                // no installer found for the current package type (@see `getInstaller()`)
                return;
            }
        };

        // if the given installer support installing binaries
        if let Some(bp) = installer.as_binary_presence_interface() {
            bp.ensure_binaries_presence(package);
        }
    }

    /// Executes solver operation.
    pub fn execute(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: Vec<std::rc::Rc<dyn OperationInterface>>,
        dev_mode: bool,
        run_scripts: bool,
        download_only: bool,
    ) -> anyhow::Result<()> {
        // For testing only: the mock records each operation and mutates the repo directly,
        // skipping the download step (ref InstallationManagerMock::execute). The alias operations'
        // repo mutation is inlined (rather than calling mark_alias_*) so `self.mock` can stay
        // borrowed across the loop without also borrowing `&self`.
        if let Some(mock) = self.mock.as_mut() {
            let _ = (dev_mode, run_scripts, download_only);
            for operation in operations {
                let trace = shirabe_php_shim::strip_tags(&operation.to_string());
                match operation.get_operation_type().as_str() {
                    "install" => {
                        let op = operation.as_install_operation().expect("install operation");
                        let package = op.get_package();
                        mock.installed.push(package.clone());
                        mock.trace.push(trace);
                        repo.add_package(PackageInterfaceHandle::dup(&package));
                    }
                    "update" => {
                        let op = operation.as_update_operation().expect("update operation");
                        let initial = op.get_initial_package().clone();
                        let target = op.get_target_package().clone();
                        mock.updated.push((initial.clone(), target.clone()));
                        mock.trace.push(trace);
                        repo.remove_package(initial);
                        if !repo.has_package(target.clone()) {
                            repo.add_package(PackageInterfaceHandle::dup(&target));
                        }
                    }
                    "uninstall" => {
                        let op = operation
                            .as_uninstall_operation()
                            .expect("uninstall operation");
                        let package = op.get_package();
                        mock.uninstalled.push(package.clone());
                        mock.trace.push(trace);
                        repo.remove_package(package);
                    }
                    "markAliasInstalled" => {
                        let op = operation
                            .as_any()
                            .downcast_ref::<MarkAliasInstalledOperation>()
                            .expect("markAliasInstalled operation");
                        let package = op.get_package();
                        mock.installed.push(package.clone().into());
                        mock.trace.push(trace);
                        if !repo.has_package(package.clone().into()) {
                            repo.add_package(PackageInterfaceHandle::dup(&package.into()));
                        }
                    }
                    "markAliasUninstalled" => {
                        let op = operation
                            .as_any()
                            .downcast_ref::<MarkAliasUninstalledOperation>()
                            .expect("markAliasUninstalled operation");
                        let package = op.get_package();
                        mock.uninstalled.push(package.clone().into());
                        mock.trace.push(trace);
                        repo.remove_package(package.into());
                    }
                    other => panic!("unknown operation type: {}", other),
                }
            }
            return Ok(());
        }

        // @var array<callable(): ?PromiseInterface<void|null>> $cleanupPromises
        let mut cleanup_promises: IndexMap<
            i64,
            Box<
                dyn Fn() -> Option<
                    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>>>>,
                >,
            >,
        > = IndexMap::new();

        let signal_handler = SignalHandler::create(
            vec![
                SignalHandler::SIGINT.to_string(),
                SignalHandler::SIGTERM.to_string(),
                SignalHandler::SIGHUP.to_string(),
            ],
            // TODO(phase-b): closure captures &mut self via &mut cleanup_promises
            Box::new(move |signal: String, handler: &SignalHandler| {
                // TODO(phase-b): self.io.write_error(...); self.run_cleanup(&cleanup_promises);
                let _ = signal;
                handler.exit_with_last_signal();
            }),
        );

        let all_operations: Vec<std::rc::Rc<dyn OperationInterface>> = operations.clone();

        let result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
            // execute operations in batches to make sure download-modifying-plugins are installed
            // before the other packages get downloaded
            let mut batches: Vec<IndexMap<i64, std::rc::Rc<dyn OperationInterface>>> = vec![];
            let mut batch: IndexMap<i64, std::rc::Rc<dyn OperationInterface>> = IndexMap::new();
            for (index, operation) in operations.into_iter().enumerate() {
                let index = index as i64;
                let package: Option<PackageInterfaceHandle> =
                    if let Some(update) = operation.as_update_operation() {
                        Some(update.get_target_package())
                    } else {
                        operation
                            .as_install_operation()
                            .map(|install| install.get_package())
                    };
                if let Some(package) = package
                    && package.get_type() == "composer-plugin"
                {
                    let extra = package.get_extra();
                    if extra
                        .get("plugin-modifies-downloads")
                        .and_then(|v| v.as_bool())
                        == Some(true)
                    {
                        if (batch.len() as i64) > 0 {
                            batches.push(std::mem::take(&mut batch));
                        }
                        let mut single = IndexMap::new();
                        single.insert(index, operation);
                        batches.push(single);

                        continue;
                    }
                }
                batch.insert(index, operation);
            }

            if (batch.len() as i64) > 0 {
                batches.push(batch);
            }

            for batch_to_execute in batches {
                sync_executor::block_on(self.download_and_execute_batch(
                    repo,
                    batch_to_execute,
                    &mut cleanup_promises,
                    dev_mode,
                    run_scripts,
                    download_only,
                    all_operations.clone(),
                ))?;
            }

            Ok(())
        })();

        // finally
        signal_handler.unregister();

        match result {
            Ok(()) => {}
            Err(e) => {
                sync_executor::block_on(self.run_cleanup(&cleanup_promises));
                return Err(e);
            }
        }

        if download_only {
            return Ok(());
        }

        // do a last write so that we write the repository even if nothing changed
        // as that can trigger an update of some files like InstalledVersions.php if
        // running a new composer version
        repo.write(dev_mode, self);

        Ok(())
    }

    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    async fn download_and_execute_batch(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: IndexMap<i64, std::rc::Rc<dyn OperationInterface>>,
        cleanup_promises: &mut IndexMap<
            i64,
            Box<
                dyn Fn() -> Option<
                    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>>>>,
                >,
            >,
        >,
        dev_mode: bool,
        run_scripts: bool,
        download_only: bool,
        all_operations: Vec<std::rc::Rc<dyn OperationInterface>>,
    ) -> anyhow::Result<()> {
        // PHP: waitOnPromises() shows a ProgressBar while the concurrent downloads resolve.
        // TODO(phase-c-promise): see the identical note in execute_batch — the single-threaded
        // port downloads serially in this same loop, so only a 0% -> 100% jump is rendered after
        // the loop instead of PHP's timing-driven intermediate snapshots.
        let download_promise_count = operations
            .values()
            .filter(|op| {
                let t = op.get_operation_type();
                t == "update" || t == "install"
            })
            .count() as i64;

        for (index, operation) in &operations {
            let op_type = operation.get_operation_type();

            // ignoring alias ops as they don't need to execute anything at this stage
            if !["update", "install", "uninstall"].contains(&op_type.as_str()) {
                continue;
            }

            let package: PackageInterfaceHandle;
            let initial_package: Option<PackageInterfaceHandle>;
            if op_type == "update" {
                // @var UpdateOperation $operation
                let update_op = operation
                    .as_update_operation()
                    .expect("op_type == \"update\" implies UpdateOperation");
                package = update_op.get_target_package();
                initial_package = Some(update_op.get_initial_package());
            } else {
                // @var InstallOperation|MarkAliasInstalledOperation|MarkAliasUninstalledOperation|UninstallOperation $operation
                package = operation.get_package();
                initial_package = None;
            }
            let installer = self.get_installer(&package.get_type())?;

            // PHP: $cleanupPromises[$index] = function () use ($index, $installer, $type, $package) {
            //   if (null === $package->getInstallationSource()) { return \React\Promise\resolve(null); }
            //   return $installer->cleanup($type, $package); };
            // TODO(phase-c): the cleanup callable must capture the installer and package and invoke
            // installer.cleanup(...) returning a React promise. It is a 'static closure stored in
            // cleanup_promises, so installer/package must be Rc-shared (the installer registry is
            // not Rc yet, see get_installer) and the promise type must be modelled. Both depend on
            // the async/React-Promise rework, so a no-op future is stored instead.
            let _ = installer;
            let op_type_clone = op_type.clone();
            let cleanup: Box<
                dyn Fn() -> Option<
                    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>>>>,
                >,
            > = Box::new(move || {
                // avoid calling cleanup if the download was not even initialized for a package
                // as without installation source configured nothing will work
                // TODO(phase-b): if (null === $package->getInstallationSource()) return resolve(null);
                let _ = &op_type_clone;
                // TODO(phase-c-promise): build the real installer.cleanup() future once the installer
                // can be shared into a 'static cleanup closure (Stage 2 Rc/Arc).
                let fut: std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>>>> =
                    Box::pin(async { Ok(()) });
                Some(fut)
            });
            cleanup_promises.insert(*index, cleanup);

            if op_type != "uninstall" {
                // TODO(phase-c-promise): PHP collects every download and runs them concurrently via
                // Loop::wait; the single-threaded loop awaits each serially instead.
                let installer = self.get_installer(&package.get_type())?;
                installer.download(package, initial_package).await?;
            }
        }

        if self.output_progress
            && !Platform::get_env("CI").is_some_and(|v| !v.is_empty() && v != "0")
            && !self.io.is_debug()
            && download_promise_count > 1
        {
            let bar = {
                let io_ref = self.io.borrow();
                io_ref
                    .as_any()
                    .downcast_ref::<ConsoleIO>()
                    .map(|console_io| console_io.get_progress_bar(download_promise_count))
            };
            if let Some(mut bar) = bar {
                bar.start(Some(download_promise_count))?;
                bar.set_progress(download_promise_count)?;
                bar.finish()?;
                bar.clear()?;
                if !self.io.is_decorated() {
                    self.io.write_error("");
                }
            }
        }

        if download_only {
            self.run_cleanup(cleanup_promises).await;

            return Ok(());
        }

        // execute operations in batches to make sure every plugin is installed in the
        // right order and activated before the packages depending on it are installed
        let mut batches: Vec<IndexMap<i64, std::rc::Rc<dyn OperationInterface>>> = vec![];
        let mut batch: IndexMap<i64, std::rc::Rc<dyn OperationInterface>> = IndexMap::new();
        for (index, operation) in operations {
            let package: Option<PackageInterfaceHandle> =
                if let Some(update) = operation.as_update_operation() {
                    Some(update.get_target_package())
                } else {
                    operation
                        .as_install_operation()
                        .map(|install| install.get_package())
                };
            if let Some(package) = package {
                let pkg_type = package.get_type();
                if pkg_type == "composer-plugin" || pkg_type == "composer-installer" {
                    if (batch.len() as i64) > 0 {
                        batches.push(std::mem::take(&mut batch));
                    }
                    let mut single = IndexMap::new();
                    single.insert(index, operation);
                    batches.push(single);

                    continue;
                }
            }
            batch.insert(index, operation);
        }

        if (batch.len() as i64) > 0 {
            batches.push(batch);
        }

        for batch_to_execute in batches {
            self.execute_batch(
                repo,
                batch_to_execute,
                cleanup_promises,
                dev_mode,
                run_scripts,
                &all_operations,
            )
            .await?;
        }

        Ok(())
    }

    async fn execute_batch(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: IndexMap<i64, std::rc::Rc<dyn OperationInterface>>,
        cleanup_promises: &IndexMap<
            i64,
            Box<
                dyn Fn() -> Option<
                    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>>>>,
                >,
            >,
        >,
        dev_mode: bool,
        run_scripts: bool,
        all_operations: &[std::rc::Rc<dyn OperationInterface>],
    ) -> anyhow::Result<()> {
        let mut post_exec_callbacks: Vec<Box<dyn Fn()>> = vec![];

        // PHP: waitOnPromises() shows a ProgressBar while React\Promise\all($promises) resolves,
        // driven by Loop::wait's active-job polling as concurrent downloads/installs finish over
        // real wall-clock time, interleaved with nothing else since the "- Installing ..." lines
        // are all written up front while the promises are being constructed.
        // TODO(phase-c-promise): the single-threaded port runs prepare/install/cleanup serially
        // in this same loop that also writes the "- Installing ..." lines, so there is no way to
        // draw a step-by-step bar without garbling it into the middle of that output (the bar's
        // line-overwrite state and the plain `write_error` lines fight over the same terminal
        // line). Rendering a single 0% -> 100% jump after the loop keeps output well-formed at
        // the cost of the intermediate snapshots real Composer shows.
        let promise_count = operations
            .values()
            .filter(|op| {
                ["update", "install", "uninstall"].contains(&op.get_operation_type().as_str())
            })
            .count() as i64;

        for (index, operation) in operations {
            let op_type = operation.get_operation_type();

            // ignoring alias ops as they don't need to execute anything
            if !["update", "install", "uninstall"].contains(&op_type.as_str()) {
                // output alias ops in debug verbosity as they have no output otherwise
                if self.io.is_debug() {
                    self.io.write_error3(
                        &format!("  - {}", operation.show(false)),
                        true,
                        io_interface::NORMAL,
                    );
                }
                match op_type.as_str() {
                    "markAliasInstalled" => {
                        let op = operation
                            .as_any()
                            .downcast_ref::<MarkAliasInstalledOperation>()
                            .expect(
                                "op_type == \"markAliasInstalled\" implies MarkAliasInstalledOperation",
                            );
                        self.mark_alias_installed(repo, op);
                    }
                    "markAliasUninstalled" => {
                        let op = operation
                            .as_any()
                            .downcast_ref::<MarkAliasUninstalledOperation>()
                            .expect(
                                "op_type == \"markAliasUninstalled\" implies MarkAliasUninstalledOperation",
                            );
                        self.mark_alias_uninstalled(repo, op);
                    }
                    _ => {}
                }

                continue;
            }

            let package: PackageInterfaceHandle;
            let initial_package: Option<PackageInterfaceHandle>;
            if op_type == "update" {
                let update_op = operation
                    .as_update_operation()
                    .expect("op_type == \"update\" implies UpdateOperation");
                package = update_op.get_target_package();
                initial_package = Some(update_op.get_initial_package());
            } else {
                package = operation.get_package();
                initial_package = None;
            }

            let event_name = match op_type.as_str() {
                "install" => PackageEvents::PRE_PACKAGE_INSTALL,
                "update" => PackageEvents::PRE_PACKAGE_UPDATE,
                "uninstall" => PackageEvents::PRE_PACKAGE_UNINSTALL,
                _ => "",
            };

            if run_scripts && self.event_dispatcher.is_some() {
                // TODO(phase-c): dispatch_package_event takes Box<dyn RepositoryInterface>/Vec<Box<...>>
                // but we hold &mut dyn here. Needs structural rework (likely shared Rc on repo and ops).
                let _ = (
                    event_name,
                    dev_mode,
                    &repo,
                    &all_operations,
                    operation.as_ref(),
                );
            }

            let _dispatcher = self.event_dispatcher.as_ref();
            let _io = self.io.as_ref();

            {
                let installer = self.get_installer(&package.get_type())?;
                installer
                    .prepare(&op_type, package.clone(), initial_package.clone())
                    .await?;
            }

            // PHP: $promise = $promise->then(fn() => $this->{$type}(...))->then($cleanupPromises[$index])
            //      ->then(fn() => $repo->write($devMode, $this->io)); the chained steps run install/
            //      update/uninstall, then cleanup, then persist the repository. The single-threaded
            //      loop awaits the steps serially instead of composing React promises.
            let op_result = match op_type.as_str() {
                "install" => {
                    let op = operation
                        .as_install_operation()
                        .expect("op_type == \"install\" implies InstallOperation");
                    self.install(repo, op).await
                }
                "update" => {
                    let op = operation
                        .as_update_operation()
                        .expect("op_type == \"update\" implies UpdateOperation");
                    self.update(repo, op).await
                }
                "uninstall" => {
                    let op = operation
                        .as_uninstall_operation()
                        .expect("op_type == \"uninstall\" implies UninstallOperation");
                    self.uninstall(repo, op).await
                }
                _ => unreachable!("op_type is one of install/update/uninstall"),
            };

            // PHP rejects the promise with an "<op> of <name> failed" message before rethrowing.
            if let Err(e) = op_result {
                self.io.write_error(&format!(
                    "    <error>{} of {} failed</error>",
                    shirabe_php_shim::ucfirst(&op_type),
                    package.get_pretty_name()
                ));
                return Err(e);
            }

            // TODO(phase-c-promise): cleanup_promises[index] currently resolves to a no-op future
            // (the real installer.cleanup() chain depends on the Rc/Arc installer rework).
            if let Some(cleanup) = cleanup_promises.get(&index)
                && let Some(fut) = cleanup()
            {
                fut.await?;
            }

            // PHP: ->then(fn() => $repo->write($devMode, $this)) persists the repository after each op.
            repo.write(dev_mode, self);

            let event_name_post = match op_type.as_str() {
                "install" => PackageEvents::POST_PACKAGE_INSTALL,
                "update" => PackageEvents::POST_PACKAGE_UPDATE,
                "uninstall" => PackageEvents::POST_PACKAGE_UNINSTALL,
                _ => "",
            };

            if run_scripts && self.event_dispatcher.is_some() {
                // PHP appends a post-exec step to the promise chain that dispatches the
                // POST_PACKAGE_* event via the event dispatcher with repo/all_operations/operation.
                // TODO(phase-c): the callback captures the event dispatcher (&mut) and the operation
                // and must outlive the loop body; that requires the dispatcher behind Rc<RefCell>
                // and the deferred event dispatch to be wired into the promise chain (todo!()).
                let _ = event_name_post;
                post_exec_callbacks.push(Box::new(|| {
                    // dispatcher.dispatch_package_event(event_name_post, dev_mode, repo, all_operations, operation);
                }));
            }
        }

        if self.output_progress
            && !Platform::get_env("CI").is_some_and(|v| !v.is_empty() && v != "0")
            && !self.io.is_debug()
            && promise_count > 1
        {
            let bar = {
                let io_ref = self.io.borrow();
                io_ref
                    .as_any()
                    .downcast_ref::<ConsoleIO>()
                    .map(|console_io| console_io.get_progress_bar(promise_count))
            };
            if let Some(mut bar) = bar {
                bar.start(Some(promise_count))?;
                bar.set_progress(promise_count)?;
                bar.finish()?;
                bar.clear()?;
                // ProgressBar in non-decorated output does not output a final line-break and clear() does nothing
                if !self.io.is_decorated() {
                    self.io.write_error("");
                }
            }
        }

        Platform::workaround_filesystem_issues();

        for cb in &post_exec_callbacks {
            cb();
        }

        Ok(())
    }

    /// Executes download operation.
    pub async fn download(&mut self, package: PackageInterfaceHandle) -> Option<PhpMixed> {
        let installer = self.get_installer(&package.get_type()).ok()?;

        installer.cleanup("install", package, None).await.ok()?
    }

    /// Executes install operation.
    pub async fn install(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &InstallOperation,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let package = operation.get_package();
        let package_type = package.get_type();
        let installer = self.get_installer(&package_type)?;
        let promise = installer.install(repo, package.clone()).await?;
        self.mark_for_notification(package.clone());

        Ok(promise)
    }

    /// Executes update operation.
    pub async fn update(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &UpdateOperation,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let initial = operation.get_initial_package().clone();
        let target = operation.get_target_package().clone();

        let initial_type = initial.get_type();
        let target_type = target.get_type();

        if initial_type == target_type {
            let installer = self.get_installer(&initial_type)?;
            let promise = installer.update(repo, initial, target.clone()).await?;
            self.mark_for_notification(target.clone());
            Ok(promise)
        } else {
            // PHP: uninstall initial, then install target via the target-type installer.
            let _ = self
                .get_installer(&initial_type)?
                .uninstall(repo, initial)
                .await?;
            let installer = self.get_installer(&target_type)?;
            installer.install(repo, target).await
        }
    }

    /// Uninstalls package.
    pub async fn uninstall(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &UninstallOperation,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let package = operation.get_package();
        let package_type = package.get_type();
        let installer = self.get_installer(&package_type)?;

        installer.uninstall(repo, package).await
    }

    /// Executes markAliasInstalled operation.
    pub fn mark_alias_installed(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &MarkAliasInstalledOperation,
    ) {
        let package = operation.get_package();

        if !repo.has_package(package.clone().into()) {
            repo.add_package(crate::package::PackageInterfaceHandle::dup(&package.into()));
        }
    }

    /// Executes markAlias operation.
    pub fn mark_alias_uninstalled(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &MarkAliasUninstalledOperation,
    ) {
        let package = operation.get_package();

        repo.remove_package(package.clone().into());
    }

    /// Returns the installation path of a package
    pub fn get_install_path(&mut self, package: PackageInterfaceHandle) -> Option<String> {
        // For testing only (ref InstallationManagerMock::getInstallPath).
        if self.mock.is_some() {
            return Some(format!("vendor/{}", package.get_name()));
        }

        let installer = self.get_installer(&package.get_type()).ok()?;

        installer.get_install_path(package)
    }

    pub fn set_output_progress(&mut self, output_progress: bool) {
        self.output_progress = output_progress;
    }

    pub fn notify_installs(&mut self, _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>) {
        // For testing only (ref InstallationManagerMock::notifyInstalls is a noop).
        if self.mock.is_some() {
            return;
        }

        // TODO(phase-c-promise): PHP collects every http_downloader.add() promise and runs them via
        // Loop::wait; the single-threaded sync bridge block_on's each notification serially instead.
        let result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
            for (repo_url, packages) in &self.notifiable_packages {
                // non-batch API, deprecated
                if str_contains(repo_url, "%package%") {
                    for package in packages {
                        let url = str_replace("%package%", &package.get_pretty_name(), repo_url);

                        let mut params: IndexMap<String, String> = IndexMap::new();
                        params.insert("version".to_string(), package.get_pretty_version());
                        params.insert("version_normalized".to_string(), package.get_version());
                        let mut opts: IndexMap<String, PhpMixed> = IndexMap::new();
                        opts.insert("retry-auth-failure".to_string(), PhpMixed::Bool(false));
                        let mut http: IndexMap<String, PhpMixed> = IndexMap::new();
                        http.insert("method".to_string(), PhpMixed::String("POST".to_string()));
                        http.insert(
                            "header".to_string(),
                            PhpMixed::List(vec![PhpMixed::String(
                                "Content-type: application/x-www-form-urlencoded".to_string(),
                            )]),
                        );
                        let params_vec: Vec<(&str, &str)> = params
                            .iter()
                            .map(|(k, v)| (k.as_str(), v.as_str()))
                            .collect();
                        http.insert(
                            "content".to_string(),
                            PhpMixed::String(http_build_query(&params_vec, "", "&")),
                        );
                        http.insert("timeout".to_string(), PhpMixed::Int(3));
                        opts.insert(
                            "http".to_string(),
                            PhpMixed::Array(http.into_iter().collect()),
                        );

                        sync_executor::block_on(
                            self.loop_
                                .borrow()
                                .get_http_downloader()
                                .borrow_mut()
                                .add(&url, opts),
                        )?;
                    }

                    continue;
                }

                let mut post_data: IndexMap<String, PhpMixed> = IndexMap::new();
                post_data.insert("downloads".to_string(), PhpMixed::List(vec![]));
                for package in packages {
                    let mut package_notification: IndexMap<String, PhpMixed> = IndexMap::new();
                    package_notification.insert(
                        "name".to_string(),
                        PhpMixed::String(package.get_pretty_name()),
                    );
                    package_notification.insert(
                        "version".to_string(),
                        PhpMixed::String(package.get_version()),
                    );
                    if strpos(repo_url, "packagist.org/").is_some() {
                        if let Some(metadata) =
                            FileDownloader::download_metadata().get(&package.get_name())
                        {
                            package_notification.insert("downloaded".to_string(), metadata.clone());
                        } else {
                            package_notification
                                .insert("downloaded".to_string(), PhpMixed::Bool(false));
                        }
                    }
                    if let Some(PhpMixed::List(downloads)) = post_data.get_mut("downloads") {
                        downloads.push(PhpMixed::Array(package_notification.into_iter().collect()));
                    }
                }

                let mut opts: IndexMap<String, PhpMixed> = IndexMap::new();
                opts.insert("retry-auth-failure".to_string(), PhpMixed::Bool(false));
                let mut http: IndexMap<String, PhpMixed> = IndexMap::new();
                http.insert("method".to_string(), PhpMixed::String("POST".to_string()));
                http.insert(
                    "header".to_string(),
                    PhpMixed::List(vec![PhpMixed::String(
                        "Content-Type: application/json".to_string(),
                    )]),
                );
                http.insert(
                    "content".to_string(),
                    PhpMixed::String(
                        json_encode(&PhpMixed::Array(post_data.into_iter().collect()))
                            .unwrap_or_default(),
                    ),
                );
                http.insert("timeout".to_string(), PhpMixed::Int(6));
                opts.insert(
                    "http".to_string(),
                    PhpMixed::Array(http.into_iter().collect()),
                );

                sync_executor::block_on(
                    self.loop_
                        .borrow()
                        .get_http_downloader()
                        .borrow_mut()
                        .add(repo_url, opts),
                )?;
            }

            Ok(())
        })();
        // PHP swallows the exception silently here
        let _ = result;

        self.reset();
    }

    fn mark_for_notification(&mut self, package: PackageInterfaceHandle) {
        if let Some(notification_url) = package.get_notification_url() {
            self.notifiable_packages
                .entry(notification_url)
                .or_default()
                .push(package.clone());
        }
    }

    async fn run_cleanup(
        &mut self,
        cleanup_promises: &IndexMap<
            i64,
            Box<
                dyn Fn() -> Option<
                    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>>>>,
                >,
            >,
        >,
    ) {
        let mut promises: Vec<
            std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>>>>,
        > = vec![];

        self.loop_.borrow().abort_jobs();

        for (_, cleanup) in cleanup_promises {
            // PHP wraps a missing cleanup promise in \React\Promise\resolve(null).
            let promise = cleanup();
            if let Some(p) = promise {
                promises.push(p);
            } else {
                promises.push(Box::pin(async { Ok(()) }));
            }
        }

        if (promises.len() as i64) > 0 {
            let _ = self.loop_.borrow_mut().wait(promises, None).await;
        }
    }
}

// Composer's PartialComposer::setInstallationManager() accepts any InstallationManager subclass, so
// plugins may swap in a replacement. The interface captures the methods reached through Composer's
// accessor and through the `&mut dyn InstallationManagerInterface` references fed from it.
pub trait InstallationManagerInterface: std::fmt::Debug {
    /// For testing only: lets a test recover the concrete manager (e.g. the recording mock) from a
    /// trait object returned by `Composer::get_installation_manager`.
    fn as_any(&self) -> &dyn std::any::Any {
        unimplemented!("as_any is only implemented for the concrete InstallationManager")
    }

    fn add_installer(&mut self, installer: Box<dyn InstallerInterface>);
    fn remove_installer(&mut self, installer: &dyn InstallerInterface);
    fn disable_plugins(&mut self);
    fn is_package_installed(
        &mut self,
        repo: &dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool>;
    fn ensure_binaries_presence(&mut self, package: PackageInterfaceHandle);
    fn execute(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: Vec<std::rc::Rc<dyn OperationInterface>>,
        dev_mode: bool,
        run_scripts: bool,
        download_only: bool,
    ) -> anyhow::Result<()>;
    fn get_install_path(&mut self, package: PackageInterfaceHandle) -> Option<String>;
    fn set_output_progress(&mut self, output_progress: bool);
    fn notify_installs(&mut self, io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>);
}

impl InstallationManagerInterface for InstallationManager {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn add_installer(&mut self, installer: Box<dyn InstallerInterface>) {
        self.add_installer(installer);
    }

    fn remove_installer(&mut self, installer: &dyn InstallerInterface) {
        self.remove_installer(installer);
    }

    fn disable_plugins(&mut self) {
        self.disable_plugins();
    }

    fn is_package_installed(
        &mut self,
        repo: &dyn InstalledRepositoryInterface,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<bool> {
        self.is_package_installed(repo, package)
    }

    fn ensure_binaries_presence(&mut self, package: PackageInterfaceHandle) {
        self.ensure_binaries_presence(package);
    }

    fn execute(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: Vec<std::rc::Rc<dyn OperationInterface>>,
        dev_mode: bool,
        run_scripts: bool,
        download_only: bool,
    ) -> anyhow::Result<()> {
        self.execute(repo, operations, dev_mode, run_scripts, download_only)
    }

    fn get_install_path(&mut self, package: PackageInterfaceHandle) -> Option<String> {
        self.get_install_path(package)
    }

    fn set_output_progress(&mut self, output_progress: bool) {
        self.set_output_progress(output_progress);
    }

    fn notify_installs(&mut self, io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>) {
        self.notify_installs(io);
    }
}
