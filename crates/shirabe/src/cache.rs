//! ref: composer/src/Composer/Cache.php

use std::sync::Mutex;

use anyhow::Result;
use chrono::Utc;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::finder::finder::Finder;
use shirabe_php_shim::{
    abs, bin2hex, dirname, file_exists, file_get_contents, file_put_contents, filemtime, hash_file,
    is_dir, is_writable, mkdir, random_bytes, random_int, rename, time, unlink,
};

use crate::io::io_interface::IOInterface;
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;
use crate::util::silencer::Silencer;

/// Reads/writes to a filesystem cache
#[derive(Debug)]
pub struct Cache {
    io: Box<dyn IOInterface>,
    root: String,
    enabled: Option<bool>,
    allowlist: String,
    filesystem: Filesystem,
    read_only: bool,
}

/// @var bool|null
static CACHE_COLLECTED: Mutex<Option<bool>> = Mutex::new(None);

impl Cache {
    /// @param string      $cacheDir   location of the cache
    /// @param string      $allowlist  List of characters that are allowed in path names (used in a regex character class)
    /// @param Filesystem  $filesystem optional filesystem instance
    /// @param bool        $readOnly   whether the cache is in readOnly mode
    pub fn new(
        io: Box<dyn IOInterface>,
        cache_dir: &str,
        allowlist: Option<&str>,
        filesystem: Option<Filesystem>,
        read_only: bool,
    ) -> Self {
        let allowlist = allowlist.unwrap_or("a-z0-9._").to_string();
        let root = format!("{}/", cache_dir.trim_end_matches(|c| c == '/' || c == '\\'));
        let filesystem = filesystem.unwrap_or_else(|| Filesystem::new(None));
        let mut this = Self {
            io,
            root,
            allowlist,
            filesystem,
            read_only,
            enabled: None,
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
        !Preg::is_match(r"{(^|[\\\\/])(\$null|nul|NUL|/dev/null)([\\\\/]|$)}", path)
            .unwrap_or(false)
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
            let file = Preg::replace(&format!("{{[^{}]}}i", self.allowlist), "-", file)
                .unwrap_or_default();
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path) {
                self.io
                    .write_error(&format!("Reading {} from cache", full_path));

                return file_get_contents(&full_path);
            }
        }

