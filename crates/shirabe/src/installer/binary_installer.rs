//! ref: composer/src/Composer/Installer/BinaryInstaller.php sha1:098d10272c26a2dcf2ef4bf287922b35ca147bdc

use crate::io::{IOInterface, IOInterfaceWrite};
use crate::package::PackageInterfaceHandle;
use crate::util::{Filesystem, Platform, ProcessExecutor};
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    basename, basename_with_suffix, bformat, bwrite, chmod, dirname, fclose, fgets, file_exists,
    file_get_contents_with_max_length, file_put_contents, fopen, is_dir, is_file, is_link,
    path_from_bytes, path_to_bytes, php_regex, preg_match, realpath, rmdir, substr, trim,
    trim_chars, umask,
};
use std::path::{Path, PathBuf};

/// Seam over the BinaryInstaller methods reached through LibraryInstaller, so tests can inject a
/// recording double. Composer has no PHP `BinaryInstallerInterface`; this exists only to allow the
/// `LibraryInstaller` collaborator to be mocked the way PHPUnit mocks the concrete class.
pub trait BinaryInstallerInterface: std::fmt::Debug {
    fn install_binaries(
        &mut self,
        package: PackageInterfaceHandle,
        install_path: &Path,
        warn_on_overwrite: bool,
    ) -> anyhow::Result<()>;
    fn remove_binaries(&mut self, package: PackageInterfaceHandle);
}

impl BinaryInstallerInterface for BinaryInstaller {
    fn install_binaries(
        &mut self,
        package: PackageInterfaceHandle,
        install_path: &Path,
        warn_on_overwrite: bool,
    ) -> anyhow::Result<()> {
        BinaryInstaller::install_binaries(self, package, install_path, warn_on_overwrite)
    }

    fn remove_binaries(&mut self, package: PackageInterfaceHandle) {
        BinaryInstaller::remove_binaries(self, package);
    }
}

/// Utility to handle installation of package "bin"/binaries
#[derive(Debug)]
pub struct BinaryInstaller {
    bin_dir: PathBuf,
    bin_compat: BString,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    filesystem: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    vendor_dir: Option<PathBuf>,
}

