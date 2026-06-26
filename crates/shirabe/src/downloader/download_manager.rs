//! ref: composer/src/Composer/Downloader/DownloadManager.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, RuntimeException, array_keys,
    array_reverse, array_shift, dirname, implode, in_array, preg_quote, rtrim, str_replace,
    strtolower, usort,
};

use crate::downloader::DownloaderInterface;
use crate::exception::IrrecoverableDownloadException;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::util::Filesystem;

/// Downloaders manager.
#[derive(Debug)]
pub struct DownloadManager {
    /// @var IOInterface
    pub(crate) io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    /// @var bool
    prefer_dist: bool,
    /// @var bool
    prefer_source: bool,
    /// @var array<string, string>
    package_preferences: IndexMap<String, String>,
    /// @var Filesystem
    filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    /// @var array<string, DownloaderInterface>
    downloaders: IndexMap<String, std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>>>,
}

impl DownloadManager {
    /// Initializes download manager.
    ///
    /// @param IOInterface     $io           The Input Output Interface
    /// @param bool            $preferSource prefer downloading from source
    /// @param Filesystem|null $filesystem   custom Filesystem object
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        prefer_source: bool,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
    ) -> Self {
        let filesystem = filesystem
            .unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))));
        Self {
            io,
            prefer_source,
            prefer_dist: false,
            package_preferences: IndexMap::new(),
            filesystem,
            downloaders: IndexMap::new(),
        }
    }

    /// Makes downloader prefer source installation over the dist.
    ///
    /// @param  bool            $preferSource prefer downloading from source
    pub fn set_prefer_source(&mut self, prefer_source: bool) -> &mut Self {
        self.prefer_source = prefer_source;

        self
    }

    /// Makes downloader prefer dist installation over the source.
    ///
    /// @param  bool            $preferDist prefer downloading from dist
    pub fn set_prefer_dist(&mut self, prefer_dist: bool) -> &mut Self {
        self.prefer_dist = prefer_dist;

        self
    }

    /// Sets fine tuned preference settings for package level source/dist selection.
    ///
    /// @param array<string, string> $preferences array of preferences by package patterns
    pub fn set_preferences(&mut self, preferences: IndexMap<String, String>) -> &mut Self {
        self.package_preferences = preferences;

        self
    }

    /// Sets installer downloader for a specific installation type.
    ///
    /// @param  string              $type       installation type
    /// @param  DownloaderInterface $downloader downloader instance
    pub fn set_downloader(
        &mut self,
        r#type: &str,
        downloader: std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>>,
    ) -> &mut Self {
        let r#type = strtolower(r#type);
        self.downloaders.insert(r#type, downloader);

        self
    }

    /// Returns downloader for a specific installation type.
    ///
    /// @param  string                    $type installation type
    /// @throws \InvalidArgumentException if downloader for provided type is not registered
    pub fn get_downloader(
        &self,
        r#type: &str,
    ) -> Result<std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>>> {
        let r#type = strtolower(r#type);
        if !self.downloaders.contains_key(&r#type) {
            return Err(InvalidArgumentException {
                message: format!(
                    "Unknown downloader type: {}. Available types: {}.",
                    r#type,
                    implode(", ", &array_keys(&self.downloaders)),
                ),
                code: 0,
            }
            .into());
        }

        Ok(self.downloaders.get(&r#type).unwrap().clone())
    }

    /// Returns downloader for already installed package.
    ///
    /// @param  PackageInterface          $package package instance
    /// @throws \InvalidArgumentException if package has no installation source specified
    /// @throws \LogicException           if specific downloader used to load package with
    ///                                           wrong type
    pub fn get_downloader_for_package(
        &self,
        package: PackageInterfaceHandle,
    ) -> Result<Option<std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>>>> {
        let installation_source = package.get_installation_source();

        if "metapackage" == package.get_type() {
            return Ok(None);
        }

        let downloader = if installation_source.as_deref() == Some("dist") {
            self.get_downloader(&package.get_dist_type().unwrap_or_default())?
        } else if installation_source.as_deref() == Some("source") {
            self.get_downloader(&package.get_source_type().unwrap_or_default())?
        } else {
            return Err(InvalidArgumentException {
                message: format!(
                    "Package {} does not have an installation source set",
                    package,
                ),
                code: 0,
            }
            .into());
        };

        let downloader_installation_source = downloader.borrow().get_installation_source();
        if installation_source.as_deref() != Some(&downloader_installation_source) {
            return Err(LogicException {
                message: format!(
                    "Downloader \"{}\" is a {} type downloader and can not be used to download {} for package {}",
                    shirabe_php_shim::get_class_obj(&*downloader.borrow()),
                    downloader_installation_source,
                    installation_source.clone().unwrap_or_default(),
                    package,
                ),
                code: 0,
            }
            .into());
        }

        Ok(Some(downloader))
    }

    pub fn get_downloader_type(
        &self,
        downloader: &std::rc::Rc<std::cell::RefCell<dyn DownloaderInterface>>,
    ) -> String {
        // PHP: array_search($downloader, $this->downloaders)
        for (r#type, candidate) in &self.downloaders {
            if std::rc::Rc::ptr_eq(candidate, downloader) {
                return r#type.clone();
            }
        }
        String::new()
    }

    /// Downloads package into target dir.
    ///
    /// @param PackageInterface      $package     package instance
    /// @param string                $targetDir   target dir
    /// @param PackageInterface|null $prevPackage previous package instance in case of updates
    /// @phpstan-return PromiseInterface<void|null>
    ///
    /// @throws \InvalidArgumentException if package have no urls to download from
    /// @throws \RuntimeException
    pub async fn download(
        &self,
        package: PackageInterfaceHandle,
        target_dir: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        let target_dir = self.normalize_target_dir(target_dir);
        self.filesystem
            .borrow_mut()
            .ensure_directory_exists(&dirname(&target_dir));

        let mut sources = self.get_available_sources(package.clone(), prev_package.clone())?;

        let mut retry = false;
        loop {
            let source = match array_shift(&mut sources) {
                Some(s) => s,
                None => {
                    return Ok(None);
                }
            };
            if retry {
                self.io.write_error3(
                    &format!(
                        "    <warning>Now trying to download from {}</warning>",
                        source,
                    ),
                    true,
                    io_interface::NORMAL,
                );
            }
            package.set_installation_source(Some(source.clone()));

            let Some(downloader) = self.get_downloader_for_package(package.clone())? else {
                return Ok(None);
            };

            let result = match downloader
                .borrow_mut()
                .download3(package.clone(), &target_dir, prev_package.clone())
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let is_runtime = e.downcast_ref::<RuntimeException>().is_some();
                    let is_irrecoverable =
                        e.downcast_ref::<IrrecoverableDownloadException>().is_some();
                    if is_runtime && !is_irrecoverable {
                        if sources.is_empty() {
                            return Err(e);
                        }

                        let message = e
                            .downcast_ref::<RuntimeException>()
                            .unwrap()
                            .message
                            .clone();
                        self.io.write_error3(
                            &format!(
                                "    <warning>Failed to download {} from {}: {}</warning>",
                                package.get_pretty_name(),
                                source,
                                message,
                            ),
                            true,
                            io_interface::NORMAL,
                        );

                        retry = true;
                        continue;
                    }

                    return Err(e);
                }
            };

            return Ok(result);
        }
    }

    /// Prepares an operation execution
    ///
    /// @param string                $type        one of install/update/uninstall
    /// @param PackageInterface      $package     package instance
    /// @param string                $targetDir   target dir
    /// @param PackageInterface|null $prevPackage previous package instance in case of updates
    /// @phpstan-return PromiseInterface<void|null>
    pub async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        target_dir: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        let target_dir = self.normalize_target_dir(target_dir);
        if let Some(downloader) = self.get_downloader_for_package(package.clone())? {
            return downloader
                .borrow_mut()
                .prepare(r#type, package, &target_dir, prev_package)
                .await;
        }

        Ok(None)
    }

    /// Installs package into target dir.
    ///
    /// @param PackageInterface $package   package instance
    /// @param string           $targetDir target dir
    /// @phpstan-return PromiseInterface<void|null>
    ///
    /// @throws \InvalidArgumentException if package have no urls to download from
    /// @throws \RuntimeException
    pub async fn install(
        &self,
        package: PackageInterfaceHandle,
        target_dir: &str,
    ) -> Result<Option<PhpMixed>> {
        let target_dir = self.normalize_target_dir(target_dir);
        if let Some(downloader) = self.get_downloader_for_package(package.clone())? {
            return downloader.borrow_mut().install2(package, &target_dir).await;
        }

        Ok(None)
    }

    /// Updates package from initial to target version.
    ///
    /// @param PackageInterface $initial   initial package version
    /// @param PackageInterface $target    target package version
    /// @param string           $targetDir target dir
    /// @phpstan-return PromiseInterface<void|null>
    ///
    /// @throws \InvalidArgumentException if initial package is not installed
    pub async fn update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        target_dir: &str,
    ) -> Result<Option<PhpMixed>> {
        let target_dir = self.normalize_target_dir(target_dir);
        let downloader = self.get_downloader_for_package(target.clone())?;
        let initial_downloader = self.get_downloader_for_package(initial.clone())?;

        // no downloaders present means update from metapackage to metapackage, nothing to do
        if initial_downloader.is_none() && downloader.is_none() {
            return Ok(None);
        }

        // if we have a downloader present before, but not after, the package became a metapackage and its files should be removed
        if downloader.is_none() {
            return initial_downloader
                .as_ref()
                .unwrap()
                .borrow_mut()
                .remove2(initial, &target_dir)
                .await;
        }

        let initial_type = self.get_downloader_type(initial_downloader.as_ref().unwrap());
        let target_type = self.get_downloader_type(downloader.as_ref().unwrap());
        if initial_type == target_type {
            match downloader
                .as_ref()
                .unwrap()
                .borrow_mut()
                .update(initial.clone(), target.clone(), &target_dir)
                .await
            {
                Ok(p) => return Ok(p),
                Err(e) => {
                    // PHP catches only \RuntimeException; other exceptions propagate uncaught.
                    if e.downcast_ref::<RuntimeException>().is_none() {
                        return Err(e);
                    }
                    if !self.io.is_interactive() {
                        return Err(e);
                    }
                    let message = e
                        .downcast_ref::<RuntimeException>()
                        .unwrap()
                        .message
                        .clone();
                    self.io.write_error3(
                        &format!("<error>    Update failed ({})</error>", message),
                        true,
                        io_interface::NORMAL,
                    );
                    if !self.io.ask_confirmation(
                        "    Would you like to try reinstalling the package instead [<comment>yes</comment>]? ".to_string(),
                        true,
                    ) {
                        return Err(e);
                    }
                }
            }
        }

        // if downloader type changed, or update failed and user asks for reinstall,
        // we wipe the dir and do a new install instead of updating it
        // PHP: return $promise->then(fn () => $this->install($target, $targetDir));
        let _ = initial_downloader
            .as_ref()
            .unwrap()
            .borrow_mut()
            .remove2(initial, &target_dir)
            .await?;
        self.install(target, &target_dir).await
    }

    /// Removes package from target dir.
    ///
    /// @param PackageInterface $package   package instance
    /// @param string           $targetDir target dir
    /// @phpstan-return PromiseInterface<void|null>
    pub async fn remove(
        &self,
        package: PackageInterfaceHandle,
        target_dir: &str,
    ) -> Result<Option<PhpMixed>> {
        let target_dir = self.normalize_target_dir(target_dir);
        if let Some(downloader) = self.get_downloader_for_package(package.clone())? {
            return downloader.borrow_mut().remove2(package, &target_dir).await;
        }

        Ok(None)
    }

    /// Cleans up a failed operation
    ///
    /// @param string                $type        one of install/update/uninstall
    /// @param PackageInterface      $package     package instance
    /// @param string                $targetDir   target dir
    /// @param PackageInterface|null $prevPackage previous package instance in case of updates
    /// @phpstan-return PromiseInterface<void|null>
    pub async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        target_dir: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        let target_dir = self.normalize_target_dir(target_dir);
        if let Some(downloader) = self.get_downloader_for_package(package.clone())? {
            return downloader
                .borrow_mut()
                .cleanup(r#type, package, &target_dir, prev_package)
                .await;
        }

        Ok(None)
    }

    /// Determines the install preference of a package
    ///
    /// @param PackageInterface $package package instance
    pub(crate) fn resolve_package_install_preference(
        &self,
        package: PackageInterfaceHandle,
    ) -> String {
        for (pattern, preference) in &self.package_preferences {
            let pattern_regex = format!(
                "{{^{}$}}i",
                str_replace("\\*", ".*", &preg_quote(pattern, None)),
            );
            if Preg::is_match(&pattern_regex, &package.get_name()) {
                if "dist" == preference || (!package.is_dev() && "auto" == preference) {
                    return "dist".to_string();
                }

                return "source".to_string();
            }
        }

        if package.is_dev() {
            "source".to_string()
        } else {
            "dist".to_string()
        }
    }

    /// @return string[]
    /// @phpstan-return array<'dist'|'source'>&non-empty-array
    fn get_available_sources(
        &self,
        package: PackageInterfaceHandle,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Vec<String>> {
        let source_type = package.get_source_type();
        let dist_type = package.get_dist_type();

        // add source before dist by default
        let mut sources: Vec<String> = vec![];
        if source_type.is_some() && !source_type.unwrap().is_empty() {
            sources.push("source".to_string());
        }
        if dist_type.is_some() && !dist_type.unwrap().is_empty() {
            sources.push("dist".to_string());
        }

        if sources.is_empty() {
            return Err(InvalidArgumentException {
                message: format!("Package {} must have a source or dist specified", package),
                code: 0,
            }
            .into());
        }

        if let Some(prev) = prev_package {
            // if we are updating, we want to keep the same source as the previously installed package (if available in the new one)
            let prev_source = prev.get_installation_source();
            if in_array(
                PhpMixed::String(prev_source.clone().unwrap_or_default()),
                &PhpMixed::List(
                    sources
                        .iter()
                        .map(|s| PhpMixed::String(s.clone()))
                        .collect(),
                ),
                true,
            )
                // unless the previous package was stable dist (by default) and the new package is dev, then we allow the new default to take over
                && !(!prev.is_dev()
                    && prev.get_installation_source().as_deref() == Some("dist")
                    && package.is_dev())
            {
                let prev_source_owned = prev_source.unwrap_or_default();
                usort(&mut sources, move |a: &String, b: &String| -> i64 {
                    if *a == prev_source_owned { -1 } else { 1 }
                });

                return Ok(sources);
            }
        }

        // reverse sources in case dist is the preferred source for this package
        if !self.prefer_source
            && (self.prefer_dist
                || "dist" == self.resolve_package_install_preference(package.clone()))
        {
            sources = array_reverse(&sources, false);
        }

        Ok(sources)
    }

    /// Downloaders expect a /path/to/dir without trailing slash
    ///
    /// If any Installer provides a path with a trailing slash, this can cause bugs so make sure we remove them
    fn normalize_target_dir(&self, dir: &str) -> String {
        if dir == "\\" || dir == "/" {
            return dir.to_string();
        }

        rtrim(dir, Some("\\/"))
    }
}
