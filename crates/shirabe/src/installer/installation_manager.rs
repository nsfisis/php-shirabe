//! ref: composer/src/Composer/Installer/InstallationManager.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::seld::signal::SignalHandler;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, array_search_mixed, array_splice, array_unshift, count,
    http_build_query, json_encode, str_contains, str_replace, strpos, strtolower, ucfirst,
};

use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::MarkAliasInstalledOperation;
use crate::dependency_resolver::operation::MarkAliasUninstalledOperation;
use crate::dependency_resolver::operation::OperationInterface;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::dependency_resolver::operation::UpdateOperation;
use crate::downloader::FileDownloader;
use crate::event_dispatcher::EventDispatcher;
use crate::installer::BinaryPresenceInterface;
use crate::installer::InstallerInterface;
use crate::installer::PackageEvents;
use crate::installer::PluginInstaller;
use crate::io::ConsoleIO;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::repository::InstalledRepositoryInterface;
use crate::util::Platform;
use crate::util::r#loop::Loop;

/// Package operation manager.
#[derive(Debug)]
pub struct InstallationManager {
    /// @var list<InstallerInterface>
    installers: Vec<Box<dyn InstallerInterface>>,
    /// @var array<string, InstallerInterface>
    cache: IndexMap<String, Box<dyn InstallerInterface>>,
    /// @var array<string, array<PackageInterface>>
    notifiable_packages: IndexMap<String, Vec<PackageInterfaceHandle>>,
    loop_: std::rc::Rc<std::cell::RefCell<Loop>>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
    output_progress: bool,
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
        }
    }

    pub fn reset(&mut self) {
        self.notifiable_packages = IndexMap::new();
        FileDownloader::reset_download_metadata();
    }

    /// Adds installer
    ///
    /// @param InstallerInterface $installer installer instance
    pub fn add_installer(&mut self, installer: Box<dyn InstallerInterface>) {
        array_unshift(&mut self.installers, installer);
        self.cache = IndexMap::new();
    }

    /// Removes installer
    ///
    /// @param InstallerInterface $installer installer instance
    pub fn remove_installer(&mut self, installer: &dyn InstallerInterface) {
        // TODO(phase-b): array_search for trait object identity needs concrete type info
        let _ = installer;
        let key: Option<usize> = None;
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
    ///
    /// @param string $type package type
    ///
    /// @throws \InvalidArgumentException if installer for provided type is not registered
    pub fn get_installer(&mut self, r#type: &str) -> Result<&mut dyn InstallerInterface> {
        let r#type = strtolower(r#type);

        if self.cache.contains_key(&r#type) {
            return Ok(self.cache.get_mut(&r#type).unwrap().as_mut());
        }

        for installer in &self.installers {
            if installer.supports(&r#type) {
                // TODO(phase-b): cache by cloning Box<dyn InstallerInterface> is non-trivial
                self.cache.insert(r#type.clone(), installer.clone_box());
                return Ok(self.cache.get_mut(&r#type).unwrap().as_mut());
            }
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
    ) -> Result<bool> {
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
    ) -> Result<()> {
        // @var array<callable(): ?PromiseInterface<void|null>> $cleanupPromises
        let mut cleanup_promises: IndexMap<
            i64,
            Box<
                dyn Fn()
                    -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>>>>>,
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

        let result: Result<()> = (|| -> Result<()> {
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
                if let Some(package) = package {
                    if package.get_type() == "composer-plugin" {
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
                }
                batch.insert(index, operation);
            }

            if (batch.len() as i64) > 0 {
                batches.push(batch);
            }

            for batch_to_execute in batches {
                tokio::runtime::Runtime::new().unwrap().block_on(
                    self.download_and_execute_batch(
                        repo,
                        batch_to_execute,
                        &mut cleanup_promises,
                        dev_mode,
                        run_scripts,
                        download_only,
                        all_operations.clone(),
                    ),
                )?;
            }

            Ok(())
        })();

        // finally
        signal_handler.unregister();

        match result {
            Ok(()) => {}
            Err(e) => {
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(self.run_cleanup(&cleanup_promises));
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

    /// @param OperationInterface[] $operations    List of operations to execute in this batch
    /// @param OperationInterface[] $allOperations Complete list of operations to be executed in the install job, used for event listeners
    /// @phpstan-param array<callable(): ?PromiseInterface<void|null>> $cleanupPromises
    async fn download_and_execute_batch(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: IndexMap<i64, std::rc::Rc<dyn OperationInterface>>,
        cleanup_promises: &mut IndexMap<
            i64,
            Box<
                dyn Fn()
                    -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>>>>>,
            >,
        >,
        dev_mode: bool,
        run_scripts: bool,
        download_only: bool,
        all_operations: Vec<std::rc::Rc<dyn OperationInterface>>,
    ) -> Result<()> {
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

            // TODO(phase-b): closure captures installer + package; needs Rc-shared installer/package
            let _ = installer;
            let op_type_clone = op_type.clone();
            let cleanup: Box<
                dyn Fn()
                    -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>>>>>,
            > = Box::new(move || {
                // avoid calling cleanup if the download was not even initialized for a package
                // as without installation source configured nothing will work
                // TODO(phase-b): if (null === $package->getInstallationSource()) return resolve(null);
                let _ = &op_type_clone;
                // TODO(phase-c-promise): build the real installer.cleanup() future once the installer
                // can be shared into a 'static cleanup closure (Stage 2 Rc/Arc).
                let fut: std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>>>> =
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

    /// @param OperationInterface[] $operations    List of operations to execute in this batch
    /// @param OperationInterface[] $allOperations Complete list of operations to be executed in the install job, used for event listeners
    /// @phpstan-param array<callable(): ?PromiseInterface<void|null>> $cleanupPromises
    async fn execute_batch(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: IndexMap<i64, std::rc::Rc<dyn OperationInterface>>,
        cleanup_promises: &IndexMap<
            i64,
            Box<
                dyn Fn()
                    -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>>>>>,
            >,
        >,
        dev_mode: bool,
        run_scripts: bool,
        all_operations: &[std::rc::Rc<dyn OperationInterface>],
    ) -> Result<()> {
        let mut post_exec_callbacks: Vec<Box<dyn Fn()>> = vec![];

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
                // TODO(phase-b): dispatch_package_event takes Box<dyn RepositoryInterface>/Vec<Box<...>>
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

            let installer = self.get_installer(&package.get_type())?;
            // TODO(phase-c-promise): PHP chains prepare()->then(install/update/uninstall)->then(cleanup
            // + repo.write); the single-threaded loop awaits prepare and leaves the rest as phase-b work.
            installer
                .prepare(&op_type, package, initial_package)
                .await?;

            // TODO(phase-b): chain the install/update/uninstall step with cleanup_promises[index], repo.write, etc.
            let _ = cleanup_promises.get(&index);

            let event_name_post = match op_type.as_str() {
                "install" => PackageEvents::POST_PACKAGE_INSTALL,
                "update" => PackageEvents::POST_PACKAGE_UPDATE,
                "uninstall" => PackageEvents::POST_PACKAGE_UNINSTALL,
                _ => "",
            };

            if run_scripts && self.event_dispatcher.is_some() {
                // TODO(phase-b): post-exec callback captures &mut dispatcher and operation
                let _ = event_name_post;
                post_exec_callbacks.push(Box::new(|| {
                    // dispatcher.dispatch_package_event(event_name_post, dev_mode, repo, all_operations, operation);
                }));
            }
        }

        Platform::workaround_filesystem_issues();

        for cb in &post_exec_callbacks {
            cb();
        }

        Ok(())
    }

    /// Executes download operation.
    ///
    /// @phpstan-return PromiseInterface<void|null>|null
    pub async fn download(&mut self, package: PackageInterfaceHandle) -> Option<PhpMixed> {
        let installer = self.get_installer(&package.get_type()).ok()?;
        let promise = installer.cleanup("install", package, None).await.ok()?;

        promise
    }

    /// Executes install operation.
    ///
    /// @phpstan-return PromiseInterface<void|null>|null
    pub async fn install(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &InstallOperation,
    ) -> Option<PhpMixed> {
        let package = operation.get_package();
        let package_type = package.get_type();
        let installer = self.get_installer(&package_type).ok()?;
        let promise = installer.install(repo, package.clone()).await.ok()?;
        self.mark_for_notification(package.clone());

        promise
    }

    /// Executes update operation.
    ///
    /// @phpstan-return PromiseInterface<void|null>|null
    pub async fn update(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &UpdateOperation,
    ) -> Option<PhpMixed> {
        let initial = operation.get_initial_package().clone();
        let target = operation.get_target_package().clone();

        let initial_type = initial.get_type();
        let target_type = target.get_type();

        let promise = if initial_type == target_type {
            let installer = self.get_installer(&initial_type).ok()?;
            let promise = installer.update(repo, initial, target.clone()).await.ok()?;
            self.mark_for_notification(target.clone());
            promise
        } else {
            // PHP: uninstall initial, then install target via the target-type installer.
            let _ = self
                .get_installer(&initial_type)
                .ok()?
                .uninstall(repo, initial)
                .await
                .ok()?;
            let installer = self.get_installer(&target_type).ok()?;
            installer.install(repo, target).await.ok()?
        };

        promise
    }

    /// Uninstalls package.
    ///
    /// @phpstan-return PromiseInterface<void|null>|null
    pub async fn uninstall(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &UninstallOperation,
    ) -> Option<PhpMixed> {
        let package = operation.get_package();
        let package_type = package.get_type();
        let installer = self.get_installer(&package_type).ok()?;

        installer.uninstall(repo, package).await.ok()?
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
    ///
    /// @return string|null absolute path to install to, which does not end with a slash, or null if the package does not have anything installed on disk
    pub fn get_install_path(&mut self, package: PackageInterfaceHandle) -> Option<String> {
        let installer = self.get_installer(&package.get_type()).ok()?;

        installer.get_install_path(package)
    }

    pub fn set_output_progress(&mut self, output_progress: bool) {
        self.output_progress = output_progress;
    }

    pub fn notify_installs(&mut self, _io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>) {
        // TODO(phase-c-promise): PHP collects every http_downloader.add() promise and runs them via
        // Loop::wait; the single-threaded sync bridge block_on's each notification serially instead.
        let result: Result<()> = (|| -> Result<()> {
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
                            PhpMixed::List(vec![Box::new(PhpMixed::String(
                                "Content-type: application/x-www-form-urlencoded".to_string(),
                            ))]),
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
                            PhpMixed::Array(
                                http.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                            ),
                        );

                        tokio::runtime::Runtime::new().unwrap().block_on(
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
                        downloads.push(Box::new(PhpMixed::Array(
                            package_notification
                                .into_iter()
                                .map(|(k, v)| (k, Box::new(v)))
                                .collect(),
                        )));
                    }
                }

                let mut opts: IndexMap<String, PhpMixed> = IndexMap::new();
                opts.insert("retry-auth-failure".to_string(), PhpMixed::Bool(false));
                let mut http: IndexMap<String, PhpMixed> = IndexMap::new();
                http.insert("method".to_string(), PhpMixed::String("POST".to_string()));
                http.insert(
                    "header".to_string(),
                    PhpMixed::List(vec![Box::new(PhpMixed::String(
                        "Content-Type: application/json".to_string(),
                    ))]),
                );
                http.insert(
                    "content".to_string(),
                    PhpMixed::String(
                        json_encode(&PhpMixed::Array(
                            post_data
                                .into_iter()
                                .map(|(k, v)| (k, Box::new(v)))
                                .collect(),
                        ))
                        .unwrap_or_default(),
                    ),
                );
                http.insert("timeout".to_string(), PhpMixed::Int(6));
                opts.insert(
                    "http".to_string(),
                    PhpMixed::Array(http.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
                );

                tokio::runtime::Runtime::new().unwrap().block_on(
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
                .or_insert_with(Vec::new)
                .push(package.clone());
        }
    }

    /// @phpstan-param array<callable(): ?PromiseInterface<void|null>> $cleanupPromises
    async fn run_cleanup(
        &mut self,
        cleanup_promises: &IndexMap<
            i64,
            Box<
                dyn Fn()
                    -> Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>>>>>,
            >,
        >,
    ) {
        let mut promises: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>>>>> =
            vec![];

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
