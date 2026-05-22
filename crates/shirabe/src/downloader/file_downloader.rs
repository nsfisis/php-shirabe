//! ref: composer/src/Composer/Downloader/FileDownloader.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use std::sync::{LazyLock, Mutex};

use crate::util::Silencer;
use shirabe_external_packages::react::promise::resolve as react_promise_resolve;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, InvalidArgumentException, PATHINFO_BASENAME, PATHINFO_EXTENSION,
    PHP_URL_PATH, PhpMixed, RuntimeException, UnexpectedValueException, array_search, array_shift,
    file_exists, filesize, get_class, hash, hash_file, in_array, is_dir, is_executable, parse_url,
    pathinfo, realpath, rtrim, spl_object_hash, strlen, strpos, strtr, trim, umask, usleep,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::dependency_resolver::operation::InstallOperation;
use crate::dependency_resolver::operation::UninstallOperation;
use crate::dependency_resolver::operation::UpdateOperation;
use crate::downloader::ChangeReportInterface;
use crate::downloader::DownloaderInterface;
use crate::downloader::MaxFileSizeExceededException;
use crate::downloader::TransportException;
use crate::event_dispatcher::EventDispatcher;
use crate::exception::IrrecoverableDownloadException;
use crate::io::IOInterface;
use crate::io::NullIO;
use crate::package::PackageInterface;
use crate::package::comparer::Comparer;
use crate::plugin::PluginEvents;
use crate::plugin::PostFileDownloadEvent;
use crate::plugin::PreFileDownloadEvent;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use crate::util::Url as UrlUtil;

/// @var array<string, int|string>
/// @private
/// @internal
pub static DOWNLOAD_METADATA: LazyLock<Mutex<IndexMap<String, PhpMixed>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

/// Collects response headers when running on GH Actions
///
/// @var array<string, array<string>>
/// @private
/// @internal
pub static RESPONSE_HEADERS: LazyLock<Mutex<IndexMap<String, IndexMap<String, Vec<String>>>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

/// Base downloader for files
#[derive(Debug)]
pub struct FileDownloader {
    /// @var IOInterface
    pub(crate) io: Box<dyn IOInterface>,
    /// @var Config
    pub(crate) config: std::rc::Rc<std::cell::RefCell<Config>>,
    /// @var HttpDownloader
    pub(crate) http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    /// @var Filesystem
    pub(crate) filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    /// @var ?Cache
    pub(crate) cache: Option<std::rc::Rc<std::cell::RefCell<Cache>>>,
    /// @var ?EventDispatcher
    pub(crate) event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
    /// @var ProcessExecutor
    pub(crate) process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    /// @var array<string, string> Map of package name to cache key
    last_cache_writes: IndexMap<String, String>,
    /// @var array<string, string[]> Map of package name to list of paths
    additional_cleanup_paths: IndexMap<String, Vec<String>>,
}

impl FileDownloader {
    /// TODO(phase-b): `$downloadMetadata` is a static property in PHP; not yet mapped to Rust.
    pub fn reset_download_metadata() {
        todo!("FileDownloader::reset_download_metadata")
    }

    /// TODO(phase-b): `$downloadMetadata` is a static property in PHP; not yet mapped to Rust.
    pub fn download_metadata() -> indexmap::IndexMap<String, shirabe_php_shim::PhpMixed> {
        todo!("FileDownloader::download_metadata")
    }

    /// Constructor.
    pub fn new(
        io: Box<dyn IOInterface>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        cache: Option<std::rc::Rc<std::cell::RefCell<Cache>>>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> Self {
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone_box(),
            ))))
        });
        let filesystem = filesystem.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(Some(
                process.clone(),
            ))))
        });

        let mut this = Self {
            io,
            config,
            http_downloader,
            event_dispatcher,
            cache,
            process,
            filesystem,
            last_cache_writes: IndexMap::new(),
            additional_cleanup_paths: IndexMap::new(),
        };

        if this.cache.is_some() && this.cache.as_ref().unwrap().borrow().gc_is_necessary() {
            // PHP: writeError('Running cache garbage collection', true, io_interface::VERY_VERBOSE)
            this.io.write_error("Running cache garbage collection");
            this.cache.as_ref().unwrap().borrow_mut().gc(
                this.config
                    .borrow_mut()
                    .get("cache-files-ttl")
                    .as_int()
                    .unwrap_or(0),
                this.config
                    .borrow_mut()
                    .get("cache-files-maxsize")
                    .as_int()
                    .unwrap_or(0),
            );
        }

        this
    }
}

