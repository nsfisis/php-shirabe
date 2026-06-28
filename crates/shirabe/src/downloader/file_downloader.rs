//! ref: composer/src/Composer/Downloader/FileDownloader.php

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
use crate::io::IOInterfaceImmutable;
use crate::io::NullIO;
use crate::io::io_interface;
use crate::package::PackageInterfaceHandle;
use crate::package::comparer::Comparer;
use crate::util::Filesystem;
use crate::util::HttpDownloader;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use crate::util::Silencer;
use crate::util::Url as UrlUtil;
use crate::util::sync_executor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, InvalidArgumentException, PATHINFO_BASENAME, PATHINFO_EXTENSION,
    PHP_URL_PATH, PhpMixed, RuntimeException, UnexpectedValueException, array_search, file_exists,
    filesize, get_class, hash, hash_file, in_array, is_dir, is_executable, parse_url, pathinfo,
    realpath, rtrim, spl_object_hash, strlen, strpos, strtr, trim, umask, usleep,
};
use std::sync::{LazyLock, Mutex};

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
pub static RESPONSE_HEADERS: LazyLock<Mutex<IndexMap<String, Vec<String>>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

/// Base downloader for files
#[derive(Debug)]
pub struct FileDownloader {
    /// @var IOInterface
    pub(crate) io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
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
    ///
    /// Behind a Mutex so `download()` can record cache writes through `&self`. `download()` blocks
    /// for a while and (once the workers are parallelized) runs from several threads; requiring
    /// `&mut self` would force locking the whole FileDownloader for each download's duration. Only
    /// this write needs guarding, so it is the only field isolated behind a lock.
    last_cache_writes: Mutex<IndexMap<String, String>>,
    /// @var array<string, string[]> Map of package name to list of paths
    additional_cleanup_paths: IndexMap<String, Vec<String>>,
}

impl FileDownloader {
    pub fn reset_download_metadata() {
        DOWNLOAD_METADATA.lock().unwrap().clear();
    }

    pub fn download_metadata() -> IndexMap<String, PhpMixed> {
        DOWNLOAD_METADATA.lock().unwrap().clone()
    }

