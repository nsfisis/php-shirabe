//! ref: composer/src/Composer/Cache.php

use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::util::Filesystem;
use crate::util::Platform;
use crate::util::Silencer;
use chrono::Utc;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::symfony::finder::Finder;
use shirabe_php_shim::{
    ErrorException, bin2hex, clearstatcache, date_format_to_strftime, dirname, disk_free_space,
    file_exists, file_get_contents, file_put_contents, filemtime, function_exists, hash_file,
    is_dir, is_writable, mkdir, php_regex, random_bytes, random_int, rename, time, unlink,
};
use std::sync::Mutex;

/// Reads/writes to a filesystem cache
#[derive(Debug)]
pub struct Cache {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    root: String,
    enabled: Option<bool>,
    allowlist: String,
    filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    read_only: bool,
    /// Test-only seam. Always `None` in production; configured via [`Cache::__set_mock`].
    mock: Option<CacheMock>,
}

/// Test-only seam mirroring the PHP CacheTest/FileDownloaderTest mocks of `Cache`.
#[derive(Debug, Default)]
pub struct CacheMock {
    /// Overrides the file lists `gc()` derives from `get_finder()`, mirroring the PHP CacheTest
    /// which mocks `getFinder` to feed `gc()` a controlled iterator.
    pub finder: Option<GcFinderMock>,
    /// When `Some`, `gc_is_necessary` returns it verbatim.
    pub gc_is_necessary: Option<bool>,
    /// When `Some`, `gc` records each `(ttl, max_size)` call here and skips its real body.
    pub gc_calls: Option<Vec<(i64, i64)>>,
}

/// The controlled file lists used by a mocked `gc()` pass.
#[derive(Debug, Default)]
pub struct GcFinderMock {
    /// Entries yielded by `get_finder().date(...)`, i.e. the outdated files.
    pub outdated: Vec<std::path::PathBuf>,
    /// Entries yielded by `get_finder().sort_by_accessed_time().get_iterator()`.
    pub by_accessed_time: Vec<std::path::PathBuf>,
}

/// @var bool|null
static CACHE_COLLECTED: Mutex<Option<bool>> = Mutex::new(None);

