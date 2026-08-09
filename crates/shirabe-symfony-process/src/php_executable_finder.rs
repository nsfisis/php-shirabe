//! ref: composer/vendor/symfony/process/PhpExecutableFinder.php

use super::executable_finder::ExecutableFinder;

#[derive(Debug)]
pub struct PhpExecutableFinder {
    executable_finder: ExecutableFinder,
}

impl Default for PhpExecutableFinder {
    fn default() -> Self {
        Self::new()
    }
}

impl PhpExecutableFinder {
    pub fn new() -> Self {
        Self {
            executable_finder: ExecutableFinder::new(),
        }
    }

    /// Finds The PHP executable.
    pub fn find(&self, _include_args: bool) -> Option<String> {
        if let Some(php) = shirabe_php_shim::getenv("PHP_BINARY").filter(|v| !v.is_empty()) {
            let mut php = php.to_string_lossy().into_owned();
            if !shirabe_php_shim::is_executable(&php) {
                match self.executable_finder.find(&php, None, &[]) {
                    Some(found) => php = found,
                    None => return None,
                }
            }

            if shirabe_php_shim::is_dir(&php) {
                return None;
            }

            return Some(php);
        }

        // The original `\PHP_BINARY && \PHP_SAPI` branch describes the running PHP interpreter.
        // These constants cannot be obtained in Rust, the branch is skipped here.

        if let Some(php) = shirabe_php_shim::getenv("PHP_PATH").filter(|v| !v.is_empty()) {
            let php = php.to_string_lossy().into_owned();
            if !shirabe_php_shim::is_executable(&php) || shirabe_php_shim::is_dir(&php) {
                return None;
            }

            return Some(php);
        }

        if let Some(php) = shirabe_php_shim::getenv("PHP_PEAR_PHP_BIN").filter(|v| !v.is_empty()) {
            let php = php.to_string_lossy().into_owned();
            if shirabe_php_shim::is_executable(&php) && !shirabe_php_shim::is_dir(&php) {
                return Some(php);
            }
        }

        // Even if `\PHP_BINDIR` is unavailable, searching `$PATH` should be performed.
        self.executable_finder.find("php", None, &[])
    }

    /// Finds the PHP executable arguments.
    pub fn find_arguments(&self) -> Vec<String> {
        // If PHP_SAPI is not "phpdbg", returns an empty array. In Rust, PHP_SAPI is always "cli",
        // so always returns an empty array.
        vec![]
    }
}