    /// Constructor.
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        cache: Option<std::rc::Rc<std::cell::RefCell<Cache>>>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> Self {
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone(),
            ))))
        });
        let filesystem = filesystem.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(Some(
                process.clone(),
            ))))
        });

        let this = Self {
            io,
            config,
            http_downloader,
            event_dispatcher,
            cache,
            process,
            filesystem,
            last_cache_writes: Mutex::new(IndexMap::new()),
            additional_cleanup_paths: IndexMap::new(),
        };

        if let Some(cache) = &this.cache
            && cache.borrow().gc_is_necessary()
        {
            // PHP: writeError('Running cache garbage collection', true, io_interface::VERY_VERBOSE)
            this.io.write_error("Running cache garbage collection");
            let ttl = this
                .config
                .borrow_mut()
                .get("cache-files-ttl")
                .as_int()
                .unwrap_or(0);
            let max_size = this
                .config
                .borrow_mut()
                .get("cache-files-maxsize")
                .as_int()
                .unwrap_or(0);
            cache.borrow_mut().gc(ttl, max_size);
        }

        this
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for FileDownloader {
    /// @inheritDoc
    fn get_installation_source(&self) -> String {
        "dist".to_owned()
    }

    fn as_change_report_interface(
        &mut self,
    ) -> Option<&mut dyn crate::downloader::ChangeReportInterface> {
        Some(self)
    }

    /// @inheritDoc
    async fn download(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _prev_package: Option<PackageInterfaceHandle>,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        if package.get_dist_url().is_none() {
            return Err(InvalidArgumentException {
                message: "The given package is missing url information".to_string(),
                code: 0,
            }
            .into());
        }

        let cache_key_generator = |package: PackageInterfaceHandle, key: &str| -> String {
            let cache_key = hash("sha1", key);

            format!(
                "{}/{}.{}",
                package.get_name(),
                cache_key,
                package.get_dist_type().unwrap_or_default()
            )
        };

        let mut retries: i64 = 3;
        let dist_urls = package.get_dist_urls();
        // @var array<array{base: non-empty-string, processed: non-empty-string, cacheKey: string}> $urls
        let mut urls: Vec<UrlEntry> = vec![];
        for url in dist_urls {
            let processed_url = self.process_url(package.clone(), &url)?;
            let cache_key = cache_key_generator(package.clone(), &processed_url);
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
        debug_assert!(!urls.is_empty());

        let file_name = self.get_file_name(package.clone(), path);
        self.filesystem.borrow_mut().ensure_directory_exists(path)?;
        let dir_of_file = shirabe_php_shim::dirname(&file_name);
        self.filesystem
            .borrow_mut()
            .ensure_directory_exists(&dir_of_file)?;

        // The PHP $download/$accept/$reject closures form a retry loop driven by recursion; here it
        // is expressed as a loop. $reject's "return $download()" maps to `continue`, "throw" to
        // `return Err`, and the success path runs the verification block and returns the file name.
        let _ = cache_key_generator;
        loop {
            // === $download() ===
            let url = urls[0].clone();
            // TODO(plugin): dispatch PreFileDownloadEvent and apply its custom cache key / processed url.
            urls[0] = url.clone();

            let checksum = package.get_dist_sha1_checksum().map(|s| s.to_string());
            let cache_key = url.cache_key.clone();

            // use from cache if it is present and has a valid checksum or we have no checksum to check against
            let mut from_cache = false;
            if let Some(cache) = self.cache.clone() {
                let checksum_matches = match checksum.as_deref() {
                    None | Some("") => true,
                    Some(c) => Some(c) == cache.borrow_mut().sha1(&cache_key).as_deref(),
                };
                if checksum_matches && cache.borrow_mut().copy_to(&cache_key, &file_name)? {
                    from_cache = true;
                }
            }

            if from_cache {
                if output {
                    self.io.write_error3(
                        &format!(
                            "  - Loading <info>{}</info> (<comment>{}</comment>) from cache",
                            package.get_name(),
                            package.get_full_pretty_version(
                                true,
                                crate::package::DisplayMode::SourceRefIfDev
                            )
                        ),
                        true,
                        io_interface::VERY_VERBOSE,
                    );
                }
                // mark the file as having been written in cache even though it is only read from cache, so that if
                // the cache is corrupt the archive will be deleted and the next attempt will re-download it
                // see https://github.com/composer/composer/issues/10028
                if let Some(cache) = self.cache.as_ref()
                    && !cache.borrow().is_read_only()
                {
                    self.last_cache_writes
                        .lock()
                        .unwrap()
                        .insert(package.get_name().to_string(), cache_key.clone());
                }
            } else {
                if output {
                    self.io.write_error(&format!(
                        "  - Downloading <info>{}</info> (<comment>{}</comment>)",
                        package.get_name(),
                        package.get_full_pretty_version(
                            true,
                            crate::package::DisplayMode::SourceRefIfDev
                        )
                    ));
                }

                let add_copy_result = self
                    .http_downloader
                    .borrow_mut()
                    .add_copy(&url.processed, &file_name, package.get_transport_options())
                    .await;
                match add_copy_result {
                    Ok(mut response) => {
                        // === $accept($response) ===
                        let cache_key = urls[0].cache_key.clone();
                        let file_size = match filesize(&file_name) {
                            Some(size) => PhpMixed::Int(size),
                            None => PhpMixed::String(
                                response
                                    .get_header("Content-Length")
                                    .unwrap_or_else(|| "?".to_string()),
                            ),
                        };
                        DOWNLOAD_METADATA
                            .lock()
                            .unwrap()
                            .insert(package.get_name().to_string(), file_size);

                        if Platform::get_env("GITHUB_ACTIONS").is_some()
                            && Platform::get_env("COMPOSER_TESTS_ARE_RUNNING").is_none()
                        {
                            RESPONSE_HEADERS.lock().unwrap().insert(
                                package.get_name().to_string(),
                                response.get_headers().clone(),
                            );
                        }

                        if let Some(cache) = self.cache.as_ref()
                            && !cache.borrow().is_read_only()
                        {
                            self.last_cache_writes
                                .lock()
                                .unwrap()
                                .insert(package.get_name().to_string(), cache_key.clone());
                            cache.borrow_mut().copy_from(&cache_key, &file_name);
                        }

                        response.collect();
                    }
                    Err(e) => {
                        // === $reject($e) ===
                        // clean up
                        if file_exists(&file_name) {
                            self.filesystem.borrow().unlink(&file_name)?;
                        }
                        self.clear_last_cache_write(package.clone());

                        if e.downcast_ref::<IrrecoverableDownloadException>().is_some() {
                            return Err(e);
                        }

                        if e.downcast_ref::<MaxFileSizeExceededException>().is_some() {
                            return Err(e);
                        }

                        if let Some(te) = e.downcast_ref::<TransportException>() {
                            // if we got an http response with a proper code, then requesting again will probably not help, abort
                            if 0 != te.get_code()
                                && !in_array(
                                    PhpMixed::Int(te.get_code()),
                                    &PhpMixed::List(vec![
                                        PhpMixed::Int(500),
                                        PhpMixed::Int(502),
                                        PhpMixed::Int(503),
                                        PhpMixed::Int(504),
                                    ]),
                                    true,
                                )
                            {
                                retries = 0;
                            }

                            // special error code returned when network is being artificially disabled
                            if te.get_status_code() == Some(499) {
                                retries = 0;
                                urls.clear();
                            }
                        }

                        if retries > 0 {
                            usleep(500000);
                            retries -= 1;

                            continue;
                        }

                        if !urls.is_empty() {
                            urls.remove(0);
                        }
                        if !urls.is_empty() {
                            let code = e
                                .downcast_ref::<TransportException>()
                                .map_or(0, |te| te.get_code());
                            if self.io.is_debug() {
                                self.io.write_error(&format!(
                                    "    Failed downloading {}: [{}] {}: {}",
                                    package.get_name(),
                                    get_class(&PhpMixed::Null),
                                    code,
                                    e
                                ));
                                self.io.write_error(&format!(
                                    "    Trying the next URL for {}",
                                    package.get_name()
                                ));
                            } else {
                                self.io.write_error(&format!(
                                    "    Failed downloading {}, trying the next URL ({}: {})",
                                    package.get_name(),
                                    code,
                                    e
                                ));
                            }

                            retries = 3;
                            usleep(100000);

                            continue;
                        }

                        return Err(e);
                    }
                }
            }

            // === $result->then(verify) ===
            if !file_exists(&file_name) {
                return Err(UnexpectedValueException {
                    message: format!(
                        "{} could not be saved to {}, make sure the directory is writable and you have internet connectivity",
                        url.base, file_name
                    ),
                    code: 0,
                }
                .into());
            }

            if let Some(checksum) = checksum.as_deref()
                && !checksum.is_empty()
                && hash_file("sha1", &file_name).as_deref() != Some(checksum)
            {
                return Err(UnexpectedValueException {
                    message: format!(
                        "The checksum verification of the file failed (downloaded from {})",
                        url.base
                    ),
                    code: 0,
                }
                .into());
            }

            // TODO(plugin): dispatch PostFileDownloadEvent.

            return Ok(Some(PhpMixed::String(file_name)));
        }
    }

    /// @inheritDoc
    async fn prepare(
        &mut self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _path: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        Ok(Some(PhpMixed::Null))
    }

    /// @inheritDoc
    async fn cleanup(
        &mut self,
        _type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        let file_name = self.get_file_name(package.clone(), path);
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
            .get(&package.get_name())
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
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        output: bool,
    ) -> Result<Option<PhpMixed>> {
        if output {
            self.io.write_error(&format!(
                "  - {}",
                InstallOperation::format(package.clone(), false)
            ));
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
        if !{
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
            self.get_file_name(package.clone(), path),
            format!(
                "{}/{}",
                path,
                self.get_dist_path(package.clone(), PATHINFO_BASENAME)
            ),
        )?;

        // Single files can not have a mode set like files in archives
        // so we make sure if the file is a binary that it is executable
        for bin in package.get_binaries() {
            let bin_path = format!("{}/{}", path, bin);
            if file_exists(&bin_path) && !is_executable(&bin_path) {
                let _ = Silencer::call(|| Ok(shirabe_php_shim::chmod(&bin_path, 0o777 & !umask())));
            }
        }

        Ok(Some(PhpMixed::Null))
    }

    /// @inheritDoc
    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        self.io.write_error(&format!(
            "  - {}{}",
            UpdateOperation::format(initial.clone(), target.clone(), false),
            self.get_install_operation_appendix(target.clone(), path)
        ));

        // PHP: return $this->remove($initial, $path, false)->then(fn () => $this->install($target, $path, false));
        let _ = self.remove(initial, path, false).await?;
        self.install(target, path, false).await
    }

    /// @inheritDoc
    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
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
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<String>> {
        let prev_io = std::mem::replace(
            &mut self.io,
            std::rc::Rc::new(std::cell::RefCell::new(NullIO::new())),
        );
        self.io
            .borrow_mut()
            .load_configuration(&mut self.config.borrow_mut())?;

        let target_dir = Filesystem::trim_trailing_slash(path);
        // PHP attaches an onRejected handler to capture the error and drives the promise via
        // httpDownloader->wait() / process->wait(); the single-threaded sync bridge block_on's the
        // download/install futures, so a rejection surfaces directly as the Err captured below.
        let result: Result<String> = (|| -> Result<String> {
            if is_dir(format!("{}_compare", target_dir)) {
                self.filesystem
                    .borrow_mut()
                    .remove_directory(format!("{}_compare", target_dir))?;
            }

            sync_executor::block_on(self.download(
                package.clone(),
                &format!("{}_compare", target_dir),
                None,
                false,
            ))?;
            sync_executor::block_on(self.install(
                package.clone(),
                &format!("{}_compare", target_dir),
                false,
            ))?;

            let mut comparer = Comparer::new();
            comparer.set_source(format!("{}_compare", target_dir));
            comparer.set_update(target_dir.clone());
            comparer.do_compare();
            let output = comparer.get_changed_as_string(true, false);
            self.filesystem
                .borrow_mut()
                .remove_directory(format!("{}_compare", target_dir))?;
            Ok(output)
        })();

        self.io = prev_io;

        let (e, output) = match result {
            Ok(output) => (None, output),
            Err(err) => (Some(err), String::new()),
        };

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
    fn get_dist_path(&self, package: PackageInterfaceHandle, component: i64) -> String {
        pathinfo(
            PhpMixed::String(
                parse_url(
                    &strtr(&package.get_dist_url().unwrap_or_default(), "\\", "/"),
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

    pub(crate) fn clear_last_cache_write(&self, package: PackageInterfaceHandle) {
        let mut last_cache_writes = self.last_cache_writes.lock().unwrap();
        if let Some(cache) = &self.cache
            && last_cache_writes.contains_key(&package.get_name())
        {
            let key = last_cache_writes.get(&package.get_name()).unwrap().clone();
            cache.borrow_mut().remove(&key);
            last_cache_writes.shift_remove(&package.get_name());
        }
    }

    pub(crate) fn add_cleanup_path(&mut self, package: PackageInterfaceHandle, path: &str) {
        self.additional_cleanup_paths
            .entry(package.get_name())
            .or_default()
            .push(path.to_string());
    }

    pub(crate) fn remove_cleanup_path(&mut self, package: PackageInterfaceHandle, path: &str) {
        if let Some(paths) = self.additional_cleanup_paths.get_mut(&package.get_name()) {
            // PHP: array_search($path, ..., true)
            let idx = paths.iter().position(|p| p == path);
            if let Some(i) = idx {
                paths.remove(i);
            }
            let _ = array_search;
        }
    }

    /// Gets file name for specific package
    pub(crate) fn get_file_name(&self, package: PackageInterfaceHandle, _path: &str) -> String {
        let extension = self.get_dist_path(package.clone(), PATHINFO_EXTENSION);
        let extension = if extension.is_empty() {
            package.get_dist_type().unwrap_or_default()
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
        _package: PackageInterfaceHandle,
        _path: &str,
    ) -> String {
        String::new()
    }

    /// For testing only: invoke the crate-private `get_file_name`.
    pub fn __get_file_name(&self, package: PackageInterfaceHandle, path: &str) -> String {
        self.get_file_name(package, path)
    }

    /// For testing only: invoke the crate-private `process_url`.
    pub fn __process_url(&self, package: PackageInterfaceHandle, url: &str) -> Result<String> {
        self.process_url(package, url)
    }

    /// Process the download url
    pub(crate) fn process_url(&self, package: PackageInterfaceHandle, url: &str) -> Result<String> {
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
                &self.config.borrow(),
                url,
                &package.get_dist_reference().unwrap(),
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