impl DownloaderInterface for FileDownloader {
    /// @inheritDoc
    fn get_installation_source(&self) -> String {
        "dist".to_owned()
    }

    /// @inheritDoc
    async fn download(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        _prev_package: Option<&dyn PackageInterface>,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        if package.get_dist_url().is_none() {
            return Err(InvalidArgumentException {
                message: "The given package is missing url information".to_string(),
                code: 0,
            }
            .into());
        }

        let cache_key_generator = |package: &dyn PackageInterface, key: &str| -> String {
            let cache_key = hash("sha1", key);

            format!(
                "{}/{}.{}",
                package.get_name(),
                cache_key,
                package.get_dist_type().unwrap_or("")
            )
        };

        let mut retries: i64 = 3;
        let dist_urls = package.get_dist_urls();
        // @var array<array{base: non-empty-string, processed: non-empty-string, cacheKey: string}> $urls
        let mut urls: Vec<UrlEntry> = vec![];
        for url in dist_urls {
            let processed_url = self.process_url(package, &url)?;
            let cache_key = cache_key_generator(package, &processed_url);
            urls.push(UrlEntry {
                base: url,
                processed: processed_url,
                // we use the complete download url here to avoid conflicting entries
                // from different packages, which would potentially allow a given package
                // in a third party repo to pre-populate the cache for the same package in
                // packagist for example.
                cache_key,
            });
        }
        debug_assert!(urls.len() > 0);

        let file_name = self.get_file_name(package, path);
        self.filesystem.borrow_mut().ensure_directory_exists(path)?;
        let dir_of_file = shirabe_php_shim::dirname(&file_name);
        self.filesystem
            .borrow_mut()
            .ensure_directory_exists(&dir_of_file)?;

        // TODO(phase-c-promise): rewrite the accept/reject/retry promise orchestration as an async loop.
        // TODO(plugin): inline closures rely on captured $accept/$reject/$urls/$retries. In Rust
        // we'd need a struct holding shared state — left as a phase-b refactor.
        let _ = (output, &urls, &mut retries, cache_key_generator, &file_name);
        let _ = PluginEvents::PRE_FILE_DOWNLOAD;
        let _ = PluginEvents::POST_FILE_DOWNLOAD;

        todo!(
            "phase-b: orchestrate download/accept/reject closures and call download() returning a PromiseInterface"
        )
    }

    /// @inheritDoc
    async fn prepare(
        &self,
        _type: &str,
        _package: &dyn PackageInterface,
        _path: &str,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        Ok(Some(PhpMixed::Null))
    }

    /// @inheritDoc
    async fn cleanup(
        &self,
        _type: &str,
        package: &dyn PackageInterface,
        path: &str,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        let file_name = self.get_file_name(package, path);
        if file_exists(&file_name) {
            self.filesystem.borrow_mut().unlink(&file_name)?;
        }

        let vendor_dir = self
            .config
            .borrow_mut()
            .get("vendor-dir")
            .as_string()
            .unwrap_or("")
            .to_string();
        let first_segment = package
            .get_pretty_name()
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        let dirs_to_clean_up: Vec<String> = vec![
            path.to_string(),
            format!("{}/{}", vendor_dir, first_segment),
            format!("{}/composer/", vendor_dir),
            vendor_dir.clone(),
        ];

        if let Some(paths) = self
            .additional_cleanup_paths
            .get(package.get_name())
            .cloned()
        {
            for path_to_clean in &paths {
                self.filesystem.borrow_mut().remove(path_to_clean)?;
            }
        }

        for dir in &dirs_to_clean_up {
            if is_dir(dir)
                && self.filesystem.borrow_mut().is_dir_empty(dir)
                && realpath(dir).as_deref() != Some(&Platform::get_cwd(false).unwrap_or_default())
            {
                self.filesystem.borrow_mut().remove_directory_php(dir)?;
            }
        }

        Ok(Some(PhpMixed::Null))
    }

