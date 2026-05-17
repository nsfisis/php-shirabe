//! ref: composer/src/Composer/Downloader/PathDownloader.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::symfony::component::filesystem::exception::io_exception::IOException;
use shirabe_external_packages::symfony::component::filesystem::filesystem::Filesystem as SymfonyFilesystem;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, PHP_WINDOWS_VERSION_MAJOR, PHP_WINDOWS_VERSION_MINOR, PhpMixed,
    RuntimeException, file_exists, function_exists, is_dir, realpath,
};

use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::dependency_resolver::operation::uninstall_operation::UninstallOperation;
use crate::downloader::file_downloader::FileDownloader;
use crate::downloader::vcs_capable_downloader_interface::VcsCapableDownloaderInterface;
use crate::io::io_interface::IOInterface;
use crate::package::archiver::archivable_files_finder::ArchivableFilesFinder;
use crate::package::dumper::array_dumper::ArrayDumper;
use crate::package::package_interface::PackageInterface;
use crate::package::version::version_guesser::VersionGuesser;
use crate::package::version::version_parser::VersionParser;
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;

#[derive(Debug)]
pub struct PathDownloader {
    pub(crate) inner: FileDownloader,
}

impl PathDownloader {
    const STRATEGY_SYMLINK: i64 = 10;
    const STRATEGY_MIRROR: i64 = 20;

    pub fn download(
        &mut self,
        package: &dyn PackageInterface,
        path: String,
        _prev_package: Option<&dyn PackageInterface>,
        _output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        let path = Filesystem::trim_trailing_slash(&path);
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
            return Ok(shirabe_external_packages::react::promise::resolve(None));
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

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    pub fn install(
        &mut self,
        package: &dyn PackageInterface,
        path: String,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        let path = Filesystem::trim_trailing_slash(&path);
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
                let appendix = self.get_install_operation_appendix(package, &path)?;
                self.inner.io.write_error(
                    PhpMixed::String(format!(
                        "  - {}{}",
                        InstallOperation::format(package, false),
                        appendix
                    )),
                    true,
                    io_interface::NORMAL,
                );
            }

            return Ok(shirabe_external_packages::react::promise::resolve(None));
        }

        // Get the transport options with default values
        let mut transport_options = package.get_transport_options();
        transport_options
            .entry("relative".to_string())
            .or_insert(PhpMixed::Bool(true));

        let (mut current_strategy, allowed_strategies) =
            self.compute_allowed_strategies(&transport_options)?;

        let symfony_filesystem = SymfonyFilesystem::new();
        self.inner.filesystem.remove_directory(&path);

        if output {
            self.inner.io.write_error(
                PhpMixed::String(format!(
                    "  - {}: ",
                    InstallOperation::format(package, false)
                )),
                false,
                io_interface::NORMAL,
            );
        }

        let mut is_fallback = false;
        if Self::STRATEGY_SYMLINK == current_strategy {
            let symlink_result: Result<Result<(), IOException>> = (|| {
                if Platform::is_windows() {
                    // Implement symlinks as NTFS junctions on Windows
                    if output {
                        self.inner.io.write_error(
                            PhpMixed::String(format!("Junctioning from {}", url)),
                            false,
                            io_interface::NORMAL,
                        );
                    }
                    Ok(self.inner.filesystem.junction(&real_url, &path))
                } else {
                    let path = path.trim_end_matches('/').to_string();
                    if output {
                        self.inner.io.write_error(
                            PhpMixed::String(format!("Symlinking from {}", url)),
                            false,
                            io_interface::NORMAL,
                        );
                    }
                    if transport_options
                        .get("relative")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        let absolute_path = if !self.inner.filesystem.is_absolute_path(&path) {
                            format!(
                                "{}{}{}",
                                Platform::get_cwd(false),
                                DIRECTORY_SEPARATOR,
                                path
                            )
                        } else {
                            path.clone()
                        };
                        let shortest_path = self.inner.filesystem.find_shortest_path(
                            &absolute_path,
                            &real_url,
                            false,
                            true,
                        );
                        Ok(symfony_filesystem.symlink(&format!("{}/", shortest_path), &path))
                    } else {
                        Ok(symfony_filesystem.symlink(&format!("{}/", real_url), &path))
                    }
                }
            })();