impl Cache {
    /// @param string      $cacheDir   location of the cache
    /// @param string      $allowlist  List of characters that are allowed in path names (used in a regex character class)
    /// @param Filesystem  $filesystem optional filesystem instance
    /// @param bool        $readOnly   whether the cache is in readOnly mode
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        cache_dir: &str,
        allowlist: Option<&str>,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
        read_only: bool,
    ) -> Self {
        let allowlist = allowlist.unwrap_or("a-z0-9._").to_string();
        let root = format!("{}/", cache_dir.trim_end_matches(['/', '\\']));
        let filesystem = filesystem
            .unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))));
        let mut this = Self {
            io,
            root,
            allowlist,
            filesystem,
            read_only,
            enabled: None,
            mock: None,
        };

        if !Self::is_usable(cache_dir) {
            this.enabled = Some(false);
        }

        this
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn is_usable(path: &str) -> bool {
        !Preg::is_match(
            php_regex!(r"{(^|[\\\\/])(\$null|nul|NUL|/dev/null)([\\\\/]|$)}"),
            path,
        )
    }

    pub fn is_enabled(&mut self) -> bool {
        if self.enabled.is_none() {
            self.enabled = Some(true);

            if !self.read_only
                && ((!is_dir(&self.root)
                    && !Silencer::call(|| Ok(mkdir(&self.root, 0o777, true))).unwrap_or(false))
                    || !is_writable(&self.root))
            {
                self.io.write_error(&format!(
                    "<warning>Cannot create cache directory {}, or directory is not writable. Proceeding without cache. See also cache-read-only config if your filesystem is read-only.</warning>",
                    self.root,
                ));
                self.enabled = Some(false);
            }
        }

        self.enabled.unwrap_or(false)
    }

    pub fn get_root(&self) -> &str {
        &self.root
    }

    /// @return string|false
    pub fn read(&mut self, file: &str) -> Option<String> {
        if self.is_enabled() {
            let file = Preg::replace(format!("{{[^{}]}}i", self.allowlist), "-", file);
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path) {
                self.io.write_error3(
                    &format!("Reading {} from cache", full_path),
                    true,
                    crate::io::DEBUG,
                );

                return file_get_contents(&full_path);
            }
        }

        None
    }

    pub fn write(&mut self, file: &str, contents: &str) -> anyhow::Result<bool> {
        let was_enabled = self.enabled == Some(true);

        if self.is_enabled() && !self.read_only {
            let file = Preg::replace(format!("{{[^{}]}}i", self.allowlist), "-", file);

            self.io.write_error3(
                &format!("Writing {}{} into cache", self.root, file),
                true,
                crate::io::DEBUG,
            );

            let temp_file_name = format!("{}{}{}.tmp", self.root, file, bin2hex(&random_bytes(5)));
            let dest = format!("{}{}", self.root, file);
            let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                file_put_contents(&temp_file_name, contents.as_bytes()).is_some()
                    && rename(&temp_file_name, &dest)
            }));
            return match attempt {
                Ok(b) => Ok(b),
                Err(payload) => {
                    let e: ErrorException = match payload.downcast::<ErrorException>() {
                        Ok(boxed) => *boxed,
                        Err(payload) => std::panic::resume_unwind(payload),
                    };

                    // If the write failed despite isEnabled checks passing earlier, rerun the isEnabled checks to
                    // see if they are still current and recreate the cache dir if needed. Refs https://github.com/composer/composer/issues/11076
                    if was_enabled {
                        clearstatcache();
                        self.enabled = None;

                        return self.write(&file, contents);
                    }

                    self.io.write_error3(
                        &format!(
                            "<warning>Failed to write into cache: {}</warning>",
                            e.message
                        ),
                        true,
                        crate::io::DEBUG,
                    );
                    let mut m = indexmap::IndexMap::new();
                    if Preg::match3(
                        php_regex!(
                            r"{^file_put_contents\(\): Only ([0-9]+) of ([0-9]+) bytes written}"
                        ),
                        &e.message,
                        Some(&mut m),
                    ) {
                        // Remove partial file.
                        unlink(&temp_file_name);

                        let free_space = if function_exists("disk_free_space") {
                            disk_free_space(&dirname(&temp_file_name))
                                .map(|space| space.to_string())
                                .unwrap_or_default()
                        } else {
                            "unknown".to_string()
                        };
                        let message = format!(
                            "<warning>Writing {} into cache failed after {} of {} bytes written, only {} bytes of free space available</warning>",
                            temp_file_name,
                            m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default(),
                            m.get(&CaptureKey::ByIndex(2)).cloned().unwrap_or_default(),
                            free_space,
                        );

                        self.io.write_error(&message);

                        return Ok(false);
                    }

                    Err(e.into())
                }
            };
        }

        Ok(false)
    }

    /// Copy a file into the cache
    pub fn copy_from(&mut self, file: &str, source: &str) -> bool {
        if self.is_enabled() && !self.read_only {
            let file = Preg::replace(format!("{{[^{}]}}i", self.allowlist), "-", file);
            let full_path = format!("{}{}", self.root, file);
            self.filesystem
                .borrow_mut()
                .ensure_directory_exists(&dirname(&full_path));

            if !file_exists(source) {
                self.io.write_error(&format!(
                    "<error>{} does not exist, can not write into cache</error>",
                    source,
                ));
            } else if self.io.is_debug() {
                self.io
                    .write_error(&format!("Writing {} into cache from {}", full_path, source));
            }

            return self
                .filesystem
                .borrow_mut()
                .copy(source, &full_path)
                .unwrap_or(false);
        }

        false
    }

    /// Copy a file out of the cache
    pub fn copy_to(&mut self, file: &str, target: &str) -> anyhow::Result<bool> {
        if self.is_enabled() {
            let file = Preg::replace(format!("{{[^{}]}}i", self.allowlist), "-", file);
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path) {
                let touch_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    shirabe_php_shim::touch3(
                        &full_path,
                        filemtime(&full_path).unwrap_or(0),
                        time(),
                    );
                }));
                if let Err(payload) = touch_result {
                    match payload.downcast::<ErrorException>() {
                        Ok(_) => {
                            // fallback in case the above failed due to incorrect ownership
                            // see https://github.com/composer/composer/issues/4070
                            Silencer::call(|| Ok(shirabe_php_shim::touch(&full_path)))?;
                        }
                        Err(payload) => std::panic::resume_unwind(payload),
                    }
                }

                self.io.write_error3(
                    &format!("Reading {} from cache", full_path),
                    true,
                    crate::io::DEBUG,
                );

                return self.filesystem.borrow_mut().copy(&full_path, target);
            }
        }

        Ok(false)
    }

    pub fn gc_is_necessary(&self) -> bool {
        if let Some(mock) = &self.mock
            && let Some(necessary) = mock.gc_is_necessary
        {
            return necessary;
        }

        let mut cache_collected = CACHE_COLLECTED.lock().unwrap();
        if cache_collected.unwrap_or(false) {
            return false;
        }

        *cache_collected = Some(true);
        if Platform::get_env("COMPOSER_TEST_SUITE").is_some() {
            return false;
        }

        if Platform::is_input_completion_process() {
            return false;
        }

        random_int(0..50) == 0
    }

    pub fn remove(&mut self, file: &str) -> bool {
        if self.is_enabled() && !self.read_only {
            let file = Preg::replace(format!("{{[^{}]}}i", self.allowlist), "-", file);
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path) {
                return self
                    .filesystem
                    .borrow_mut()
                    .unlink(&full_path)
                    .unwrap_or(false);
            }
        }

        false
    }

    pub fn clear(&mut self) -> bool {
        if self.is_enabled() && !self.read_only {
            let _ = self
                .filesystem
                .borrow_mut()
                .empty_directory(&self.root, true);

            return true;
        }

        false
    }

    /// @return int|false
    /// @phpstan-return int<0, max>|false
    pub fn get_age(&mut self, file: &str) -> Option<i64> {
        if self.is_enabled() {
            let file = Preg::replace(format!("{{[^{}]}}i", self.allowlist), "-", file);
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path)
                && let Some(mtime) = filemtime(&full_path)
            {
                return Some((time() - mtime).abs());
            }
        }

        None
    }

    pub fn gc(&mut self, ttl: i64, max_size: i64) -> bool {
        if let Some(mock) = self.mock.as_mut()
            && let Some(calls) = mock.gc_calls.as_mut()
        {
            calls.push((ttl, max_size));
            return true;
        }

        if self.is_enabled() && !self.read_only {
            let expire = Utc::now() - chrono::Duration::seconds(ttl);
            let expire_str = format!(
                "until {}",
                expire.format(date_format_to_strftime("Y-m-d H:i:s"))
            );

            for file in self.gc_outdated_files(&expire_str) {
                let _ = self.filesystem.borrow_mut().unlink(&file);
            }

            let mut total_size = self.filesystem.borrow_mut().size(&self.root).unwrap_or(0);
            if total_size > max_size {
                for filepath in self.gc_files_by_accessed_time() {
                    if total_size <= max_size {
                        break;
                    }
                    total_size -= self.filesystem.borrow_mut().size(&filepath).unwrap_or(0);
                    let _ = self.filesystem.borrow_mut().unlink(&filepath);
                }
            }

            *CACHE_COLLECTED.lock().unwrap() = Some(true);

            return true;
        }

        false
    }

    /// Files matching `get_finder().date(...)`. Honours the [`CacheMock`] seam when set.
    fn gc_outdated_files(&self, expire_str: &str) -> Vec<std::path::PathBuf> {
        if let Some(mock) = &self.mock
            && let Some(finder) = &mock.finder
        {
            return finder.outdated.clone();
        }

        let mut finder = self.get_finder();
        finder.date(expire_str);
        finder.into_iter().collect()
    }

    /// Files from `get_finder().sort_by_accessed_time()`. Honours the [`CacheMock`] seam when set.
    fn gc_files_by_accessed_time(&self) -> Vec<std::path::PathBuf> {
        if let Some(mock) = &self.mock
            && let Some(finder) = &mock.finder
        {
            return finder.by_accessed_time.clone();
        }

        self.get_finder()
            .sort_by_accessed_time()
            .get_iterator()
            .collect()
    }

    /// For testing only: install the [`CacheMock`] seam used by CacheTest/FileDownloaderTest.
    pub fn __set_mock(&mut self, mock: CacheMock) {
        self.mock = Some(mock);
    }

    /// For testing only: read back the `(ttl, max_size)` calls recorded by the [`CacheMock`] seam.
    pub fn __gc_calls(&self) -> Vec<(i64, i64)> {
        self.mock
            .as_ref()
            .and_then(|m| m.gc_calls.clone())
            .unwrap_or_default()
    }

    pub fn gc_vcs_cache(&mut self, ttl: i64) -> bool {
        if self.is_enabled() {
            let expire = Utc::now() - chrono::Duration::seconds(ttl);

            let mut finder = Finder::create();
            finder
                .r#in(&self.root)
                .directories()
                .depth(0)
                .date(&format!(
                    "until {}",
                    expire.format(date_format_to_strftime("Y-m-d H:i:s"))
                ));
            for file in &mut finder {
                let _ = self.filesystem.borrow_mut().remove_directory(&file);
            }

            *CACHE_COLLECTED.lock().unwrap() = Some(true);

            return true;
        }

        false
    }

    /// @return string|false
    pub fn sha1(&mut self, file: &str) -> Option<String> {
        if self.is_enabled() {
            let file = Preg::replace(format!("{{[^{}]}}i", self.allowlist), "-", file);
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path) {
                return hash_file("sha1", &full_path);
            }
        }

        None
    }

    /// @return string|false
    pub fn sha256(&mut self, file: &str) -> Option<String> {
        if self.is_enabled() {
            let file = Preg::replace(format!("{{[^{}]}}i", self.allowlist), "-", file);
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path) {
                return hash_file("sha256", &full_path);
            }
        }

        None
    }

    pub(crate) fn get_finder(&self) -> Finder {
        let mut finder = Finder::create();
        finder.r#in(&self.root).files();
        finder
    }
}