    /// @inheritDoc
    async fn install(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        if output {
            self.io
                .write_error(&format!("  - {}", InstallOperation::format(package, false)));
        }

        let vendor_dir = self
            .config
            .borrow_mut()
            .get("vendor-dir")
            .as_string()
            .unwrap_or("")
            .to_string();

        // clean up the target directory, unless it contains the vendor dir, as the vendor dir contains
        // the file to be installed. This is the case when installing with create-project in the current directory
        // but in that case we ensure the directory is empty already in ProjectInstaller so no need to empty it here.
        if false == {
            let normalized_vendor = self.filesystem.borrow_mut().normalize_path(&vendor_dir);
            let normalized_path = self
                .filesystem
                .borrow()
                .normalize_path(&format!("{}{}", path, DIRECTORY_SEPARATOR));
            strpos(&normalized_vendor, &normalized_path).is_some()
        } {
            self.filesystem.borrow_mut().empty_directory(path, true)?;
        }
        self.filesystem.borrow_mut().ensure_directory_exists(path)?;
        self.filesystem.borrow_mut().rename(
            &self.get_file_name(package, path),
            &format!(
                "{}/{}",
                path,
                self.get_dist_path(package, PATHINFO_BASENAME)
            ),
        )?;

        // Single files can not have a mode set like files in archives
        // so we make sure if the file is a binary that it is executable
        for bin in package.get_binaries() {
            let bin_path = format!("{}/{}", path, bin);
            if file_exists(&bin_path) && !is_executable(&bin_path) {
                // TODO(phase-b): Silencer::call_named for native PHP function
                let _ = Silencer::call(|| {
                    let _ = bin_path;
                    let _ = umask();
                    Ok(())
                });
            }
        }

        Ok(Some(PhpMixed::Null))
    }

    /// @inheritDoc
    async fn update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        self.io.write_error(&format!(
            "  - {}{}",
            UpdateOperation::format(initial, target, false),
            self.get_install_operation_appendix(target, path)
        ));

        // PHP: return $this->remove($initial, $path, false)->then(fn () => $this->install($target, $path, false));
        let _ = self.remove(initial, path, false).await?;
        self.install(target, path, false).await
    }

    /// @inheritDoc
    async fn remove(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        if output {
            self.io.write_error(&format!(
                "  - {}",
                UninstallOperation::format(package, false)
            ));
        }
        let result = self
            .filesystem
            .borrow_mut()
            .remove_directory_async(path)
            .await?;
        if !result {
            return Err(RuntimeException {
                message: format!("Could not completely delete {}, aborting.", path),
                code: 0,
            }
            .into());
        }

        Ok(None)
    }
}

impl ChangeReportInterface for FileDownloader {
    /// @inheritDoc
    /// @throws \RuntimeException
    fn get_local_changes(
        &self,
        package: &dyn PackageInterface,
        path: &str,
    ) -> Result<Option<String>> {
        // TODO(phase-c-promise): get_local_changes drives promises via http_downloader/process wait();
        // converting requires deciding whether ChangeReportInterface::get_local_changes becomes async. Left as-is.
        // TODO(phase-b): swap self.io to NullIO and restore — needs a take/swap helper

        let mut null_io = NullIO::new();
        null_io.load_configuration(&mut *self.config.borrow_mut())?;
        // TODO(phase-b): `e` is captured by both the inner closure (assignment in error handler)
        // and the outer block (read after the closure). PHP closures capture by reference (`use (&$e)`);
        // emulate via Rc<RefCell> or restructure when proper async/promise types land.
        let e: std::cell::RefCell<Option<anyhow::Error>> = std::cell::RefCell::new(None);
        let output_cell: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());

        let target_dir = Filesystem::trim_trailing_slash(path);
        let result: Result<()> = (|| -> Result<()> {
            if is_dir(&format!("{}_compare", target_dir)) {
                self.filesystem
                    .borrow_mut()
                    .remove_directory(&format!("{}_compare", target_dir))?;
            }

            let promise =
                self.download(package, &format!("{}_compare", target_dir), None, false)?;
            promise.then_with(
                None,
                Some(Box::new(|ex: PhpMixed| {
                    let _ = ex;
                    PhpMixed::Null
                })),
            );
            self.http_downloader.borrow_mut().wait()?;
            if e.borrow().is_some() {
                return Err(e.borrow_mut().take().unwrap());
            }
            let promise = self.install(package, &format!("{}_compare", target_dir), false)?;
            promise.then_with(
                None,
                Some(Box::new(|ex: PhpMixed| {
                    let _ = ex;
                    PhpMixed::Null
                })),
            );
            self.process.borrow_mut().wait()?;
            if e.borrow().is_some() {
                return Err(e.borrow_mut().take().unwrap());
            }

            let mut comparer = Comparer::new();
            comparer.set_source(format!("{}_compare", target_dir));
            comparer.set_update(target_dir.clone());
            comparer.do_compare();
            *output_cell.borrow_mut() = comparer.get_changed_as_string(true, false);
            self.filesystem
                .borrow_mut()
                .remove_directory(&format!("{}_compare", target_dir))?;
            Ok(())
        })();
        if let Err(err) = result {
            *e.borrow_mut() = Some(err);
        }
        let e = e.into_inner();
        let output = output_cell.into_inner();