            match symlink_result? {
                Ok(()) => {}
                Err(_e) => {
                    if allowed_strategies.contains(&Self::STRATEGY_MIRROR) {
                        if output {
                            self.inner.io.write_error(
                                PhpMixed::String("".to_string()),
                                true,
                                io_interface::NORMAL,
                            );
                            self.inner.io.write_error(
                                PhpMixed::String(
                                    "    <error>Symlink failed, fallback to use mirroring!</error>"
                                        .to_string(),
                                ),
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
            let real_url = self.inner.filesystem.normalize_path(&real_url);

            if output {
                self.inner.io.write_error(
                    PhpMixed::String(format!(
                        "{}Mirroring from {}",
                        if is_fallback { "    " } else { "" },
                        url
                    )),
                    false,
                    io_interface::NORMAL,
                );
            }
            let iterator = ArchivableFilesFinder::new(&real_url, vec![]);
            symfony_filesystem.mirror(&real_url, &path, Some(&iterator));
        }

        if output {
            self.inner
                .io
                .write_error(PhpMixed::String("".to_string()), true, io_interface::NORMAL);
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    pub fn remove(
        &mut self,
        package: &dyn PackageInterface,
        path: String,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        let path = Filesystem::trim_trailing_slash(&path);
        // realpath() may resolve Windows junctions to the source path, so we'll check for a junction
        // first to prevent a false positive when checking if the dist and install paths are the same.
        // See https://bugs.php.net/bug.php?id=77639
        //
        // For junctions don't blindly rely on Filesystem::removeDirectory as it may be overzealous. If a
        // process inadvertently locks the file the removal will fail, but it would fall back to recursive
        // delete which is disastrous within a junction. So in that case we have no other real choice but
        // to fail hard.
        if Platform::is_windows() && self.inner.filesystem.is_junction(&path) {
            if output {
                self.inner.io.write_error(
                    PhpMixed::String(format!(
                        "  - {}, source is still present in {}",
                        UninstallOperation::format(package, false),
                        path
                    )),
                    true,
                    io_interface::NORMAL,
                );
            }
            if !self.inner.filesystem.remove_junction(&path) {
                self.inner.io.write_error(
                    PhpMixed::String(format!(
                        "    <warning>Could not remove junction at {} - is another process locking it?</warning>",
                        path
                    )),
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

            return Ok(shirabe_external_packages::react::promise::resolve(None));
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
        let fs = Filesystem::new();
        let abs_path = if fs.is_absolute_path(&path) {
            path.clone()
        } else {
            format!("{}/{}", Platform::get_cwd(false), path)
        };
        let abs_dist_url = if fs.is_absolute_path(&url) {
            url.clone()
        } else {
            format!("{}/{}", Platform::get_cwd(false), url)
        };
        if fs.normalize_path(&abs_path) == fs.normalize_path(&abs_dist_url) {
            if output {
                self.inner.io.write_error(
                    PhpMixed::String(format!(
                        "  - {}, source is still present in {}",
                        UninstallOperation::format(package, false),
                        path
                    )),
                    true,
                    io_interface::NORMAL,
                );
            }

            return Ok(shirabe_external_packages::react::promise::resolve(None));
        }

        self.inner.remove(package, &path, output)
    }

    pub fn get_vcs_reference(&self, package: &dyn PackageInterface, path: &str) -> Option<String> {
        let path = Filesystem::trim_trailing_slash(path);
        let parser = VersionParser::new();
        let guesser = VersionGuesser::new(
            &self.inner.config,
            &self.inner.process,
            &parser,
            Some(&*self.inner.io),
        );
        let dumper = ArrayDumper::new();

        let package_config = dumper.dump(package);
        let package_version = guesser.guess_version(&package_config, &path);
        if let Some(version) = package_version {
            return version
                .get("commit")
                .and_then(|v| v.as_string())
                .map(|s| s.to_owned());
        }

        None
    }

    pub(crate) fn get_install_operation_appendix(
        &self,
        package: &dyn PackageInterface,
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
        if mirror_path_repos.map_or(false, |v| !v.is_empty()) {
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
    fn get_vcs_reference(&self, package: &dyn PackageInterface, path: String) -> Option<String> {
        PathDownloader::get_vcs_reference(self, package, &path)
    }
}
