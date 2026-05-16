//! ref: composer/src/Composer/Installer/InstallationManager.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::react::promise;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::seld::signal::signal_handler::SignalHandler;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, array_search_mixed, array_splice, array_unshift, count,
    http_build_query, json_encode, str_contains, str_replace, strpos, strtolower, ucfirst,
};

use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::dependency_resolver::operation::mark_alias_installed_operation::MarkAliasInstalledOperation;
use crate::dependency_resolver::operation::mark_alias_uninstalled_operation::MarkAliasUninstalledOperation;
use crate::dependency_resolver::operation::operation_interface::OperationInterface;
use crate::dependency_resolver::operation::uninstall_operation::UninstallOperation;
use crate::dependency_resolver::operation::update_operation::UpdateOperation;
use crate::downloader::file_downloader::FileDownloader;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::installer::binary_presence_interface::BinaryPresenceInterface;
use crate::installer::installer_interface::InstallerInterface;
use crate::installer::package_events::PackageEvents;
use crate::installer::plugin_installer::PluginInstaller;
use crate::io::console_io::ConsoleIO;
use crate::io::io_interface::IOInterface;
use crate::package::alias_package::AliasPackage;
use crate::package::package_interface::PackageInterface;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::util::loop_::Loop;
use crate::util::platform::Platform;

/// Package operation manager.
#[derive(Debug)]
pub struct InstallationManager {
    /// @var list<InstallerInterface>
    installers: Vec<Box<dyn InstallerInterface>>,
    /// @var array<string, InstallerInterface>
    cache: IndexMap<String, Box<dyn InstallerInterface>>,
    /// @var array<string, array<PackageInterface>>
    notifiable_packages: IndexMap<String, Vec<Box<dyn PackageInterface>>>,
    loop_: Loop,
    io: Box<dyn IOInterface>,
    event_dispatcher: Option<EventDispatcher>,
    output_progress: bool,
}