impl BinaryInstaller {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        bin_dir: PathBuf,
        bin_compat: BString,
        filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
        vendor_dir: Option<PathBuf>,
    ) -> Self {
        let filesystem = filesystem
            .unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))));
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
        package: PackageInterfaceHandle,
        install_path: &Path,
        warn_on_overwrite: bool,
    ) -> anyhow::Result<()> {
        let binaries = self.get_binaries(package.clone());
        if binaries.is_empty() {
            return Ok(());
        }

        Platform::workaround_filesystem_issues();

        for bin in &binaries {
            let mut bin_path = PathBuf::from(path_from_bytes(&bformat!("{install_path}/{bin}")));
            if !file_exists(&bin_path) {
                self.io.write_error(bformat!("    <warning>Skipped installation of bin {bin} for package {}: file not found in package</warning>", package.get_name()));
                continue;
            }
            if is_dir(&bin_path) {
                self.io.write_error(bformat!("    <warning>Skipped installation of bin {bin} for package {}: found a directory at that path</warning>", package.get_name()));
                continue;
            }
            if !self.filesystem.borrow_mut().is_absolute_path(&bin_path) {
                // in case a custom installer returned a relative path for the
                // $package, we can now safely turn it into a absolute path (as we
                // already checked the binary's existence). The following helpers
                // will require absolute paths to work properly.
                bin_path = realpath(&bin_path).unwrap_or_default();
            }
            self.initialize_bin_dir();
            let link = PathBuf::from(path_from_bytes(&bformat!(
                "{}/{}",
                self.bin_dir,
                basename(bin)
            )));
            if file_exists(&link) {
                if !is_link(&link) {
                    if warn_on_overwrite {
                        self.io.write_error(bformat!("    Skipped installation of bin {bin} for package {}: name conflicts with an existing file", package.get_name()));
                    }
                    continue;
                }
                if realpath(&link) == realpath(&bin_path) {
                    // It is a linked binary from a previous installation, which can be replaced with a proxy file
                    self.filesystem.borrow_mut().unlink(&link);
                }
            }

            let mut bin_compat = self.bin_compat.clone();
            if bin_compat == "auto"
                && (Platform::is_windows() || Platform::is_windows_subsystem_for_linux())
            {
                bin_compat = BString::from("full");
            }

            if bin_compat == "full" {
                self.install_full_binaries(&bin_path, &link, bin.as_bstr(), package.clone())?;
            } else {
                self.install_unixy_proxy_binaries(&bin_path, &link)?;
            }
            chmod(&bin_path, 0o777 & !umask());
        }

        Ok(())
    }

    pub fn remove_binaries(&mut self, package: PackageInterfaceHandle) {
        self.initialize_bin_dir();

        let binaries = self.get_binaries(package);
        if binaries.is_empty() {
            return;
        }
        for bin in &binaries {
            let link = PathBuf::from(path_from_bytes(&bformat!(
                "{}/{}",
                self.bin_dir,
                basename(bin)
            )));
            if is_link(&link) || file_exists(&link) {
                // still checking for symlinks here for legacy support
                self.filesystem.borrow_mut().unlink(&link);
            }
            let bat = PathBuf::from(path_from_bytes(&bformat!("{link}.bat")));
            if is_file(&bat) {
                self.filesystem.borrow_mut().unlink(&bat);
            }
        }

        // attempt removing the bin dir in case it is left empty
        if is_dir(&self.bin_dir) && self.filesystem.borrow_mut().is_dir_empty(&self.bin_dir) {
            let _ = rmdir(&self.bin_dir);
        }
    }

    pub fn determine_binary_caller(bin: &Path) -> BString {
        let bin_bytes = path_to_bytes(bin);
        if ".bat" == substr(bin_bytes, -4, None) || ".exe" == substr(bin_bytes, -4, None) {
            return BString::from("call");
        }

        let line = match fopen(bin, "r") {
            Ok(handle) => {
                let line = fgets(&handle, None).unwrap_or_default();
                fclose(&handle);
                line
            }
            Err(_) => BString::default(),
        };
        if let Some(m) = preg_match(
            php_regex!(r"{^#!/(?:usr/bin/env )?(?:[^/]+/)*(.+)$}m"),
            &line,
        ) {
            return trim(m.get(1).unwrap_or_default());
        }

        BString::from("php")
    }

    fn get_binaries(&self, package: PackageInterfaceHandle) -> Vec<BString> {
        package.get_binaries()
    }

    fn install_full_binaries(
        &mut self,
        bin_path: &Path,
        link: &Path,
        bin: &BStr,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<()> {
        let mut link = link.to_path_buf();
        // add unixy support for cygwin and similar environments
        if ".bat" != substr(path_to_bytes(bin_path), -4, None) {
            self.install_unixy_proxy_binaries(bin_path, &link)?;
            link = PathBuf::from(path_from_bytes(&bformat!("{link}.bat")));
            if file_exists(&link) {
                self.io.write_error(bformat!("    Skipped installation of bin {bin}.bat proxy for package {}: a .bat proxy was already installed", package.get_name()));
            }
        }
        if !file_exists(&link) {
            let code = self.generate_windows_proxy_code(bin_path, &link)?;
            file_put_contents(&link, &code);
            chmod(&link, 0o777 & !umask());
        }

        Ok(())
    }

    fn install_unixy_proxy_binaries(&self, bin_path: &Path, link: &Path) -> anyhow::Result<()> {
        let code = self.generate_unixy_proxy_code(bin_path, link)?;
        file_put_contents(link, &code);
        chmod(link, 0o777 & !umask());

        Ok(())
    }

    fn initialize_bin_dir(&mut self) {
        self.filesystem
            .borrow_mut()
            .ensure_directory_exists(&self.bin_dir);
        self.bin_dir = realpath(&self.bin_dir).unwrap_or_default();
    }

    fn generate_windows_proxy_code(&self, bin: &Path, link: &Path) -> anyhow::Result<BString> {
        let bin_path = self
            .filesystem
            .borrow_mut()
            .find_shortest_path(link, bin, false, false)?;
        let caller = Self::determine_binary_caller(bin);

        // if the target is a php file, we run the unixy proxy file
        // to ensure that _composer_autoload_path gets defined, instead
        // of running the binary directly
        if caller == "php" {
            return Ok(bformat!(
                "@ECHO OFF\r\nsetlocal DISABLEDELAYEDEXPANSION\r\nSET BIN_TARGET=%~dp0/{}\r\nSET COMPOSER_RUNTIME_BIN_DIR=%~dp0\r\n{caller} \"%BIN_TARGET%\" %*\r\n",
                trim_chars(
                    ProcessExecutor::escape(basename_with_suffix(path_to_bytes(link), ".bat")),
                    "\"'"
                )
            ));
        }

        Ok(bformat!(
            "@ECHO OFF\r\nsetlocal DISABLEDELAYEDEXPANSION\r\nSET BIN_TARGET=%~dp0/{}\r\nSET COMPOSER_RUNTIME_BIN_DIR=%~dp0\r\n{caller} \"%BIN_TARGET%\" %*\r\n",
            trim_chars(ProcessExecutor::escape(path_to_bytes(&bin_path)), "\"'")
        ))
    }

    fn generate_unixy_proxy_code(&self, bin: &Path, link: &Path) -> anyhow::Result<BString> {
        let bin_path = self
            .filesystem
            .borrow_mut()
            .find_shortest_path(link, bin, false, false)?;

        let bin_dir = ProcessExecutor::escape(dirname(path_to_bytes(&bin_path)));
        let bin_file = basename(path_to_bytes(&bin_path));

        let bin_contents = file_get_contents_with_max_length(bin, 500).unwrap_or_default();
        // For php files, we generate a PHP proxy instead of a shell one,
        // which allows calling the proxy with a custom php process
        if let Some(m) = preg_match(
            php_regex!(r"{^(#!.*\r?\n)?[\r\n\t ]*<\?php}"),
            &bin_contents,
        ) {
            // carry over the existing shebang if present, otherwise add our own
            let proxy_code = match m.get(1) {
                None => BString::from("#!/usr/bin/env php"),
                Some(shebang) => trim(shebang),
            };
            let bin_path_exported = self
                .filesystem
                .borrow()
                .find_shortest_path_code(link, bin, false, true, false)?;
            let mut stream_proxy_code = BString::default();
            let mut stream_hint = BString::default();
            let mut globals_code = BString::from("$GLOBALS['_composer_bin_dir'] = __DIR__;\n");
            let mut phpunit_hack1 = BString::default();
            let mut phpunit_hack2 = BString::default();
            // Don't expose autoload path when vendor dir was not set in custom installers
            if let Some(vendor_dir) = &self.vendor_dir {
                // ensure comparisons work accurately if the CWD is a symlink, as $link is realpath'd already
                let vendor_dir_real = realpath(vendor_dir).unwrap_or_else(|| vendor_dir.clone());
                bwrite!(
                    globals_code,
                    "$GLOBALS['_composer_autoload_path'] = {};\n",
                    self.filesystem.borrow_mut().find_shortest_path_code(
                        link,
                        path_from_bytes(&bformat!("{vendor_dir_real}/autoload.php")),
                        false,
                        true,
                        false,
                    )?
                );
            }
            // Add workaround for PHPUnit process isolation
            if let Some(vendor_dir) = &self.vendor_dir
                && self.filesystem.borrow().normalize_path(bin).as_os_str()
                    == self
                        .filesystem
                        .borrow()
                        .normalize_path(path_from_bytes(&bformat!(
                            "{vendor_dir}/phpunit/phpunit/phpunit"
                        )))
                        .as_os_str()
            {
                // workaround issue on PHPUnit 6.5+ running on PHP 8+
                bwrite!(
                    globals_code,
                    "$GLOBALS['__PHPUNIT_ISOLATION_EXCLUDE_LIST'] = $GLOBALS['__PHPUNIT_ISOLATION_BLACKLIST'] = array(realpath({bin_path_exported}));\n"
                );
                // workaround issue on all PHPUnit versions running on PHP <8
                phpunit_hack1 = BString::from("'phpvfscomposer://'.");
                phpunit_hack2 = BString::from(
                    "
                $data = str_replace('__DIR__', var_export(dirname($this->realpath), true), $data);
                $data = str_replace('__FILE__', var_export($this->realpath, true), $data);",
                );
            }
            if trim(m.get(0).unwrap_or_default()) != "<?php" {
                stream_hint = BString::from(
                    " using a stream wrapper to prevent the shebang from being output on PHP<8\n *",
                );
                stream_proxy_code = bformat!(
                    "if (PHP_VERSION_ID < 80000) {{\n    if (!class_exists('Composer\\BinProxyWrapper')) {{\n        /**\n         * @internal\n         */\n        final class BinProxyWrapper\n        {{\n            private $handle;\n            private $position;\n            private $realpath;\n\n            public function stream_open($path, $mode, $options, &$opened_path)\n            {{\n                // get rid of phpvfscomposer:// prefix for __FILE__ & __DIR__ resolution\n                $opened_path = substr($path, 17);\n                $this->realpath = realpath($opened_path) ?: $opened_path;\n                $opened_path = {phpunit_hack1}$this->realpath;\n                $this->handle = fopen($this->realpath, $mode);\n                $this->position = 0;\n\n                return (bool) $this->handle;\n            }}\n\n            public function stream_read($count)\n            {{\n                $data = fread($this->handle, $count);\n\n                if ($this->position === 0) {{\n                    $data = preg_replace('{{^#!.*\\r?\\n}}', '', $data);\n                }}{phpunit_hack2}\n\n                $this->position += strlen($data);\n\n                return $data;\n            }}\n\n            public function stream_cast($castAs)\n            {{\n                return $this->handle;\n            }}\n\n            public function stream_close()\n            {{\n                fclose($this->handle);\n            }}\n\n            public function stream_lock($operation)\n            {{\n                return $operation ? flock($this->handle, $operation) : true;\n            }}\n\n            public function stream_seek($offset, $whence)\n            {{\n                if (0 === fseek($this->handle, $offset, $whence)) {{\n                    $this->position = ftell($this->handle);\n                    return true;\n                }}\n\n                return false;\n            }}\n\n            public function stream_tell()\n            {{\n                return $this->position;\n            }}\n\n            public function stream_eof()\n            {{\n                return feof($this->handle);\n            }}\n\n            public function stream_stat()\n            {{\n                return array();\n            }}\n\n            public function stream_set_option($option, $arg1, $arg2)\n            {{\n                return true;\n            }}\n\n            public function url_stat($path, $flags)\n            {{\n                $path = substr($path, 17);\n                if (file_exists($path)) {{\n                    return stat($path);\n                }}\n\n                return false;\n            }}\n        }}\n    }}\n\n    if (\n        (function_exists('stream_get_wrappers') && in_array('phpvfscomposer', stream_get_wrappers(), true))\n        || (function_exists('stream_wrapper_register') && stream_wrapper_register('phpvfscomposer', 'Composer\\BinProxyWrapper'))\n    ) {{\n        return include(\"phpvfscomposer://\" . {bin_path_exported});\n    }}\n}}\n"
                );
            }

            return Ok(bformat!(
                "{proxy_code}\n<?php\n\n/**\n * Proxy PHP file generated by Composer\n *\n * This file includes the referenced bin path ({bin_path})\n *{stream_hint}\n * @generated\n */\n\nnamespace Composer;\n\n{globals_code}\n{stream_proxy_code}\nreturn include {bin_path_exported};\n"
            ));
        }

        Ok(bformat!(
            "#!/usr/bin/env sh\n\n# Support bash to support `source` with fallback on $0 if this does not run with bash\n# https://stackoverflow.com/a/35006505/6512\nselfArg=\"$BASH_SOURCE\"\nif [ -z \"$selfArg\" ]; then\n    selfArg=\"$0\"\nfi\n\nself=$(realpath \"$selfArg\" 2> /dev/null)\nif [ -z \"$self\" ]; then\n    self=\"$selfArg\"\nfi\n\ndir=$(cd \"${{self%[/\\\\]*}}\" > /dev/null; cd {bin_dir} && pwd)\n\nif [ -d /proc/cygdrive ]; then\n    case $(which php) in\n        $(readlink -n /proc/cygdrive)/*)\n            # We are in Cygwin using Windows php, so the path must be translated\n            dir=$(cygpath -m \"$dir\");\n            ;;\n    esac\nfi\n\nexport COMPOSER_RUNTIME_BIN_DIR=\"$(cd \"${{self%[/\\\\]*}}\" > /dev/null; pwd)\"\n\n# If bash is sourcing this file, we have to source the target as well\nbashSource=\"$BASH_SOURCE\"\nif [ -n \"$bashSource\" ]; then\n    if [ \"$bashSource\" != \"$0\" ]; then\n        source \"${{dir}}/{bin_file}\" \"$@\"\n        return\n    fi\nfi\n\nexec \"${{dir}}/{bin_file}\" \"$@\"\n"
        ))
    }
}
