//! ref: composer/src/Composer/Downloader/ZipDownloader.php

use crate::downloader::archive_downloader::ArchiveDownloader;
use crate::downloader::file_downloader::FileDownloader;
use crate::package::package_interface::PackageInterface;
use crate::util::ini_helper::IniHelper;
use crate::util::platform::Platform;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_external_packages::symfony::component::process::executable_finder::ExecutableFinder;
use shirabe_external_packages::symfony::component::process::process::Process;
use shirabe_php_shim::{
    DIRECTORY_SEPARATOR, ErrorException, RuntimeException, UnexpectedValueException, ZipArchive,
    bin2hex, class_exists, file_exists, file_get_contents, filesize, function_exists, hash_file,
    is_file, json_encode, random_int, version_compare,
};
use std::sync::Mutex;

static UNZIP_COMMANDS: Mutex<Option<Vec<Vec<String>>>> = Mutex::new(None);
static HAS_ZIP_ARCHIVE: Mutex<Option<bool>> = Mutex::new(None);
static IS_WINDOWS: Mutex<Option<bool>> = Mutex::new(None);

#[derive(Debug)]
pub struct ZipDownloader {
    inner: ArchiveDownloader,
    // @phpstan-ignore property.onlyRead (helper property that is set via reflection for testing purposes)
    zip_archive_object: Option<ZipArchive>,
}

impl ZipDownloader {
    pub fn download(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        {
            let mut unzip_commands = UNZIP_COMMANDS.lock().unwrap();
            if unzip_commands.is_none() {
                *unzip_commands = Some(vec![]);
                let finder = ExecutableFinder::new();
                let commands = unzip_commands.as_mut().unwrap();
                if Platform::is_windows() {
                    if let Some(cmd) = finder.find("7z", None, &[r"C:\Program Files\7-Zip"]) {
                        commands.push(vec![
                            "7z".to_string(),
                            cmd,
                            "x".to_string(),
                            "-bb0".to_string(),
                            "-y".to_string(),
                            "%file%".to_string(),
                            "-o%path%".to_string(),
                        ]);
                    }
                }
                if let Some(cmd) = finder.find("unzip", None, &[]) {
                    commands.push(vec![
                        "unzip".to_string(),
                        cmd,
                        "-qq".to_string(),
                        "%file%".to_string(),
                        "-d".to_string(),
                        "%path%".to_string(),
                    ]);
                }
                if !Platform::is_windows() {
                    if let Some(cmd) = finder.find("7z", None, &[]) {
                        // 7z linux/macOS support is only used if unzip is not present
                        commands.push(vec![
                            "7z".to_string(),
                            cmd,
                            "x".to_string(),
                            "-bb0".to_string(),
                            "-y".to_string(),
                            "%file%".to_string(),
                            "-o%path%".to_string(),
                        ]);
                    } else if let Some(cmd) = finder.find("7zz", None, &[]) {
                        // 7zz linux/macOS support is only used if unzip is not present
                        commands.push(vec![
                            "7zz".to_string(),
                            cmd,
                            "x".to_string(),
                            "-bb0".to_string(),
                            "-y".to_string(),
                            "%file%".to_string(),
                            "-o%path%".to_string(),
                        ]);
                    } else if let Some(cmd) = finder.find("7za", None, &[]) {
                        // 7za linux/macOS support is only used if unzip is not present
                        commands.push(vec![
                            "7za".to_string(),
                            cmd,
                            "x".to_string(),
                            "-bb0".to_string(),
                            "-y".to_string(),
                            "%file%".to_string(),
                            "-o%path%".to_string(),
                        ]);
                    }
                }
            }
        }

        let proc_open_missing = !function_exists("proc_open");
        if proc_open_missing {
            *UNZIP_COMMANDS.lock().unwrap() = Some(vec![]);
        }

        {
            let mut has_zip_archive = HAS_ZIP_ARCHIVE.lock().unwrap();
            if has_zip_archive.is_none() {
                *has_zip_archive = Some(class_exists("ZipArchive"));
            }
        }

        let has_zip_archive = HAS_ZIP_ARCHIVE.lock().unwrap().unwrap_or(false);
        let unzip_commands_empty = UNZIP_COMMANDS
            .lock()
            .unwrap()
            .as_ref()
            .map_or(true, |v| v.is_empty());

        if !has_zip_archive && unzip_commands_empty {
            let ini_message = IniHelper::get_message();
            let error = if proc_open_missing {
                format!(
                    "The zip extension is missing and unzip/7z commands cannot be called as proc_open is disabled, skipping.\n{}",
                    ini_message
                )
            } else {
                format!(
                    "The zip extension and unzip/7z commands are both missing, skipping.\n{}",
                    ini_message
                )
            };
            return Err(RuntimeException {
                message: error,
                code: 0,
            }
            .into());
        }

        {
            let mut is_windows_guard = IS_WINDOWS.lock().unwrap();
            if is_windows_guard.is_none() {
                *is_windows_guard = Some(Platform::is_windows());

                if !is_windows_guard.unwrap() && unzip_commands_empty {
                    if proc_open_missing {
                        self.inner.inner.io.write_error("<warning>proc_open is disabled so 'unzip' and '7z' commands cannot be used, zip files are being unpacked using the PHP zip extension.</warning>");
                        self.inner.inner.io.write_error("<warning>This may cause invalid reports of corrupted archives. Besides, any UNIX permissions (e.g. executable) defined in the archives will be lost.</warning>");
                        self.inner.inner.io.write_error("<warning>Enabling proc_open and installing 'unzip' or '7z' (21.01+) may remediate them.</warning>");
                    } else {
                        self.inner.inner.io.write_error("<warning>As there is no 'unzip' nor '7z' command installed zip files are being unpacked using the PHP zip extension.</warning>");
                        self.inner.inner.io.write_error("<warning>This may cause invalid reports of corrupted archives. Besides, any UNIX permissions (e.g. executable) defined in the archives will be lost.</warning>");
                        self.inner.inner.io.write_error("<warning>Installing 'unzip' or '7z' (21.01+) may remediate them.</warning>");
                    }
                }
            }
        }

        self.inner
            .inner
            .download(package, path, prev_package, output)
    }