        // TODO(phase-b): restore self.io = prev_io

        if let Some(err) = e {
            if self.io.is_debug() {
                return Err(err);
            }

            return Ok(Some(format!(
                "Failed to detect changes: [{}] {}",
                get_class(&PhpMixed::Null),
                err
            )));
        }

        let output = trim(&output, None);

        Ok(if strlen(&output) > 0 {
            Some(output)
        } else {
            None
        })
    }
}

impl FileDownloader {
    /// @param PATHINFO_EXTENSION|PATHINFO_BASENAME $component
    fn get_dist_path(&self, package: &dyn PackageInterface, component: i64) -> String {
        pathinfo(
            PhpMixed::String(
                parse_url(
                    &strtr(package.get_dist_url().unwrap_or(""), "\\", "/"),
                    PHP_URL_PATH,
                )
                .as_string()
                .unwrap_or("")
                .to_string(),
            ),
            component,
        )
        .as_string()
        .unwrap_or("")
        .to_string()
    }

    pub(crate) fn clear_last_cache_write(&mut self, package: &dyn PackageInterface) {
        if self.cache.is_some() && self.last_cache_writes.contains_key(package.get_name()) {
            let key = self
                .last_cache_writes
                .get(package.get_name())
                .unwrap()
                .clone();
            self.cache.as_ref().unwrap().borrow_mut().remove(&key);
            self.last_cache_writes.shift_remove(package.get_name());
        }
    }

    pub(crate) fn add_cleanup_path(&mut self, package: &dyn PackageInterface, path: &str) {
        self.additional_cleanup_paths
            .entry(package.get_name().to_string())
            .or_insert_with(Vec::new)
            .push(path.to_string());
    }

    pub(crate) fn remove_cleanup_path(&mut self, package: &dyn PackageInterface, path: &str) {
        if let Some(paths) = self.additional_cleanup_paths.get_mut(package.get_name()) {
            // PHP: array_search($path, ..., true)
            let idx = paths.iter().position(|p| p == path);
            if let Some(i) = idx {
                paths.remove(i);
            }
            let _ = array_search;
        }
    }

    /// Gets file name for specific package
    pub(crate) fn get_file_name(&self, package: &dyn PackageInterface, _path: &str) -> String {
        let extension = self.get_dist_path(package, PATHINFO_EXTENSION);
        let extension = if extension.is_empty() {
            package.get_dist_type().unwrap_or("").to_string()
        } else {
            extension
        };

        rtrim(
            &format!(
                "{}/composer/tmp-{}.{}",
                self.config
                    .borrow_mut()
                    .get("vendor-dir")
                    .as_string()
                    .unwrap_or(""),
                hash(
                    "md5",
                    &format!("{}{}", package, spl_object_hash(&PhpMixed::Null))
                ),
                extension
            ),
            Some("."),
        )
    }

    /// Gets appendix message to add to the "- Upgrading x" string being output on update
    fn get_install_operation_appendix(
        &self,
        _package: &dyn PackageInterface,
        _path: &str,
    ) -> String {
        String::new()
    }

    /// Process the download url
    pub(crate) fn process_url(&self, package: &dyn PackageInterface, url: &str) -> Result<String> {
        if !shirabe_php_shim::extension_loaded("openssl") && Some(0) == strpos(url, "https:") {
            return Err(RuntimeException {
                message: "You must enable the openssl extension to download files via https"
                    .to_string(),
                code: 0,
            }
            .into());
        }

        let mut url = url.to_string();
        if package.get_dist_reference().is_some() {
            url = UrlUtil::update_dist_reference(
                &*self.config.borrow(),
                url,
                package.get_dist_reference().unwrap(),
            );
        }

        Ok(url)
    }
}

#[derive(Debug, Clone)]
struct UrlEntry {
    base: String,
    processed: String,
    cache_key: String,
}

// Suppress unused-import warnings for items kept for parity with the PHP source.
#[allow(dead_code)]
fn _use_parity() {
    let _ = filesize;
    let _ = hash_file;
    let _ = in_array;
    let _ = usleep;
    let _ = array_shift::<u8>;
    let _ = UnexpectedValueException {
        message: String::new(),
        code: 0,
    };
}
