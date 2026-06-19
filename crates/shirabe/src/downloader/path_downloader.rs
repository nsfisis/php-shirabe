//! ref: composer/src/Composer/Downloader/PathDownloader.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::symfony::filesystem::Filesystem as SymfonyFilesystem;
use shirabe_external_packages::symfony::filesystem::exception::IOException;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, PHP_WINDOWS_VERSION_MAJOR, PHP_WINDOWS_VERSION_MINOR, PhpMixed,
    RuntimeException, file_exists, function_exists, is_dir, realpath,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::downloader::ChangeReportInterface;
use crate::downloader::DownloaderInterface;
use crate::downloader::FileDownloader;
use crate::downloader::VcsCapableDownloaderInterface;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::package::archiver::ArchivableFilesFinder;
use crate::package::dumper::ArrayDumper;
use crate::package::version::VersionGuesser;
use crate::package::version::VersionParser;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::Platform;
use crate::util::ProcessExecutor;

#[derive(Debug)]
pub struct PathDownloader {
    pub(crate) inner: FileDownloader,
}

impl PathDownloader {
    const STRATEGY_SYMLINK: i64 = 10;
    const STRATEGY_MIRROR: i64 = 20;

    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        cache: Option<std::rc::Rc<std::cell::RefCell<Cache>>>,
        filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> Self {
        Self {
            inner: FileDownloader::new(
                io,
                config,
                http_downloader,
                event_dispatcher,
                cache,
                Some(filesystem),
                Some(process),
            ),
        }
    }

    pub fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: &str) -> Option<String> {
        let path = Filesystem::trim_trailing_slash(path);
        let parser = VersionParser::new();
        let mut guesser = VersionGuesser::new(
            self.inner.config.clone(),
            self.inner.process.clone(),
            parser.clone(),
            Some(self.inner.io.clone()),
        );
        let dumper = ArrayDumper::new();

        let package_config = dumper.dump(package.clone());
        let package_version = guesser.guess_version(&package_config, &path);
        if let Ok(Some(version)) = package_version {
            return version.commit;
        }

        None
    }

    pub(crate) fn get_install_operation_appendix(
        &self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<String> {
        let url = package.get_dist_url().ok_or_else(|| RuntimeException {
            message: format!(
                "The package {} has no dist url configured, cannot install.",
                package.get_pretty_name()
            ),
            code: 0,
        })?;
        let real_url = realpath(&url).ok_or_else(|| RuntimeException {
            message: format!("Failed to realpath {}", url),
            code: 0,
        })?;

        if realpath(path).as_deref() == Some(&real_url) {
            return Ok(": Source already present".to_string());
        }

        let (current_strategy, _) =
            self.compute_allowed_strategies(&package.get_transport_options())?;

        if current_strategy == Self::STRATEGY_SYMLINK {
            if Platform::is_windows() {
                return Ok(format!(
                    ": Junctioning from {}",
                    package.get_dist_url().unwrap_or_default()
                ));
            }

            return Ok(format!(
                ": Symlinking from {}",
                package.get_dist_url().unwrap_or_default()
            ));
        }

        Ok(format!(
            ": Mirroring from {}",
            package.get_dist_url().unwrap_or_default()
        ))
    }

    fn compute_allowed_strategies(
        &self,
        transport_options: &IndexMap<String, PhpMixed>,
    ) -> Result<(i64, Vec<i64>)> {
        // When symlink transport option is null, both symlink and mirror are allowed
        let mut current_strategy = Self::STRATEGY_SYMLINK;
        let mut allowed_strategies = vec![Self::STRATEGY_SYMLINK, Self::STRATEGY_MIRROR];

        let mirror_path_repos = Platform::get_env("COMPOSER_MIRROR_PATH_REPOS");
        if mirror_path_repos.is_some_and(|v| !v.is_empty()) {
            current_strategy = Self::STRATEGY_MIRROR;
        }

        let symlink_option = transport_options.get("symlink");

        match symlink_option {
            Some(PhpMixed::Bool(true)) => {
                current_strategy = Self::STRATEGY_SYMLINK;
                allowed_strategies = vec![Self::STRATEGY_SYMLINK];
            }
            Some(PhpMixed::Bool(false)) => {
                current_strategy = Self::STRATEGY_MIRROR;
                allowed_strategies = vec![Self::STRATEGY_MIRROR];
            }
            _ => {}
        }

        // Check we can use junctions safely if we are on Windows
        if Platform::is_windows()
            && Self::STRATEGY_SYMLINK == current_strategy
            && !self.safe_junctions()
        {
            if !allowed_strategies.contains(&Self::STRATEGY_MIRROR) {
                return Err(RuntimeException {
                    message: "You are on an old Windows / old PHP combo which does not allow Composer to use junctions/symlinks and this path repository has symlink:true in its options so copying is not allowed".to_string(),
                    code: 0,
                }
                .into());
            }
            current_strategy = Self::STRATEGY_MIRROR;
            allowed_strategies = vec![Self::STRATEGY_MIRROR];
        }

        // Check we can use symlink() otherwise
        if !Platform::is_windows()
            && Self::STRATEGY_SYMLINK == current_strategy
            && !function_exists("symlink")
        {
            if !allowed_strategies.contains(&Self::STRATEGY_MIRROR) {
                return Err(RuntimeException {
                    message: "Your PHP has the symlink() function disabled which does not allow Composer to use symlinks and this path repository has symlink:true in its options so copying is not allowed".to_string(),
                    code: 0,
                }
                .into());
            }
            current_strategy = Self::STRATEGY_MIRROR;
            allowed_strategies = vec![Self::STRATEGY_MIRROR];
        }

        Ok((current_strategy, allowed_strategies))
    }

    // Returns true if junctions can be created and safely used on Windows.
    //
    // A PHP bug makes junction detection fragile, leading to possible data loss when removing a
    // package. See https://bugs.php.net/bug.php?id=77552
    //
    // For safety we require a minimum version of Windows 7, so we can call the system rmdir which
    // will preserve target content if given a junction.
    //
    // The PHP bug was fixed in 7.2.16 and 7.3.3 (requires at least Windows 7).
    fn safe_junctions(&self) -> bool {
        // We need to call mklink, and rmdir on Windows 7 (version 6.1)
        function_exists("proc_open")
            && (PHP_WINDOWS_VERSION_MAJOR > 6
                || (PHP_WINDOWS_VERSION_MAJOR == 6 && PHP_WINDOWS_VERSION_MINOR >= 1))
    }
}