    fn extract_with_system_unzip(
        &mut self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        static WARNED_7ZIP_LINUX: Mutex<bool> = Mutex::new(false);

        let is_last_chance = !HAS_ZIP_ARCHIVE.lock().unwrap().unwrap_or(false);

        let unzip_commands_empty = UNZIP_COMMANDS
            .lock()
            .unwrap()
            .as_ref()
            .map_or(true, |v| v.is_empty());
        if unzip_commands_empty {
            return self.extract_with_zip_archive(package, file, path);
        }

        let command_spec = UNZIP_COMMANDS.lock().unwrap().as_ref().unwrap()[0].clone();
        let executable = command_spec[0].clone();
        let map: IndexMap<&str, String> = [
            // normalize separators to backslashes to avoid problems with 7-zip on windows
            // see https://github.com/composer/composer/issues/10058
            ("%file%", file.replace('/', DIRECTORY_SEPARATOR)),
            ("%path%", path.replace('/', DIRECTORY_SEPARATOR)),
        ]
        .into_iter()
        .collect();
        let command: Vec<String> = command_spec[1..]
            .iter()
            .map(|value| {
                let mut v = value.clone();
                for (from, to) in &map {
                    v = v.replace(from, to.as_str());
                }
                v
            })
            .collect();

        if !*WARNED_7ZIP_LINUX.lock().unwrap()
            && !Platform::is_windows()
            && ["7z", "7zz", "7za"].contains(&executable.as_str())
        {
            *WARNED_7ZIP_LINUX.lock().unwrap() = true;
            let mut output = String::new();
            if self
                .inner
                .inner
                .process
                .execute(&[command_spec[1].as_str()], &mut output)
                == 0
            {
                if let Some(m) =
                    Preg::is_match_strict_groups(r"^\s*7-Zip(?:\s\[64\])?\s([0-9.]+)", &output)
                {
                    if version_compare(&m[1], "21.01", "<") {
                        self.inner.inner.io.write_error(&format!(
                            "    <warning>Unzipping using {} {} may result in incorrect file permissions. Install {} 21.01+ or unzip to ensure you get correct permissions.</warning>",
                            executable, m[1], executable,
                        ));
                    }
                }
            }
        }

        let io = &self.inner.inner.io;
        let try_fallback = |process_error: anyhow::Error| -> Result<Box<dyn PromiseInterface>> {
            if is_last_chance {
                return Err(process_error);
            }

            if process_error.to_string().contains("zip bomb") {
                return Err(process_error);
            }

            if !is_file(file) {
                io.write_error(&format!("    <warning>{}</warning>", process_error));
                io.write_error("    <warning>This most likely is due to a custom installer plugin not handling the returned Promise from the downloader</warning>");
                io.write_error("    <warning>See https://github.com/composer/installers/commit/5006d0c28730ade233a8f42ec31ac68fb1c5c9bb for an example fix</warning>");
            } else {
                io.write_error(&format!("    <warning>{}</warning>", process_error));
                io.write_error("    The archive may contain identical file names with different capitalization (which fails on case insensitive filesystems)");
                io.write_error(&format!(
                    "    Unzip with {} command failed, falling back to ZipArchive class",
                    executable
                ));

                if Platform::get_env("GITHUB_ACTIONS").is_some()
                    && Platform::get_env("COMPOSER_TESTS_ARE_RUNNING").is_none()
                {
                    io.write_error("    <warning>Additional debug info, please report to https://github.com/composer/composer/issues/11148 if you see this:</warning>");
                    io.write_error(&format!("File size: {}", filesize(file).unwrap_or(0)));
                    io.write_error(&format!(
                        "File SHA1: {}",
                        hash_file("sha1", file).unwrap_or_default()
                    ));
                    let content = file_get_contents(file).unwrap_or_default();
                    let bytes = content.as_bytes();
                    io.write_error(&format!(
                        "First 100 bytes (hex): {}",
                        bin2hex(&bytes[..bytes.len().min(100)])
                    ));
                    let len = bytes.len();
                    io.write_error(&format!(
                        "Last 100 bytes (hex): {}",
                        bin2hex(&bytes[len.saturating_sub(100)..])
                    ));
                    if package.get_dist_url().map_or(false, |u| !u.is_empty()) {
                        io.write_error(&format!(
                            "Origin URL: {}",
                            self.inner
                                .inner
                                .process_url(package, &package.get_dist_url().unwrap_or_default())
                        ));
                        let headers = FileDownloader::response_headers.lock().unwrap();
                        io.write_error(&format!(
                            "Response Headers: {}",
                            json_encode(&shirabe_php_shim::PhpMixed::Null)
                                .unwrap_or_else(|| "[]".to_string())
                        ));
                    }
                }
            }

            self.extract_with_zip_archive(package, file, path)
        };

        match self.inner.inner.process.execute_async(&command) {
            Ok(promise) => Ok(promise.then(
                Box::new(move |process: Process| -> Result<()> {
                    if !process.is_successful() {
                        if self.inner.cleanup_executed.contains_key(package.get_name()) {
                            return Err(RuntimeException {
                                message: format!("Failed to extract {} as the installation was aborted by another package operation.", package.get_name()),
                                code: 0,
                            }.into());
                        }

                        let mut output = process.get_error_output();
                        output = output.replace(&format!(", {}.zip or {}.ZIP", file, file), "");

                        return try_fallback(RuntimeException {
                            message: format!(
                                "Failed to extract {}: ({}) {}\n\n{}",
                                package.get_name(),
                                process.get_exit_code().unwrap_or(0),
                                command.join(" "),
                                output,
                            ),
                            code: 0,
                        }.into());
                    }
                    Ok(())
                }),
                None,
            )),
            Err(e) => try_fallback(e),
        }
    }

