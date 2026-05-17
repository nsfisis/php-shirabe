//! ref: composer/src/Composer/Installer/BinaryInstaller.php

use crate::io::io_interface;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    PhpMixed, basename, basename_with_suffix, chmod, dirname, fclose, fgets, file_exists,
    file_get_contents, file_put_contents, fopen, is_dir, is_file, is_link, realpath, rmdir, substr,
    trim, umask,
};

use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;
use crate::util::silencer::Silencer;

/// Utility to handle installation of package "bin"/binaries
#[derive(Debug)]
pub struct BinaryInstaller {
    pub(crate) bin_dir: String,
    pub(crate) bin_compat: String,
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) filesystem: Filesystem,
    vendor_dir: Option<String>,
}

impl BinaryInstaller {
    pub fn new(
        io: Box<dyn IOInterface>,
        bin_dir: String,
        bin_compat: String,
        filesystem: Option<Filesystem>,
        vendor_dir: Option<String>,
    ) -> Self {
        let filesystem = filesystem.unwrap_or_else(Filesystem::new);
        Self {
            bin_dir,
            bin_compat,
            io,
            filesystem,
            vendor_dir,
        }
    }

    pub fn install_binaries(
        &mut self,
        package: &dyn PackageInterface,
        install_path: &str,
        warn_on_overwrite: bool,
    ) {
        let binaries = self.get_binaries(package);
        if binaries.is_empty() {
            return;
        }

        Platform::workaround_filesystem_issues();

        for bin in &binaries {
            let mut bin_path = format!("{}/{}", install_path, bin);
            if !file_exists(&bin_path) {
                self.io.write_error(
                    PhpMixed::String(format!(
                        "    <warning>Skipped installation of bin {} for package {}: file not found in package</warning>",
                        bin,
                        package.get_name(),
                    )),
                    true,
                    io_interface::NORMAL,
                );
                continue;
            }
            if is_dir(&bin_path) {
                self.io.write_error(
                    PhpMixed::String(format!(
                        "    <warning>Skipped installation of bin {} for package {}: found a directory at that path</warning>",
                        bin,
                        package.get_name(),
                    )),
                    true,
                    io_interface::NORMAL,
                );
                continue;
            }
            if !self.filesystem.is_absolute_path(&bin_path) {
                // in case a custom installer returned a relative path for the
                // $package, we can now safely turn it into a absolute path (as we
                // already checked the binary's existence). The following helpers
                // will require absolute paths to work properly.
                bin_path = realpath(&bin_path).unwrap_or_default();
            }
            self.initialize_bin_dir();
            let link = format!("{}/{}", self.bin_dir, basename(bin));
            if file_exists(&link) {
                if !is_link(&link) {
                    if warn_on_overwrite {
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "    Skipped installation of bin {} for package {}: name conflicts with an existing file",
                                bin,
                                package.get_name(),
                            )),
                            true,
                            io_interface::NORMAL,
                        );
                    }
                    continue;
                }
                if realpath(&link) == realpath(&bin_path) {
                    // It is a linked binary from a previous installation, which can be replaced with a proxy file
                    self.filesystem.unlink(&link);
                }
            }

            let mut bin_compat = self.bin_compat.clone();
            if bin_compat == "auto"
                && (Platform::is_windows() || Platform::is_windows_subsystem_for_linux())
            {
                bin_compat = "full".to_string();
            }

            if bin_compat == "full" {
                self.install_full_binaries(&bin_path, &link, bin, package);
            } else {
                self.install_unixy_proxy_binaries(&bin_path, &link);
            }
            let _ = Silencer::call(|| {
                chmod(&bin_path, 0o777 & !umask());
                Ok(())
            });
        }
    }

    pub fn remove_binaries(&mut self, package: &dyn PackageInterface) {
        self.initialize_bin_dir();

        let binaries = self.get_binaries(package);
        if binaries.is_empty() {
            return;
        }
        for bin in &binaries {
            let link = format!("{}/{}", self.bin_dir, basename(bin));
            if is_link(&link) || file_exists(&link) {
                // still checking for symlinks here for legacy support
                self.filesystem.unlink(&link);
            }
            if is_file(&format!("{}.bat", link)) {
                self.filesystem.unlink(&format!("{}.bat", link));
            }
        }

        // attempt removing the bin dir in case it is left empty
        if is_dir(&self.bin_dir) && self.filesystem.is_dir_empty(&self.bin_dir) {
            let bin_dir = self.bin_dir.clone();
            let _ = Silencer::call(|| {
                rmdir(&bin_dir);
                Ok(())
            });
        }
    }

    pub fn determine_binary_caller(bin: &str) -> String {
        if ".bat" == substr(bin, -4, None) || ".exe" == substr(bin, -4, None) {
            return "call".to_string();
        }

        let handle = fopen(bin, "r");
        let line = fgets(handle.clone()).unwrap_or_default();
        fclose(handle);
        if let Some(m) =
            Preg::is_match_strict_groups(r"{^#!/(?:usr/bin/env )?(?:[^/]+/)*(.+)$}m", &line)
        {
            return trim(m.get(1).map(|s| s.as_str()).unwrap_or(""), None);
        }

        "php".to_string()
    }

    /// @return string[]
    pub(crate) fn get_binaries(&self, package: &dyn PackageInterface) -> Vec<String> {
        package.get_binaries()
    }

    pub(crate) fn install_full_binaries(
        &mut self,
        bin_path: &str,
        link: &str,
        bin: &str,
        package: &dyn PackageInterface,
    ) {
        let mut link = link.to_string();
        // add unixy support for cygwin and similar environments
        if ".bat" != substr(bin_path, -4, None) {
            self.install_unixy_proxy_binaries(bin_path, &link);
            link.push_str(".bat");
            if file_exists(&link) {
                self.io.write_error(
                    PhpMixed::String(format!(
                        "    Skipped installation of bin {}.bat proxy for package {}: a .bat proxy was already installed",
                        bin,
                        package.get_name(),
                    )),
                    true,
                    io_interface::NORMAL,
                );
            }
        }
        if !file_exists(&link) {
            let code = self.generate_windows_proxy_code(bin_path, &link);
            file_put_contents(&link, code.as_bytes());
            let link_clone = link.clone();
            let _ = Silencer::call(|| {
                chmod(&link_clone, 0o777 & !umask());
                Ok(())
            });
        }
    }

    pub(crate) fn install_unixy_proxy_binaries(&self, bin_path: &str, link: &str) {
        let code = self.generate_unixy_proxy_code(bin_path, link);
        file_put_contents(link, code.as_bytes());
        let link_owned = link.to_string();
        let _ = Silencer::call(|| {
            chmod(&link_owned, 0o777 & !umask());
            Ok(())
        });
    }

    pub(crate) fn initialize_bin_dir(&mut self) {
        self.filesystem.ensure_directory_exists(&self.bin_dir);
        // TODO(phase-b): PHP assigns realpath(...) even when realpath returns false
        self.bin_dir = realpath(&self.bin_dir).unwrap_or_default();
    }

    pub(crate) fn generate_windows_proxy_code(&self, bin: &str, link: &str) -> String {
        let bin_path = self.filesystem.find_shortest_path(link, bin, false);
        let caller = Self::determine_binary_caller(bin);

        // if the target is a php file, we run the unixy proxy file
        // to ensure that _composer_autoload_path gets defined, instead
        // of running the binary directly
        if caller == "php" {
            return format!(
                "@ECHO OFF\r\n\
                 setlocal DISABLEDELAYEDEXPANSION\r\n\
                 SET BIN_TARGET=%~dp0/{}\r\n\
                 SET COMPOSER_RUNTIME_BIN_DIR=%~dp0\r\n\
                 {} \"%BIN_TARGET%\" %*\r\n",
                trim(
                    &ProcessExecutor::escape(&basename_with_suffix(link, ".bat")),
                    Some("\"'")
                ),
                caller,
            );
        }

        format!(
            "@ECHO OFF\r\n\
             setlocal DISABLEDELAYEDEXPANSION\r\n\
             SET BIN_TARGET=%~dp0/{}\r\n\
             SET COMPOSER_RUNTIME_BIN_DIR=%~dp0\r\n\
             {} \"%BIN_TARGET%\" %*\r\n",
            trim(&ProcessExecutor::escape(&bin_path), Some("\"'")),
            caller,
        )
    }

    pub(crate) fn generate_unixy_proxy_code(&self, bin: &str, link: &str) -> String {
        let bin_path = self.filesystem.find_shortest_path(link, bin, false);

        let bin_dir = ProcessExecutor::escape(&dirname(&bin_path));
        let bin_file = basename(&bin_path);

        // PHP: file_get_contents($bin, false, null, 0, 500) — limit 500 bytes
        // TODO(phase-b): file_get_contents shim does not support offset/maxlen
        let bin_contents = file_get_contents(bin).unwrap_or_default();
        // For php files, we generate a PHP proxy instead of a shell one,
        // which allows calling the proxy with a custom php process
        if let Some(m) =
            Preg::is_match_with_indexed_captures(r"{^(#!.*\r?\n)?[\r\n\t ]*<\?php}", &bin_contents)
                .ok()
                .flatten()
        {
            // carry over the existing shebang if present, otherwise add our own
            let proxy_code = if m.get(1).is_none() {
                "#!/usr/bin/env php".to_string()
            } else {
                trim(m.get(1).map(|s| s.as_str()).unwrap_or(""), None)
            };
            let bin_path_exported = self
                .filesystem
                .find_shortest_path_code(link, bin, false, true);
            let mut stream_proxy_code = String::new();
            let mut stream_hint = String::new();
            let mut globals_code = format!("$GLOBALS['_composer_bin_dir'] = __DIR__;\n",);
            let mut phpunit_hack1 = String::new();
            let mut phpunit_hack2 = String::new();
            // Don't expose autoload path when vendor dir was not set in custom installers
            if let Some(vendor_dir) = &self.vendor_dir {
                // ensure comparisons work accurately if the CWD is a symlink, as $link is realpath'd already
                let vendor_dir_real = realpath(vendor_dir).unwrap_or_else(|| vendor_dir.clone());
                globals_code.push_str(&format!(
                    "$GLOBALS['_composer_autoload_path'] = {};\n",
                    self.filesystem.find_shortest_path_code(
                        link,
                        &format!("{}/autoload.php", vendor_dir_real),
                        false,
                        true,
                    ),
                ));
            }
            // Add workaround for PHPUnit process isolation
            if let Some(vendor_dir) = &self.vendor_dir {
                if self.filesystem.normalize_path(bin)
                    == self
                        .filesystem
                        .normalize_path(&format!("{}/phpunit/phpunit/phpunit", vendor_dir))
                {
                    // workaround issue on PHPUnit 6.5+ running on PHP 8+
                    globals_code.push_str(&format!(
                        "$GLOBALS['__PHPUNIT_ISOLATION_EXCLUDE_LIST'] = $GLOBALS['__PHPUNIT_ISOLATION_BLACKLIST'] = array(realpath({}));\n",
                        bin_path_exported,
                    ));
                    // workaround issue on all PHPUnit versions running on PHP <8
                    phpunit_hack1 = "'phpvfscomposer://'.".to_string();
                    phpunit_hack2 = "
                $data = str_replace('__DIR__', var_export(dirname($this->realpath), true), $data);
                $data = str_replace('__FILE__', var_export($this->realpath, true), $data);"
                        .to_string();
                }
            }
            if trim(m.get(0).map(|s| s.as_str()).unwrap_or(""), None) != "<?php" {
                stream_hint = format!(
                    " using a stream wrapper to prevent the shebang from being output on PHP<8\n *"
                );
                stream_proxy_code = format!(
                    "if (PHP_VERSION_ID < 80000) {{\n    if (!class_exists('Composer\\BinProxyWrapper')) {{\n        /**\n         * @internal\n         */\n        final class BinProxyWrapper\n        {{\n            private $handle;\n            private $position;\n            private $realpath;\n\n            public function stream_open($path, $mode, $options, &$opened_path)\n            {{\n                // get rid of phpvfscomposer:// prefix for __FILE__ & __DIR__ resolution\n                $opened_path = substr($path, 17);\n                $this->realpath = realpath($opened_path) ?: $opened_path;\n                $opened_path = {phpunit_hack1}$this->realpath;\n                $this->handle = fopen($this->realpath, $mode);\n                $this->position = 0;\n\n                return (bool) $this->handle;\n            }}\n\n            public function stream_read($count)\n            {{\n                $data = fread($this->handle, $count);\n\n                if ($this->position === 0) {{\n                    $data = preg_replace('{{^#!.*\\r?\\n}}', '', $data);\n                }}{phpunit_hack2}\n\n                $this->position += strlen($data);\n\n                return $data;\n            }}\n\n            public function stream_cast($castAs)\n            {{\n                return $this->handle;\n            }}\n\n            public function stream_close()\n            {{\n                fclose($this->handle);\n            }}\n\n            public function stream_lock($operation)\n            {{\n                return $operation ? flock($this->handle, $operation) : true;\n            }}\n\n            public function stream_seek($offset, $whence)\n            {{\n                if (0 === fseek($this->handle, $offset, $whence)) {{\n                    $this->position = ftell($this->handle);\n                    return true;\n                }}\n\n                return false;\n            }}\n\n            public function stream_tell()\n            {{\n                return $this->position;\n            }}\n\n            public function stream_eof()\n            {{\n                return feof($this->handle);\n            }}\n\n            public function stream_stat()\n            {{\n                return array();\n            }}\n\n            public function stream_set_option($option, $arg1, $arg2)\n            {{\n                return true;\n            }}\n\n            public function url_stat($path, $flags)\n            {{\n                $path = substr($path, 17);\n                if (file_exists($path)) {{\n                    return stat($path);\n                }}\n\n                return false;\n            }}\n        }}\n    }}\n\n    if (\n        (function_exists('stream_get_wrappers') && in_array('phpvfscomposer', stream_get_wrappers(), true))\n        || (function_exists('stream_wrapper_register') && stream_wrapper_register('phpvfscomposer', 'Composer\\BinProxyWrapper'))\n    ) {{\n        return include(\"phpvfscomposer://\" . {bin_path_exported});\n    }}\n}}\n",
                    phpunit_hack1 = phpunit_hack1,
                    phpunit_hack2 = phpunit_hack2,
                    bin_path_exported = bin_path_exported,
                );
            }

            return format!(
                "{}\n<?php\n\n/**\n * Proxy PHP file generated by Composer\n *\n * This file includes the referenced bin path ({})\n *{}\n * @generated\n */\n\nnamespace Composer;\n\n{}\n{}\nreturn include {};\n",
                proxy_code,
                bin_path,
                stream_hint,
                globals_code,
                stream_proxy_code,
                bin_path_exported,
            );
        }

        format!(
            "#!/usr/bin/env sh\n\
             \n\
             # Support bash to support `source` with fallback on $0 if this does not run with bash\n\
             # https://stackoverflow.com/a/35006505/6512\n\
             selfArg=\"$BASH_SOURCE\"\n\
             if [ -z \"$selfArg\" ]; then\n\
             \x20\x20\x20\x20selfArg=\"$0\"\n\
             fi\n\
             \n\
             self=$(realpath \"$selfArg\" 2> /dev/null)\n\
             if [ -z \"$self\" ]; then\n\
             \x20\x20\x20\x20self=\"$selfArg\"\n\
             fi\n\
             \n\
             dir=$(cd \"${{self%[/\\\\]*}}\" > /dev/null; cd {bin_dir} && pwd)\n\
             \n\
             if [ -d /proc/cygdrive ]; then\n\
             \x20\x20\x20\x20case $(which php) in\n\
             \x20\x20\x20\x20\x20\x20\x20\x20$(readlink -n /proc/cygdrive)/*)\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20# We are in Cygwin using Windows php, so the path must be translated\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20dir=$(cygpath -m \"$dir\");\n\
             \x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20\x20;;\n\
             \x20\x20\x20\x20esac\n\
             fi\n\
             \n\
             export COMPOSER_RUNTIME_BIN_DIR=\"$(cd \"${{self%[/\\\\]*}}\" > /dev/null; pwd)\"\n\
             \n\
             # If bash is sourcing this file, we have to source the target as well\n\
             bashSource=\"$BASH_SOURCE\"\n\
             if [ -n \"$bashSource\" ]; then\n\
             \x20\x20\x20\x20if [ \"$bashSource\" != \"$0\" ]; then\n\
             \x20\x20\x20\x20\x20\x20\x20\x20source \"${{dir}}/{bin_file}\" \"$@\"\n\
             \x20\x20\x20\x20\x20\x20\x20\x20return\n\
             \x20\x20\x20\x20fi\n\
             fi\n\
             \n\
             exec \"${{dir}}/{bin_file}\" \"$@\"\n",
            bin_dir = bin_dir,
            bin_file = bin_file,
        )
    }
}
