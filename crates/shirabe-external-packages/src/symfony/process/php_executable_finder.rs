//! ref: composer/vendor/symfony/process/PhpExecutableFinder.php

use super::executable_finder::ExecutableFinder;
use shirabe_php_shim::{self as php};

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
    pub fn find(&self, include_args: bool) -> Option<String> {
        if let Some(php) = php::getenv("PHP_BINARY").filter(|v| !v.is_empty()) {
            let mut php = php.to_string_lossy().into_owned();
            if !php::is_executable(&php) {
                match self.executable_finder.find(&php, None, &[]) {
                    Some(found) => php = found,
                    None => return None,
                }
            }

            if php::is_dir(&php) {
                return None;
            }

            return Some(php);
        }

        let args = self.find_arguments();
        let _args = if include_args && !args.is_empty() {
            format!(" {}", args.join(" "))
        } else {
            String::new()
        };

        // PHP_BINARY return the current sapi executable
        //
        // Everything from here on depends on runtime constants describing the *running* PHP
        // interpreter (\PHP_BINARY truthiness, \PHP_SAPI, \PHP_BINDIR). The shim does not model a
        // current PHP runtime, so the remaining fallbacks (the \PHP_SAPI sapi check, \PHP_PATH,
        // \PHP_PEAR_PHP_BIN, \PHP_BINDIR probing and the final php lookup seeded with \PHP_BINDIR)
        // cannot be ported faithfully here.
        // TODO(php-runtime): port once the shim exposes \PHP_SAPI and \PHP_BINDIR.
        todo!()
    }

    /// Finds the PHP executable arguments.
    pub fn find_arguments(&self) -> Vec<String> {
        let _arguments: Vec<String> = vec![];
        // TODO(php-runtime): \PHP_SAPI is the SAPI name of the running PHP interpreter; the shim
        // does not model a current PHP runtime, so the 'phpdbg' check cannot be ported faithfully.
        todo!()
    }
}
