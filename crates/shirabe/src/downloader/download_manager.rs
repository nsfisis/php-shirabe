//! ref: composer/src/Composer/Downloader/DownloadManager.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::{
    array_keys, array_reverse, array_shift, dirname, get_class, implode, in_array, preg_quote,
    rtrim, sprintf, str_replace, strtolower, usort, InvalidArgumentException, LogicException,
    PhpMixed, RuntimeException,
};

use crate::downloader::downloader_interface::DownloaderInterface;
use crate::downloader::irrecoverable_download_exception::IrrecoverableDownloadException;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::util::filesystem::Filesystem;

/// Downloaders manager.
#[derive(Debug)]
pub struct DownloadManager {
    /// @var IOInterface
    pub(crate) io: Box<dyn IOInterface>,
    /// @var bool
    prefer_dist: bool,
    /// @var bool
    prefer_source: bool,
    /// @var array<string, string>
    package_preferences: IndexMap<String, String>,
    /// @var Filesystem
    filesystem: Filesystem,
    /// @var array<string, DownloaderInterface>
    downloaders: IndexMap<String, Box<dyn DownloaderInterface>>,
}

impl DownloadManager {
    /// Initializes download manager.
    ///
    /// @param IOInterface     $io           The Input Output Interface
    /// @param bool            $preferSource prefer downloading from source
    /// @param Filesystem|null $filesystem   custom Filesystem object
    pub fn new(
        io: Box<dyn IOInterface>,
        prefer_source: bool,
        filesystem: Option<Filesystem>,
    ) -> Self {
        let filesystem = filesystem.unwrap_or_else(Filesystem::new);
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
        downloader: Box<dyn DownloaderInterface>,
    ) -> &mut Self {
        let r#type = strtolower(r#type);
        self.downloaders.insert(r#type, downloader);

        self
    }

    /// Returns downloader for a specific installation type.
    ///
    /// @param  string                    $type installation type
    /// @throws \InvalidArgumentException if downloader for provided type is not registered
    pub fn get_downloader(&self, r#type: &str) -> Result<&dyn DownloaderInterface> {
        let r#type = strtolower(r#type);
        if !self.downloaders.contains_key(&r#type) {
            return Err(InvalidArgumentException {
                message: sprintf(
                    "Unknown downloader type: %s. Available types: %s.",
                    &[
                        PhpMixed::String(r#type),
                        PhpMixed::String(implode(", ", &array_keys(&self.downloaders))),
                    ],
                ),
                code: 0,
            }
            .into());
        }

        Ok(self.downloaders.get(&r#type).unwrap().as_ref())
    }

    /// Returns downloader for already installed package.
    ///
    /// @param  PackageInterface          $package package instance
    /// @throws \InvalidArgumentException if package has no installation source specified
    /// @throws \LogicException           if specific downloader used to load package with
    ///                                           wrong type
    pub fn get_downloader_for_package(
        &self,
        package: &dyn PackageInterface,
    ) -> Result<Option<&dyn DownloaderInterface>> {
        let installation_source = package.get_installation_source();

        if "metapackage" == package.get_type() {
            return Ok(None);
        }

        let downloader = if installation_source == Some("dist") {
            self.get_downloader(package.get_dist_type().unwrap_or(""))?
        } else if installation_source == Some("source") {
            self.get_downloader(package.get_source_type().unwrap_or(""))?
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

        if installation_source != Some(&downloader.get_installation_source()) {
            return Err(LogicException {
                message: sprintf(
                    "Downloader \"%s\" is a %s type downloader and can not be used to download %s for package %s",
                    &[
                        PhpMixed::String(get_class(downloader)),
                        PhpMixed::String(downloader.get_installation_source()),
                        PhpMixed::String(installation_source.unwrap_or("").to_string()),
                        PhpMixed::String(package.to_string()),
                    ],
                ),
                code: 0,
            }
            .into());
        }

        Ok(Some(downloader))
    }

    pub fn get_downloader_type(&self, downloader: &dyn DownloaderInterface) -> String {
        // PHP: array_search($downloader, $this->downloaders)
        // TODO(phase-b): reference equality on Box<dyn DownloaderInterface>
        for (r#type, candidate) in &self.downloaders {
            if std::ptr::eq(
                candidate.as_ref() as *const dyn DownloaderInterface as *const (),
                downloader as *const dyn DownloaderInterface as *const (),
            ) {
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
    pub fn download(
        &self,
        package: &dyn PackageInterface,
        target_dir: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        let target_dir = self.normalize_target_dir(target_dir);
        self.filesystem.ensure_directory_exists(&dirname(&target_dir));

        let mut sources = self.get_available_sources(package, prev_package)?;

        // PHP closure: uses recursive variable $download and captures $sources by reference
        // TODO(phase-b): recursive closure with mutable shared state needs Rc<RefCell<>> or similar
        let mut retry_state = false;
        loop {
            let source = match array_shift(&mut sources) {
                Some(s) => s,
                None => {
                    return Ok(shirabe_external_packages::react::promise::resolve(None));
                }
            };
            if retry_state {
                self.io.write_error(
                    PhpMixed::String(format!(
                        "    <warning>Now trying to download from {}</warning>",
                        source,
                    )),
                    true,
                    IOInterface::NORMAL,
                );
            }
            // TODO(phase-b): &mut on shared package — PHP mutates by reference
            todo!("package.set_installation_source(Some(source.clone()))");

            let downloader = match self.get_downloader_for_package(package)? {
                Some(d) => d,
                None => {
                    return Ok(shirabe_external_packages::react::promise::resolve(None));
                }
            };

            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            let result = match downloader.download(package, &target_dir, prev_package) {
                Ok(r) => r,
                Err(e) => {
                    // PHP closure handleError: rethrow if not RuntimeException or if IrrecoverableDownloadException
                    // TODO(phase-b): downcast for instanceof checks
                    let is_runtime: bool = todo!("e instanceof RuntimeException");
                    let is_irrecoverable: bool = todo!("e instanceof IrrecoverableDownloadException");
                    if is_runtime && !is_irrecoverable {
                        if sources.is_empty() {
                            return Err(e);
                        }

                        self.io.write_error(
                            PhpMixed::String(format!(
                                "    <warning>Failed to download {} from {}: {}</warning>",
                                package.get_pretty_name(),
                                source,
                                e,
                            )),
                            true,
                            IOInterface::NORMAL,
                        );

                        retry_state = true;
                        continue;
                    }

                    return Err(e);
                }
            };

            // PHP: $result->then(static fn ($res) => $res, $handleError);
            // TODO(phase-b): chain $handleError as the rejection handler on the promise
            let res = result.then(Box::new(move |res: PhpMixed| -> Result<PhpMixed> { Ok(res) }));

            return Ok(res);
        }
    }

    /// Prepares an operation execution
    ///
    /// @param string                $type        one of install/update/uninstall
    /// @param PackageInterface      $package     package instance
    /// @param string                $targetDir   target dir
    /// @param PackageInterface|null $prevPackage previous package instance in case of updates
    /// @phpstan-return PromiseInterface<void|null>
    pub fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        target_dir: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        let target_dir = self.normalize_target_dir(target_dir);
        if let Some(downloader) = self.get_downloader_for_package(package)? {
            return downloader.prepare(r#type, package, &target_dir, prev_package);
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    /// Installs package into target dir.
    ///
    /// @param PackageInterface $package   package instance
    /// @param string           $targetDir target dir
    /// @phpstan-return PromiseInterface<void|null>
    ///
    /// @throws \InvalidArgumentException if package have no urls to download from
    /// @throws \RuntimeException
    pub fn install(
        &self,
        package: &dyn PackageInterface,
        target_dir: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        let target_dir = self.normalize_target_dir(target_dir);
        if let Some(downloader) = self.get_downloader_for_package(package)? {
            return downloader.install(package, &target_dir);
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    /// Updates package from initial to target version.
    ///
    /// @param PackageInterface $initial   initial package version
    /// @param PackageInterface $target    target package version
    /// @param string           $targetDir target dir
    /// @phpstan-return PromiseInterface<void|null>
    ///
    /// @throws \InvalidArgumentException if initial package is not installed
    pub fn update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        target_dir: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        let target_dir = self.normalize_target_dir(target_dir);
        let downloader = self.get_downloader_for_package(target)?;
        let initial_downloader = self.get_downloader_for_package(initial)?;

        // no downloaders present means update from metapackage to metapackage, nothing to do
        if initial_downloader.is_none() && downloader.is_none() {
            return Ok(shirabe_external_packages::react::promise::resolve(None));
        }

        // if we have a downloader present before, but not after, the package became a metapackage and its files should be removed
        if downloader.is_none() {
            return initial_downloader.unwrap().remove(initial, &target_dir);
        }

        let initial_type = self.get_downloader_type(initial_downloader.unwrap());
        let target_type = self.get_downloader_type(downloader.unwrap());
        if initial_type == target_type {
            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            match downloader.unwrap().update(initial, target, &target_dir) {
                Ok(p) => return Ok(p),
                Err(e) => {
                    // TODO(phase-b): downcast to RuntimeException
                    let _re: &RuntimeException = todo!("downcast e to RuntimeException");
                    if !self.io.is_interactive() {
                        return Err(e);
                    }
                    self.io.write_error(
                        PhpMixed::String(format!(
                            "<error>    Update failed ({})</error>",
                            e,
                        )),
                        true,
                        IOInterface::NORMAL,
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
        let promise = initial_downloader.unwrap().remove(initial, &target_dir)?;

        let target_dir_owned = target_dir.clone();
        // TODO(phase-b): capture self and target into the closure
        Ok(promise.then(Box::new(
            move |_res: PhpMixed| -> Result<Box<dyn PromiseInterface>> {
                todo!("self.install(target, &target_dir_owned)")
            },
        )))
    }

    /// Removes package from target dir.
    ///
    /// @param PackageInterface $package   package instance
    /// @param string           $targetDir target dir
    /// @phpstan-return PromiseInterface<void|null>
    pub fn remove(
        &self,
        package: &dyn PackageInterface,
        target_dir: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        let target_dir = self.normalize_target_dir(target_dir);
        if let Some(downloader) = self.get_downloader_for_package(package)? {
            return downloader.remove(package, &target_dir);
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    /// Cleans up a failed operation
    ///
    /// @param string                $type        one of install/update/uninstall
    /// @param PackageInterface      $package     package instance
    /// @param string                $targetDir   target dir
    /// @param PackageInterface|null $prevPackage previous package instance in case of updates
    /// @phpstan-return PromiseInterface<void|null>
    pub fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        target_dir: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        let target_dir = self.normalize_target_dir(target_dir);
        if let Some(downloader) = self.get_downloader_for_package(package)? {
            return downloader.cleanup(r#type, package, &target_dir, prev_package);
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    /// Determines the install preference of a package
    ///
    /// @param PackageInterface $package package instance
    pub(crate) fn resolve_package_install_preference(
        &self,
        package: &dyn PackageInterface,
    ) -> String {
        for (pattern, preference) in &self.package_preferences {
            let pattern_regex = format!(
                "{{^{}$}}i",
                str_replace("\\*", ".*", &preg_quote(pattern, None)),
            );
            if Preg::is_match(&pattern_regex, package.get_name()) {
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
        package: &dyn PackageInterface,
        prev_package: Option<&dyn PackageInterface>,
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
                PhpMixed::String(prev_source.unwrap_or("").to_string()),
                &PhpMixed::List(
                    sources
                        .iter()
                        .map(|s| Box::new(PhpMixed::String(s.clone())))
                        .collect(),
                ),
                true,
            )
                // unless the previous package was stable dist (by default) and the new package is dev, then we allow the new default to take over
                && !(!prev.is_dev()
                    && prev.get_installation_source() == Some("dist")
                    && package.is_dev())
            {
                let prev_source_owned = prev_source.unwrap_or("").to_string();
                usort(&mut sources, move |a: &String, b: &String| -> i64 {
                    if *a == prev_source_owned {
                        -1
                    } else {
                        1
                    }
                });

                return Ok(sources);
            }
        }

        // reverse sources in case dist is the preferred source for this package
        if !self.prefer_source
            && (self.prefer_dist || "dist" == self.resolve_package_install_preference(package))
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