    fn extract_with_zip_archive(
        &mut self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        let mut zip_archive = self
            .zip_archive_object
            .take()
            .unwrap_or_else(ZipArchive::new);

        let result: Result<Box<dyn PromiseInterface>> = (|| {
            let retval = if !file_exists(file) || filesize(file).map_or(true, |s| s == 0) {
                Err(-1i64)
            } else {
                zip_archive.open(file, 0)
            };

            if retval.is_ok() {
                let archive_size = filesize(file);
                let total_files = zip_archive.count();
                if total_files > 0 {
                    let mut total_size: i64 = 0;
                    let mut inspect_all = false;
                    let mut files_to_inspect = total_files.min(5);
                    let mut i: i64 = 0;
                    while i < files_to_inspect {
                        let stat_index = if inspect_all {
                            i
                        } else {
                            random_int(0, total_files - 1)
                        };
                        if let Some(stat) = zip_archive.stat_index(stat_index) {
                            let size = stat.get("size").and_then(|v| v.as_int()).unwrap_or(0);
                            let comp_size =
                                stat.get("comp_size").and_then(|v| v.as_int()).unwrap_or(0);
                            total_size += size;
                            if !inspect_all && size > comp_size * 200 {
                                total_size = 0;
                                inspect_all = true;
                                i = -1;
                                files_to_inspect = total_files;
                            }
                        }
                        i += 1;
                    }
                    if let Some(archive_sz) = archive_size {
                        if total_size > archive_sz * 100 && total_size > 50 * 1024 * 1024 {
                            return Err(RuntimeException {
                                message: format!(
                                    "Invalid zip file for \"{}\" with compression ratio >99% (possible zip bomb)",
                                    package.get_name(),
                                ),
                                code: 0,
                            }.into());
                        }
                    }
                }

                let extract_result = zip_archive.extract_to(path);

                if extract_result {
                    zip_archive.close();
                    return Ok(shirabe_external_packages::react::promise::resolve(None));
                }

                return Err(RuntimeException {
                    message: format!(
                        "There was an error extracting the ZIP file for \"{}\", it is either corrupted or using an invalid format.",
                        package.get_name(),
                    ),
                    code: 0,
                }.into());
            } else {
                let code = retval.unwrap_err();
                return Err(UnexpectedValueException {
                    message: self.get_error_message(code, file).trim_end().to_string(),
                    code,
                }
                .into());
            }
        })();

        result.map_err(|e| {
            if let Some(err) = e.downcast_ref::<ErrorException>() {
                RuntimeException {
                    message: format!(
                        "The archive for \"{}\" may contain identical file names with different capitalization (which fails on case insensitive filesystems): {}",
                        package.get_name(),
                        err.message,
                    ),
                    code: 0,
                }.into()
            } else {
                e
            }
        })
    }

    pub(crate) fn extract(
        &mut self,
        package: &dyn PackageInterface,
        file: &str,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.extract_with_system_unzip(package, file, path)
    }

    pub fn get_error_message(&self, retval: i64, file: &str) -> String {
        match retval {
            ZipArchive::ER_EXISTS => format!("File '{}' already exists.", file),
            ZipArchive::ER_INCONS => format!("Zip archive '{}' is inconsistent.", file),
            ZipArchive::ER_INVAL => format!("Invalid argument ({})", file),
            ZipArchive::ER_MEMORY => format!("Malloc failure ({})", file),
            ZipArchive::ER_NOENT => format!("No such zip file: '{}'", file),
            ZipArchive::ER_NOZIP => format!("'{}' is not a zip archive.", file),
            ZipArchive::ER_OPEN => format!("Can't open zip file: {}", file),
            ZipArchive::ER_READ => format!("Zip read error ({})", file),
            ZipArchive::ER_SEEK => format!("Zip seek error ({})", file),
            -1 => format!(
                "'{}' is a corrupted zip archive (0 bytes), try again.",
                file
            ),
            _ => format!(
                "'{}' is not a valid zip archive, got error code: {}",
                file, retval
            ),
        }
    }
}
