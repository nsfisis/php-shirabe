//! ref: composer/src/Composer/Downloader/FileDownloader.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use std::sync::{LazyLock, Mutex};

use crate::util::silencer::Silencer;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::react::promise::resolve as react_promise_resolve;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, InvalidArgumentException, PATHINFO_BASENAME, PATHINFO_EXTENSION,
    PHP_URL_PATH, PhpMixed, RuntimeException, UnexpectedValueException, array_search, array_shift,
    file_exists, filesize, get_class, hash, hash_file, in_array, is_dir, is_executable, parse_url,
    pathinfo, realpath, rtrim, spl_object_hash, strlen, strpos, strtr, trim, umask, usleep,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::dependency_resolver::operation::install_operation::InstallOperation;
use crate::dependency_resolver::operation::uninstall_operation::UninstallOperation;
use crate::dependency_resolver::operation::update_operation::UpdateOperation;
use crate::downloader::change_report_interface::ChangeReportInterface;
use crate::downloader::downloader_interface::DownloaderInterface;
use crate::downloader::max_file_size_exceeded_exception::MaxFileSizeExceededException;
use crate::downloader::transport_exception::TransportException;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::exception::irrecoverable_download_exception::IrrecoverableDownloadException;
use crate::io::io_interface::IOInterface;
use crate::io::null_io::NullIO;
use crate::package::comparer::comparer::Comparer;
use crate::package::package_interface::PackageInterface;
use crate::plugin::plugin_events::PluginEvents;
use crate::plugin::post_file_download_event::PostFileDownloadEvent;
use crate::plugin::pre_file_download_event::PreFileDownloadEvent;
use crate::util::filesystem::Filesystem;
use crate::util::http_downloader::HttpDownloader;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;
use crate::util::url::Url as UrlUtil;

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
    pub(crate) cache: Option<Cache>,
    /// @var ?EventDispatcher
    pub(crate) event_dispatcher: Option<EventDispatcher>,
    /// @var ProcessExecutor
    pub(crate) process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    /// @var array<string, string> Map of package name to cache key
    last_cache_writes: IndexMap<String, String>,
    /// @var array<string, string[]> Map of package name to list of paths
    additional_cleanup_paths: IndexMap<String, Vec<String>>,
}

impl FileDownloader {
    /// Constructor.
    pub fn new(
        io: Box<dyn IOInterface>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        event_dispatcher: Option<EventDispatcher>,
        cache: Option<Cache>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> Self {
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                Box::new(&*io),
            ))))
        });
        let filesystem = filesystem.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(Some(
                std::rc::Rc::clone(&process),
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

        if this.cache.is_some() && this.cache.as_ref().unwrap().gc_is_necessary() {
            // PHP: writeError('Running cache garbage collection', true, io_interface::VERY_VERBOSE)
            this.io.write_error("Running cache garbage collection");
            this.cache.as_mut().unwrap().gc(
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
    fn download(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        _prev_package: Option<&dyn PackageInterface>,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
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
        let dir_of_file = shirabe_php_shim::dirname(&file_name, 1);
        self.filesystem
            .borrow_mut()
            .ensure_directory_exists(&dir_of_file)?;

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
    fn prepare(
        &self,
        _type: &str,
        _package: &dyn PackageInterface,
        _path: &str,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        Ok(react_promise_resolve(PhpMixed::Null))
    }

    /// @inheritDoc
    fn cleanup(
        &self,
        _type: &str,
        package: &dyn PackageInterface,
        path: &str,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
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
                && self.filesystem.borrow_mut().is_dir_empty(dir)?
                && realpath(dir).as_deref() != Some(&Platform::get_cwd(false).unwrap_or_default())
            {
                self.filesystem.borrow_mut().remove_directory_php(dir)?;
            }
        }

        Ok(react_promise_resolve(PhpMixed::Null))
    }

    /// @inheritDoc
    fn install(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
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

        Ok(react_promise_resolve(Some(PhpMixed::Null)))
    }

    /// @inheritDoc
    fn update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.io.write_error(&format!(
            "  - {}{}",
            UpdateOperation::format(initial, target, false),
            self.get_install_operation_appendix(target, path)
        ));

        let _promise = self.remove(initial, path, false)?;
        // TODO(phase-b): chain `.then(|| self.install(target, path, false))`
        let _ = (initial, target, path);
        todo!("phase-b: chain promise.then(|| self.install(target, path, false))")
    }

    /// @inheritDoc
    fn remove(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        if output {
            self.io.write_error(&format!(
                "  - {}",
                UninstallOperation::format(package, false)
            ));
        }
        let _promise = self.filesystem.borrow_mut().remove_directory_async(path)?;

        // TODO(phase-b): chain `.then(|result| if !result { throw RuntimeException })`
        let _ = path;
        todo!("phase-b: chain promise.then(|result| {{ if !result {{ throw RuntimeException }} }})")
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
        // TODO(phase-b): swap self.io to NullIO and restore — needs a take/swap helper

        let mut null_io = NullIO::new();
        null_io.load_configuration(&mut *self.config.borrow_mut())?;
        let mut e: Option<anyhow::Error> = None;
        let mut output: String = String::new();

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
            if e.is_some() {
                return Err(e.unwrap());
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
            if e.is_some() {
                return Err(e.unwrap());
            }

            let mut comparer = Comparer::new();
            comparer.set_source(format!("{}_compare", target_dir));
            comparer.set_update(target_dir.clone());
            comparer.do_compare();
            output = comparer.get_changed_as_string(true, false);
            self.filesystem
                .borrow_mut()
                .remove_directory(&format!("{}_compare", target_dir))?;
            Ok(())
        })();
        if let Err(err) = result {
            e = Some(err);
        }

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

    fn clear_last_cache_write(&mut self, package: &dyn PackageInterface) {
        if self.cache.is_some() && self.last_cache_writes.contains_key(package.get_name()) {
            self.cache
                .as_ref()
                .unwrap()
                .remove(self.last_cache_writes.get(package.get_name()).unwrap());
            self.last_cache_writes.shift_remove(package.get_name());
        }
    }

    fn add_cleanup_path(&mut self, package: &dyn PackageInterface, path: &str) {
        self.additional_cleanup_paths
            .entry(package.get_name().to_string())
            .or_insert_with(Vec::new)
            .push(path.to_string());
    }

    fn remove_cleanup_path(&mut self, package: &dyn PackageInterface, path: &str) {
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
    fn get_file_name(&self, package: &dyn PackageInterface, _path: &str) -> String {
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
    fn process_url(&self, package: &dyn PackageInterface, url: &str) -> Result<String> {
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
                &url,
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
const _USE_PARITY: () = {
    let _ = filesize;
    let _ = hash_file;
    let _ = in_array;
    let _ = usleep;
    let _ = array_shift::<u8>;
    let _ = UnexpectedValueException {
        message: String::new(),
        code: 0,
    };
};