impl InstallationManager {
    pub fn new(
        loop_: Loop,
        io: Box<dyn IOInterface>,
        event_dispatcher: Option<EventDispatcher>,
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
        // TODO(phase-b): FileDownloader::$downloadMetadata is a static property
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
            array_splice(&mut self.installers, k as i64, Some(1), None);
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
            // TODO(phase-b): $installer instanceof PluginInstaller downcast
            let plugin_installer: Option<&mut PluginInstaller> = None;
            let _ = plugin_installer;
            // if let Some(pi) = plugin_installer { pi.disable_plugins(); }
        }
    }

    /// Returns installer for a specific package type.
    ///
    /// @param string $type package type
    ///
    /// @throws \InvalidArgumentException if installer for provided type is not registered
    pub fn get_installer(&mut self, r#type: &str) -> Result<&dyn InstallerInterface> {
        let r#type = strtolower(r#type);

        if self.cache.contains_key(&r#type) {
            return Ok(self.cache.get(&r#type).unwrap().as_ref());
        }

        for installer in &self.installers {
            if installer.supports(&r#type) {
                // TODO(phase-b): cache by cloning Box<dyn InstallerInterface> is non-trivial
                self.cache.insert(r#type.clone(), installer.clone_box());
                return Ok(self.cache.get(&r#type).unwrap().as_ref());
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
        package: &dyn PackageInterface,
    ) -> Result<bool> {
        // TODO(phase-b): $package instanceof AliasPackage downcast
        let package_as_alias: Option<&AliasPackage> = None;
        if let Some(alias) = package_as_alias {
            return Ok(repo.has_package(package)
                && self.is_package_installed(repo, alias.get_alias_of())?);
        }

        Ok(self
            .get_installer(package.get_type())?
            .is_installed(repo, package))
    }

    /// Install binary for the given package.
    /// If the installer associated to this package doesn't handle that function, it'll do nothing.
    pub fn ensure_binaries_presence(&mut self, package: &dyn PackageInterface) {
        let installer = self.get_installer(package.get_type());
        let installer = match installer {
            Ok(i) => i,
            Err(_e) => {
                // no installer found for the current package type (@see `getInstaller()`)
                return;
            }
        };

        // if the given installer support installing binaries
        // TODO(phase-b): $installer instanceof BinaryPresenceInterface downcast
        let bp: Option<&dyn BinaryPresenceInterface> = None;
        if let Some(bp) = bp {
            bp.ensure_binaries_presence(package);
        }
        let _ = installer;
    }

    /// Executes solver operation.
    pub fn execute(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: Vec<Box<dyn OperationInterface>>,
        dev_mode: bool,
        run_scripts: bool,
        download_only: bool,
    ) -> Result<()> {
        // @var array<callable(): ?PromiseInterface<void|null>> $cleanupPromises
        let mut cleanup_promises: IndexMap<
            i64,
            Box<dyn Fn() -> Option<Box<dyn PromiseInterface>>>,
        > = IndexMap::new();

        let signal_handler = SignalHandler::create(
            vec![
                SignalHandler::SIGINT,
                SignalHandler::SIGTERM,
                SignalHandler::SIGHUP,
            ],
            // TODO(phase-b): closure captures &mut self via &mut cleanup_promises
            Box::new(move |signal: String, handler: &SignalHandler| {
                // TODO(phase-b): self.io.write_error(...); self.run_cleanup(&cleanup_promises);
                let _ = signal;
                handler.exit_with_last_signal();
            }),
        );

        let result: Result<()> = (|| -> Result<()> {
            // execute operations in batches to make sure download-modifying-plugins are installed
            // before the other packages get downloaded
            let mut batches: Vec<IndexMap<i64, Box<dyn OperationInterface>>> = vec![];
            let mut batch: IndexMap<i64, Box<dyn OperationInterface>> = IndexMap::new();
            for (index, operation) in operations.into_iter().enumerate() {
                let index = index as i64;
                // TODO(phase-b): instanceof downcasts for UpdateOperation/InstallOperation
                let is_update_or_install = false;
                if is_update_or_install {
                    let package: Option<&dyn PackageInterface> = None;
                    let _ = package;
                    let extra: IndexMap<String, PhpMixed> = IndexMap::new();
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
                self.download_and_execute_batch(
                    repo,
                    batch_to_execute,
                    &mut cleanup_promises,
                    dev_mode,
                    run_scripts,
                    download_only,
                    // TODO(phase-b): allOperations should be the original full list; would require clone
                    vec![],
                )?;
            }

            Ok(())
        })();

        // finally
        signal_handler.unregister();

        match result {
            Ok(()) => {}
            Err(e) => {
                self.run_cleanup(&cleanup_promises);
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
    fn download_and_execute_batch(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: IndexMap<i64, Box<dyn OperationInterface>>,
        cleanup_promises: &mut IndexMap<i64, Box<dyn Fn() -> Option<Box<dyn PromiseInterface>>>>,
        dev_mode: bool,
        run_scripts: bool,
        download_only: bool,
        all_operations: Vec<Box<dyn OperationInterface>>,
    ) -> Result<()> {
        let mut promises: Vec<Box<dyn PromiseInterface>> = vec![];

        for (index, operation) in &operations {
            let op_type = operation.get_operation_type();

            // ignoring alias ops as they don't need to execute anything at this stage
            if !["update", "install", "uninstall"].contains(&op_type.as_str()) {
                continue;
            }

            let package: &dyn PackageInterface;
            let initial_package: Option<&dyn PackageInterface>;
            // TODO(phase-b): downcast for UpdateOperation / Install/Mark/Uninstall variants
            let update_op: Option<&UpdateOperation> = None;
            if op_type == "update" {
                // @var UpdateOperation $operation
                if let Some(u) = update_op {
                    package = u.get_target_package();
                    initial_package = Some(u.get_initial_package());
                } else {
                    continue;
                }
            } else {
                // @var InstallOperation|MarkAliasInstalledOperation|MarkAliasUninstalledOperation|UninstallOperation $operation
                package = operation.get_package();
                initial_package = None;
            }
            let installer = self.get_installer(package.get_type())?;

            // TODO(phase-b): closure captures installer + package; needs Arc/Rc for shared state
            let _ = installer;
            let op_type_clone = op_type.clone();
            let cleanup: Box<dyn Fn() -> Option<Box<dyn PromiseInterface>>> =
                Box::new(move || -> Option<Box<dyn PromiseInterface>> {
                    // avoid calling cleanup if the download was not even initialized for a package
                    // as without installation source configured nothing will work
                    // TODO(phase-b): if (null === $package->getInstallationSource()) return \React\Promise\resolve(null);
                    let _ = &op_type_clone;
                    Some(promise::resolve(None))
                });
            cleanup_promises.insert(*index, cleanup);

            if op_type != "uninstall" {
                let installer = self.get_installer(package.get_type())?;
                let promise = installer.download(package, initial_package);
                if let Some(p) = promise {
                    promises.push(p);
                }
            }
        }

        // execute all downloads first
        if (promises.len() as i64) > 0 {
            self.wait_on_promises(promises);
        }

        if download_only {
            self.run_cleanup(cleanup_promises);

            return Ok(());
        }

        // execute operations in batches to make sure every plugin is installed in the
        // right order and activated before the packages depending on it are installed
        let mut batches: Vec<IndexMap<i64, Box<dyn OperationInterface>>> = vec![];
        let mut batch: IndexMap<i64, Box<dyn OperationInterface>> = IndexMap::new();
        for (index, operation) in operations {
            // TODO(phase-b): instanceof InstallOperation/UpdateOperation downcasts
            let is_install_or_update = false;
            if is_install_or_update {
                // TODO(phase-b): package type check (composer-plugin / composer-installer)
                let pkg_type = "";
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
            )?;
        }

        Ok(())
    }

    /// @param OperationInterface[] $operations    List of operations to execute in this batch
    /// @param OperationInterface[] $allOperations Complete list of operations to be executed in the install job, used for event listeners
    /// @phpstan-param array<callable(): ?PromiseInterface<void|null>> $cleanupPromises
    fn execute_batch(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operations: IndexMap<i64, Box<dyn OperationInterface>>,
        cleanup_promises: &IndexMap<i64, Box<dyn Fn() -> Option<Box<dyn PromiseInterface>>>>,
        dev_mode: bool,
        run_scripts: bool,
        all_operations: &[Box<dyn OperationInterface>],
    ) -> Result<()> {
        let mut promises: Vec<Box<dyn PromiseInterface>> = vec![];
        let mut post_exec_callbacks: Vec<Box<dyn Fn()>> = vec![];

        for (index, operation) in operations {
            let op_type = operation.get_operation_type();

            // ignoring alias ops as they don't need to execute anything
            if !["update", "install", "uninstall"].contains(&op_type.as_str()) {
                // output alias ops in debug verbosity as they have no output otherwise
                if self.io.is_debug() {
                    self.io.write_error(
                        PhpMixed::String(format!("  - {}", operation.show(false))),
                        true,
                        IOInterface::NORMAL,
                    );
                }
                // PHP: $this->{$opType}($repo, $operation);
                match op_type.as_str() {
                    "markAliasInstalled" => {
                        // TODO(phase-b): downcast operation to MarkAliasInstalledOperation
                    }
                    "markAliasUninstalled" => {
                        // TODO(phase-b): downcast operation to MarkAliasUninstalledOperation
                    }
                    _ => {}
                }

                continue;
            }

            let package: &dyn PackageInterface;
            let initial_package: Option<&dyn PackageInterface>;
            let update_op: Option<&UpdateOperation> = None;
            if op_type == "update" {
                if let Some(u) = update_op {
                    package = u.get_target_package();
                    initial_package = Some(u.get_initial_package());
                } else {
                    continue;
                }
            } else {
                package = operation.get_package();
                initial_package = None;
            }

            let installer = self.get_installer(package.get_type())?;

            let event_name = match op_type.as_str() {
                "install" => PackageEvents::PRE_PACKAGE_INSTALL,
                "update" => PackageEvents::PRE_PACKAGE_UPDATE,
                "uninstall" => PackageEvents::PRE_PACKAGE_UNINSTALL,
                _ => "",
            };

            if run_scripts && self.event_dispatcher.is_some() {
                self.event_dispatcher
                    .as_mut()
                    .unwrap()
                    .dispatch_package_event(
                        event_name,
                        dev_mode,
                        repo,
                        all_operations,
                        operation.as_ref(),
                    );
            }

            let _dispatcher = self.event_dispatcher.as_ref();
            let _io = self.io.as_ref();

            let promise = installer.prepare(&op_type, package, initial_package);
            let promise = match promise {
                Some(p) => p,
                None => promise::resolve(None),
            };

            // TODO(phase-b): chain `.then(cb1).then(cb2)` with cleanup_promises[index], repo.write, etc.
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

            promises.push(promise);
        }

        // execute all prepare => installs/updates/removes => cleanup steps
        if (promises.len() as i64) > 0 {
            self.wait_on_promises(promises);
        }

        Platform::workaround_filesystem_issues();

        for cb in &post_exec_callbacks {
            cb();
        }

        Ok(())
    }

    /// @param array<PromiseInterface<void|null>> $promises
    fn wait_on_promises(&mut self, promises: Vec<Box<dyn PromiseInterface>>) {
        let mut progress: Option<()> = None;
        // TODO(phase-b): self.io instanceof ConsoleIO downcast
        let io_is_console = false;
        if self.output_progress
            && io_is_console
            && Platform::get_env("CI").is_none()
            && !self.io.is_debug()
            && (promises.len() as i64) > 1
        {
            // TODO(phase-b): progress = self.io.get_progress_bar();
            progress = Some(());
        }
        self.loop_.wait(promises, progress);
        if progress.is_some() {
            // progress.clear();
            // ProgressBar in non-decorated output does not output a final line-break and clear() does nothing
            if !self.io.is_decorated() {
                self.io
                    .write_error(PhpMixed::String(String::new()), true, IOInterface::NORMAL);
            }
        }
    }

    /// Executes download operation.
    ///
    /// @phpstan-return PromiseInterface<void|null>|null
    pub fn download(
        &mut self,
        package: &dyn PackageInterface,
    ) -> Option<Box<dyn PromiseInterface>> {
        let installer = self.get_installer(package.get_type()).ok()?;
        let promise = installer.cleanup("install", package, None);

        promise
    }

    /// Executes install operation.
    ///
    /// @phpstan-return PromiseInterface<void|null>|null
    pub fn install(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &InstallOperation,
    ) -> Option<Box<dyn PromiseInterface>> {
        let package = operation.get_package();
        let installer = self.get_installer(package.get_type()).ok()?;
        let promise = installer.install(repo, package);
        self.mark_for_notification(package);

        promise
    }

    /// Executes update operation.
    ///
    /// @phpstan-return PromiseInterface<void|null>|null
    pub fn update(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &UpdateOperation,
    ) -> Option<Box<dyn PromiseInterface>> {
        let initial = operation.get_initial_package();
        let target = operation.get_target_package();

        let initial_type = initial.get_type();
        let target_type = target.get_type();

        let promise = if initial_type == target_type {
            let installer = self.get_installer(initial_type).ok()?;
            let promise = installer.update(repo, initial, target);
            self.mark_for_notification(target);
            promise
        } else {
            let promise = self
                .get_installer(initial_type)
                .ok()?
                .uninstall(repo, initial);
            let promise = match promise {
                Some(p) => p,
                None => promise::resolve(None),
            };

            let target_type = target_type.to_string();
            // TODO(phase-b): promise.then(closure capturing self/installer)
            let _ = target_type;
            Some(promise)
        };

        promise
    }

    /// Uninstalls package.
    ///
    /// @phpstan-return PromiseInterface<void|null>|null
    pub fn uninstall(
        &mut self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &UninstallOperation,
    ) -> Option<Box<dyn PromiseInterface>> {
        let package = operation.get_package();
        let installer = self.get_installer(package.get_type()).ok()?;

        installer.uninstall(repo, package)
    }

    /// Executes markAliasInstalled operation.
    pub fn mark_alias_installed(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &MarkAliasInstalledOperation,
    ) {
        let package = operation.get_package();

        if !repo.has_package(package) {
            repo.add_package(package.clone_box());
        }
    }

    /// Executes markAlias operation.
    pub fn mark_alias_uninstalled(
        &self,
        repo: &mut dyn InstalledRepositoryInterface,
        operation: &MarkAliasUninstalledOperation,
    ) {
        let package = operation.get_package();

        repo.remove_package(package);
    }

    /// Returns the installation path of a package
    ///
    /// @return string|null absolute path to install to, which does not end with a slash, or null if the package does not have anything installed on disk
    pub fn get_install_path(&mut self, package: &dyn PackageInterface) -> Option<String> {
        let installer = self.get_installer(package.get_type()).ok()?;

        installer.get_install_path(package)
    }

    pub fn set_output_progress(&mut self, output_progress: bool) {
        self.output_progress = output_progress;
    }

    pub fn notify_installs(&mut self, _io: &dyn IOInterface) {
        let mut promises: Vec<Box<dyn PromiseInterface>> = vec![];

        let result: Result<()> = (|| -> Result<()> {
            for (repo_url, packages) in &self.notifiable_packages {
                // non-batch API, deprecated
                if str_contains(repo_url, "%package%") {
                    for package in packages {
                        let url = str_replace("%package%", package.get_pretty_name(), repo_url);

                        let mut params: IndexMap<String, String> = IndexMap::new();
                        params.insert(
                            "version".to_string(),
                            package.get_pretty_version().to_string(),
                        );
                        params.insert(
                            "version_normalized".to_string(),
                            package.get_version().to_string(),
                        );
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
                        http.insert(
                            "content".to_string(),
                            PhpMixed::String(http_build_query(&params, "", Some("&"))),
                        );
                        http.insert("timeout".to_string(), PhpMixed::Int(3));
                        opts.insert(
                            "http".to_string(),
                            PhpMixed::Array(
                                http.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                            ),
                        );

                        promises.push(self.loop_.get_http_downloader().add(
                            &url,
                            &PhpMixed::Array(
                                opts.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                            ),
                        ));
                    }

                    continue;
                }

                let mut post_data: IndexMap<String, PhpMixed> = IndexMap::new();
                post_data.insert("downloads".to_string(), PhpMixed::List(vec![]));
                for package in packages {
                    let mut package_notification: IndexMap<String, PhpMixed> = IndexMap::new();
                    package_notification.insert(
                        "name".to_string(),
                        PhpMixed::String(package.get_pretty_name().to_string()),
                    );
                    package_notification.insert(
                        "version".to_string(),
                        PhpMixed::String(package.get_version().to_string()),
                    );
                    if strpos(repo_url, "packagist.org/").is_some() {
                        if let Some(metadata) =
                            FileDownloader::download_metadata().get(package.get_name())
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

                promises.push(self.loop_.get_http_downloader().add(
                    repo_url,
                    &PhpMixed::Array(opts.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
                ));
            }

            self.loop_.wait(promises, None);

            Ok(())
        })();
        // PHP swallows the exception silently here
        let _ = result;

        self.reset();
    }

    fn mark_for_notification(&mut self, package: &dyn PackageInterface) {
        if let Some(notification_url) = package.get_notification_url() {
            self.notifiable_packages
                .entry(notification_url.to_string())
                .or_insert_with(Vec::new)
                .push(package.clone_box());
        }
    }

    /// @phpstan-param array<callable(): ?PromiseInterface<void|null>> $cleanupPromises
    fn run_cleanup(
        &mut self,
        cleanup_promises: &IndexMap<i64, Box<dyn Fn() -> Option<Box<dyn PromiseInterface>>>>,
    ) {
        let mut promises: Vec<Box<dyn PromiseInterface>> = vec![];

        self.loop_.abort_jobs();

        for (_, cleanup) in cleanup_promises {
            // TODO(phase-b): React\Promise\Promise constructor with executor; emulate by wrapping cleanup()
            let promise = cleanup();
            if let Some(p) = promise {
                promises.push(p);
            } else {
                promises.push(promise::resolve(None));
            }
        }

        if (promises.len() as i64) > 0 {
            self.loop_.wait(promises, None);
        }
    }
}