impl VcsCapableDownloaderInterface for PathDownloader {
    fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: String) -> Option<String> {
        PathDownloader::get_vcs_reference(self, package, &path)
    }
}

impl crate::downloader::ChangeReportInterface for PathDownloader {
    fn get_local_changes(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        self.inner.get_local_changes(package, path)
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for PathDownloader {
    fn get_installation_source(&self) -> String {
        self.inner.get_installation_source()
    }

    fn as_change_report_interface(
        &mut self,
    ) -> Option<&mut dyn crate::downloader::ChangeReportInterface> {
        Some(self)
    }

    fn as_vcs_capable_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::VcsCapableDownloaderInterface> {
        Some(self)
    }

    async fn download(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        let path = Filesystem::trim_trailing_slash(path);
        let url = package.get_dist_url().ok_or_else(|| RuntimeException {
            message: format!(
                "The package {} has no dist url configured, cannot download.",
                package.get_pretty_name()
            ),
            code: 0,
        })?;
        let real_url = realpath(&url);
        if real_url.is_none()
            || !file_exists(real_url.as_deref().unwrap_or(""))
            || !is_dir(real_url.as_deref().unwrap_or(""))
        {
            return Err(RuntimeException {
                message: format!(
                    "Source path \"{}\" is not found for package {}",
                    url,
                    package.get_name()
                ),
                code: 0,
            }
            .into());
        }
        let real_url = real_url.unwrap();

        if realpath(&path).as_deref() == Some(&real_url) {
            return Ok(None);
        }

        if format!(
            "{}{}",
            realpath(&path).unwrap_or_default(),
            DIRECTORY_SEPARATOR
        )
        .starts_with(&format!("{}{}", real_url, DIRECTORY_SEPARATOR))
        {
            // IMPORTANT NOTICE: If you wish to change this, don't. You are wasting your time and ours.
            //
            // Please see https://github.com/composer/composer/pull/5974 and https://github.com/composer/composer/pull/6174
            // for previous attempts that were shut down because they did not work well enough or introduced too many risks.
            return Err(RuntimeException {
                message: format!(
                    "Package {} cannot install to \"{}\" inside its source at \"{}\"",
                    package.get_name(),
                    realpath(&path).unwrap_or_default(),
                    real_url
                ),
                code: 0,
            }
            .into());
        }

        Ok(None)
    }

    async fn prepare(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        self.inner
            .prepare(r#type, package, path, prev_package)
            .await
    }

    async fn install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        let path = Filesystem::trim_trailing_slash(path);
        let url = package.get_dist_url().ok_or_else(|| RuntimeException {
            message: format!(
                "The package {} has no dist url configured, cannot install.",
                package.get_pretty_name()
            ),
            code: 0,
        })?;
        let real_url = realpath(&url).ok_or_else(|| RuntimeException {
            message: format!("Failed to realpath {}", url),
            code: 0,
        })?;

        if realpath(&path).as_deref() == Some(&real_url) {
            if output {
                let appendix = self.get_install_operation_appendix(package.clone(), &path)?;
                self.inner.io.write_error3(
                    &format!(
                        "  - {}{}",
                        InstallOperation::format(package.clone(), false),
                        appendix
                    ),
                    true,
                    io_interface::NORMAL,
                );
            }

            return Ok(None);
        }

        // Get the transport options with default values
        let mut transport_options = package.get_transport_options();
        transport_options
            .entry("relative".to_string())
            .or_insert(PhpMixed::Bool(true));

        let (mut current_strategy, allowed_strategies) =
            self.compute_allowed_strategies(&transport_options)?;

        let symfony_filesystem = SymfonyFilesystem::new();
        self.inner.filesystem.borrow_mut().remove_directory(&path);

        if output {
            self.inner.io.write_error3(
                &format!("  - {}: ", InstallOperation::format(package, false)),
                false,
                io_interface::NORMAL,
            );
        }

        let mut is_fallback = false;
        if Self::STRATEGY_SYMLINK == current_strategy {
            let symlink_result: Result<anyhow::Result<()>> =
                (|| {
                    if Platform::is_windows() {
                        // Implement symlinks as NTFS junctions on Windows
                        if output {
                            self.inner.io.write_error3(
                                &format!("Junctioning from {}", url),
                                false,
                                io_interface::NORMAL,
                            );
                        }
                        Ok(self
                            .inner
                            .filesystem
                            .borrow_mut()
                            .junction(&real_url, &path))
                    } else {
                        let path = path.trim_end_matches('/').to_string();
                        if output {
                            self.inner.io.write_error3(
                                &format!("Symlinking from {}", url),
                                false,
                                io_interface::NORMAL,
                            );
                        }
                        if transport_options
                            .get("relative")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                        {
                            let absolute_path =
                                if !self.inner.filesystem.borrow_mut().is_absolute_path(&path) {
                                    format!(
                                        "{}{}{}",
                                        Platform::get_cwd(false)?,
                                        DIRECTORY_SEPARATOR,
                                        path
                                    )
                                } else {
                                    path.clone()
                                };
                            let shortest_path = self
                                .inner
                                .filesystem
                                .borrow_mut()
                                .find_shortest_path(&absolute_path, &real_url, false, true);
                            Ok(symfony_filesystem.symlink(
                                &format!("{}/", shortest_path),
                                &path,
                                false,
                            ))
                        } else {
                            Ok(symfony_filesystem.symlink(&format!("{}/", real_url), &path, false))
                        }
                    }
                })();

            match symlink_result? {
                Ok(()) => {}
                Err(_e) => {
                    if allowed_strategies.contains(&Self::STRATEGY_MIRROR) {
                        if output {
                            self.inner.io.write_error3("", true, io_interface::NORMAL);
                            self.inner.io.write_error3(
                                "    <error>Symlink failed, fallback to use mirroring!</error>",
                                true,
                                io_interface::NORMAL,
                            );
                        }
                        current_strategy = Self::STRATEGY_MIRROR;
                        is_fallback = true;
                    } else {
                        return Err(RuntimeException {
                            message: format!(
                                "Symlink from \"{}\" to \"{}\" failed!",
                                real_url, path
                            ),
                            code: 0,
                        }
                        .into());
                    }
                }
            }
        }

        // Fallback if symlink failed or if symlink is not allowed for the package
        if Self::STRATEGY_MIRROR == current_strategy {
            let real_url = self.inner.filesystem.borrow_mut().normalize_path(&real_url);

            if output {
                self.inner.io.write_error3(
                    &format!(
                        "{}Mirroring from {}",
                        if is_fallback { "    " } else { "" },
                        url
                    ),
                    false,
                    io_interface::NORMAL,
                );
            }
            let _iterator = ArchivableFilesFinder::new(&real_url, vec![], false)?;
            // PHP: $symfonyFilesystem->mirror($realUrl, $path, $iterator);
            // TODO(phase-c): Symfony Filesystem::mirror takes a Traversable iterator as its third
            // argument, but the external-package Filesystem stub does not model the iterator type
            // that ArchivableFilesFinder (an IteratorAggregate) would be wrapped into, so None is
            // passed and the mirrored file list is not restricted.
            symfony_filesystem.mirror(&real_url, &path, None, &IndexMap::new())?;
        }

        if output {
            self.inner.io.write_error3("", true, io_interface::NORMAL);
        }

        Ok(None)
    }

    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        self.inner.update(initial, target, path).await
    }

    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        let path = Filesystem::trim_trailing_slash(path);
        // realpath() may resolve Windows junctions to the source path, so we'll check for a junction
        // first to prevent a false positive when checking if the dist and install paths are the same.
        // See https://bugs.php.net/bug.php?id=77639
        //
        // For junctions don't blindly rely on Filesystem::removeDirectory as it may be overzealous. If a
        // process inadvertently locks the file the removal will fail, but it would fall back to recursive
        // delete which is disastrous within a junction. So in that case we have no other real choice but
        // to fail hard.
        if Platform::is_windows() && self.inner.filesystem.borrow_mut().is_junction(&path) {
            if output {
                self.inner.io.write_error3(
                    &format!(
                        "  - {}, source is still present in {}",
                        UninstallOperation::format(package.clone(), false),
                        path
                    ),
                    true,
                    io_interface::NORMAL,
                );
            }
            if !self.inner.filesystem.borrow_mut().remove_junction(&path)? {
                self.inner.io.write_error3(
                    &format!(
                        "    <warning>Could not remove junction at {} - is another process locking it?</warning>",
                        path
                    ),
                    true,
                    io_interface::NORMAL,
                );
                return Err(RuntimeException {
                    message: format!(
                        "Could not reliably remove junction for package {}",
                        package.get_name()
                    ),
                    code: 0,
                }
                .into());
            }

            return Ok(None);
        }

        let url = package.get_dist_url().ok_or_else(|| RuntimeException {
            message: format!(
                "The package {} has no dist url configured, cannot remove.",
                package.get_pretty_name()
            ),
            code: 0,
        })?;

        // ensure that the source path (dist url) is not the same as the install path, which
        // can happen when using custom installers, see https://github.com/composer/composer/pull/9116
        // not using realpath here as we do not want to resolve the symlink to the original dist url
        // it points to
        let fs = Filesystem::new(None);
        let abs_path = if fs.is_absolute_path(&path) {
            path.clone()
        } else {
            format!("{}/{}", Platform::get_cwd(false)?, path)
        };
        let abs_dist_url = if fs.is_absolute_path(&url) {
            url.to_string()
        } else {
            format!("{}/{}", Platform::get_cwd(false)?, url)
        };
        if fs.normalize_path(&abs_path) == fs.normalize_path(&abs_dist_url) {
            if output {
                self.inner.io.write_error3(
                    &format!(
                        "  - {}, source is still present in {}",
                        UninstallOperation::format(package.clone(), false),
                        path
                    ),
                    true,
                    io_interface::NORMAL,
                );
            }

            return Ok(None);
        }

        self.inner.remove(package, &path, output).await
    }

    async fn cleanup(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        self.inner
            .cleanup(r#type, package, path, prev_package)
            .await
    }
}