        None
    }

    pub fn write(&mut self, file: &str, contents: &str) -> Result<bool> {
        let was_enabled = self.enabled == Some(true);

        if self.is_enabled() && !self.read_only {
            let file = Preg::replace(&format!("{{[^{}]}}i", self.allowlist), "-", file)
                .unwrap_or_default();

            self.io
                .write_error(&format!("Writing {}{} into cache", self.root, file));

            let temp_file_name = format!("{}{}{}.tmp", self.root, file, bin2hex(&random_bytes(5)),);
            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch (ErrorException)
            let attempt: Result<bool> = {
                let put = file_put_contents(&temp_file_name, contents.as_bytes());
                Ok(put.is_some()
                    && put != Some(-1)
                    && rename(&temp_file_name, &format!("{}{}", self.root, file)))
            };
            return match attempt {
                Ok(b) => Ok(b),
                Err(e) => {
                    // TODO(phase-b): downcast e to ErrorException; handle partial write cleanup
                    Err(e)
                }
            };
        }

        Ok(false)
    }

    /// Copy a file into the cache
    pub fn copy_from(&mut self, file: &str, source: &str) -> bool {
        if self.is_enabled() && !self.read_only {
            let file = Preg::replace(&format!("{{[^{}]}}i", self.allowlist), "-", file)
                .unwrap_or_default();
            let full_path = format!("{}{}", self.root, file);
            self.filesystem
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

            return self.filesystem.copy(source, &full_path).unwrap_or(false);
        }

        false
    }

    /// Copy a file out of the cache
    pub fn copy_to(&mut self, file: &str, target: &str) -> Result<bool> {
        if self.is_enabled() {
            let file = Preg::replace(&format!("{{[^{}]}}i", self.allowlist), "-", file)
                .unwrap_or_default();
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path) {
                // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
                let touch_result: Result<()> = {
                    shirabe_php_shim::touch(
                        &full_path,
                        // TODO(phase-b): PHP touch signature accepts (filename, mtime, atime)
                    );
                    Ok(())
                };
                if touch_result.is_err() {
                    // fallback in case the above failed due to incorrect ownership
                    // see https://github.com/composer/composer/issues/4070
                    Silencer::call(|| {
                        shirabe_php_shim::touch(&full_path);
                        Ok(())
                    })?;
                }

                self.io
                    .write_error(&format!("Reading {} from cache", full_path));

                return self.filesystem.copy(&full_path, target);
            }
        }

        Ok(false)
    }

    pub fn gc_is_necessary(&self) -> bool {
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

        random_int(0, 50) == 0
    }

    pub fn remove(&mut self, file: &str) -> bool {
        if self.is_enabled() && !self.read_only {
            let file = Preg::replace(&format!("{{[^{}]}}i", self.allowlist), "-", file)
                .unwrap_or_default();
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path) {
                return self.filesystem.unlink(&full_path).unwrap_or(false);
            }
        }

        false
    }

    pub fn clear(&mut self) -> bool {
        if self.is_enabled() && !self.read_only {
            let _ = self.filesystem.empty_directory(&self.root, true);

            return true;
        }

        false
    }

    /// @return int|false
    /// @phpstan-return int<0, max>|false
    pub fn get_age(&mut self, file: &str) -> Option<i64> {
        if self.is_enabled() {
            let file = Preg::replace(&format!("{{[^{}]}}i", self.allowlist), "-", file)
                .unwrap_or_default();
            let full_path = format!("{}{}", self.root, file);
            if file_exists(&full_path) {
                if let Some(mtime) = filemtime(&full_path) {
                    return Some(abs(time() - mtime));
                }
            }
        }

        None
    }

    pub fn gc(&mut self, ttl: i64, max_size: i64) -> bool {
        if self.is_enabled() && !self.read_only {
            let mut expire = Utc::now();
            // PHP: $expire->modify('-'.$ttl.' seconds');
            expire -= chrono::Duration::seconds(ttl);

            let mut finder = self.get_finder();
            finder.date(&format!("until {}", expire.format("%Y-%m-%d %H:%M:%S")));
            for file in &mut finder {
                let _ = self.filesystem.unlink(&file.get_pathname());
            }

            let mut total_size = self.filesystem.size(&self.root).unwrap_or(0);
            if total_size > max_size {
                let mut iterator = self.get_finder().sort_by_accessed_time().get_iterator();
                while total_size > max_size && iterator.valid() {
                    let filepath = iterator.current().get_pathname();
                    total_size -= self.filesystem.size(&filepath).unwrap_or(0);
                    let _ = self.filesystem.unlink(&filepath);
                    iterator.next();
                }
            }

            *CACHE_COLLECTED.lock().unwrap() = Some(true);

            return true;
        }

        false
    }

    pub fn gc_vcs_cache(&mut self, ttl: i64) -> bool {
        if self.is_enabled() {
            let mut expire = Utc::now();
            expire -= chrono::Duration::seconds(ttl);

            let mut finder = Finder::create();
            finder
                .r#in(&self.root)
                .directories()
                .depth(0)
                .date(&format!("until {}", expire.format("%Y-%m-%d %H:%M:%S")));
            for file in &mut finder {
                let _ = self.filesystem.remove_directory(&file.get_pathname());
            }

            *CACHE_COLLECTED.lock().unwrap() = Some(true);

            return true;
        }

        false
    }

    /// @return string|false
    pub fn sha1(&mut self, file: &str) -> Option<String> {
        if self.is_enabled() {
            let file = Preg::replace(&format!("{{[^{}]}}i", self.allowlist), "-", file)
                .unwrap_or_default();
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
            let file = Preg::replace(&format!("{{[^{}]}}i", self.allowlist), "-", file)
                .unwrap_or_default();
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
