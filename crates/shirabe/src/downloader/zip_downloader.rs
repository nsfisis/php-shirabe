//! ref: composer/src/Composer/Downloader/ZipDownloader.php

use crate::downloader::{ArchiveDownloader, ChangeReportInterface, FileDownloader};
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::util::{IniHelper, Platform};
use bstr::{BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, CmpOp, ErrorException, PhpMixed, RuntimeException, UnexpectedValueException,
    ZipArchive, bconcat, bin2hex, class_exists, file_exists, file_get_contents, filesize,
    hash_file, impl_php_class, implode, is_file, json_encode, legacy_lossy, path_to_bytes,
    php_regex, preg_match, random_int, str_replace, strlen, substr, version_compare,
};
use shirabe_symfony_process::ExecutableFinder;
use std::sync::Mutex;

static UNZIP_COMMANDS: Mutex<Option<Vec<Vec<BString>>>> = Mutex::new(None);
static HAS_ZIP_ARCHIVE: Mutex<Option<bool>> = Mutex::new(None);
static IS_WINDOWS: Mutex<Option<bool>> = Mutex::new(None);

#[derive(Debug)]
pub struct ZipDownloader {
    inner: FileDownloader,
    cleanup_executed: std::cell::RefCell<IndexMap<bstr::BString, bool>>,
    zip_archive_object: std::cell::RefCell<Option<ZipArchive>>,
}

impl_php_class!(ZipDownloader, r"Composer\Downloader\ZipDownloader");

impl ZipDownloader {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn crate::io::IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<crate::config::Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
        event_dispatcher: Option<
            std::rc::Rc<std::cell::RefCell<crate::event_dispatcher::EventDispatcher>>,
        >,
        cache: Option<std::rc::Rc<std::cell::RefCell<crate::cache::Cache>>>,
        filesystem: std::rc::Rc<std::cell::RefCell<crate::util::Filesystem>>,
        process: std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    ) -> Self {
        Self {
            inner: FileDownloader::new(
                io,
                config,
                http_downloader,
                event_dispatcher,
                cache,
                Some(filesystem),
                Some(process),
            ),
            cleanup_executed: std::cell::RefCell::new(IndexMap::new()),
            zip_archive_object: std::cell::RefCell::new(None),
        }
    }

    async fn extract_with_system_unzip(
        &self,
        package: PackageInterfaceHandle,
        file: &std::path::Path,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        static WARNED_7ZIP_LINUX: Mutex<bool> = Mutex::new(false);

        let is_last_chance = !HAS_ZIP_ARCHIVE.lock().unwrap().unwrap_or(false);

        let unzip_commands_empty = UNZIP_COMMANDS
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|v| v.is_empty());
        if unzip_commands_empty {
            return self.extract_with_zip_archive(package, file, path).await;
        }

        let command_spec = UNZIP_COMMANDS.lock().unwrap().as_ref().unwrap()[0].clone();
        let executable = command_spec[0].clone();
        let map: IndexMap<&str, BString> = [
            // normalize separators to backslashes to avoid problems with 7-zip on windows
            // see https://github.com/composer/composer/issues/10058
            (
                "%file%",
                str_replace("/", std::path::MAIN_SEPARATOR_STR, path_to_bytes(file)),
            ),
            (
                "%path%",
                str_replace("/", std::path::MAIN_SEPARATOR_STR, path_to_bytes(path)),
            ),
        ]
        .into_iter()
        .collect();
        let command: Vec<BString> = command_spec[1..]
            .iter()
            .map(|value| {
                let mut v = value.clone();
                for (from, to) in &map {
                    v = str_replace(from, to, &v);
                }
                v
            })
            .collect();

        if !*WARNED_7ZIP_LINUX.lock().unwrap()
            && !Platform::is_windows()
            && ["7z", "7zz", "7za"].iter().any(|e| executable == *e)
        {
            *WARNED_7ZIP_LINUX.lock().unwrap() = true;
            let mut output = String::new();
            if self
                .inner
                .process
                .borrow_mut()
                .execute(
                    &[command_spec[1].clone()],
                    &mut output,
                    None::<&str>.map(std::path::Path::new),
                )
                .unwrap_or(1)
                == 0
                && let Some(m) =
                    preg_match(php_regex!(r"{^\s*7-Zip(?:\s\[64\])?\s([0-9.]+)}"), &output)
            {
                let m1 = legacy_lossy(m.get(1).unwrap_or_default()).into_owned();
                if version_compare(&m1, "21.01", CmpOp::Lt) {
                    self.inner.io.borrow().write_error(&format!(
                        "    <warning>Unzipping using {} {} may result in incorrect file permissions. Install {} 21.01+ or unzip to ensure you get correct permissions.</warning>",
                        executable.display_lossy(),
                        m1,
                        executable.display_lossy(),
                    ));
                }
            }
        }

        let process_future = self
            .inner
            .process
            .borrow_mut()
            .execute_async(&command, None);
        let process_result = process_future.await;
        match process_result {
            Ok(mut process) => {
                if !process.is_successful() {
                    if self
                        .cleanup_executed
                        .borrow()
                        .contains_key(package.get_name().as_bstr())
                    {
                        return Err(RuntimeException::new(format!(
                            "Failed to extract {} as the installation was aborted by another package operation.",
                            package.get_name().display_lossy()
                        ))
                        .into());
                    }

                    let output = process.get_error_output()?;
                    let output =
                        str_replace(bconcat!(", ", file, ".zip or ", file, ".ZIP"), "", &output);

                    return self
                        .try_fallback(
                            RuntimeException::new(format!(
                                "Failed to extract {}: ({}) {}\n\n{}",
                                package.get_name().display_lossy(),
                                process
                                    .get_exit_code()
                                    .map(|c| c.to_string())
                                    .unwrap_or_default(),
                                implode(" ", &command).display_lossy(),
                                output.display_lossy()
                            ))
                            .into(),
                            is_last_chance,
                            file,
                            path,
                            package,
                            &executable,
                        )
                        .await;
                }

                Ok(None)
            }
            Err(e) => {
                self.try_fallback(e, is_last_chance, file, path, package, &executable)
                    .await
            }
        }
    }

    async fn try_fallback(
        &self,
        process_error: anyhow::Error,
        is_last_chance: bool,
        file: &std::path::Path,
        path: &std::path::Path,
        package: PackageInterfaceHandle,
        executable: &BString,
    ) -> anyhow::Result<Option<PhpMixed>> {
        if is_last_chance {
            return Err(process_error);
        }

        if process_error.to_string().contains("zip bomb") {
            return Err(process_error);
        }

        if !is_file(file) {
            self.inner
                .io
                .borrow()
                .write_error(&format!("    <warning>{}</warning>", process_error));
            self.inner.io.borrow().write_error("    <warning>This most likely is due to a custom installer plugin not handling the returned Promise from the downloader</warning>");
            self.inner.io.borrow().write_error("    <warning>See https://github.com/composer/installers/commit/5006d0c28730ade233a8f42ec31ac68fb1c5c9bb for an example fix</warning>");
        } else {
            self.inner
                .io
                .borrow()
                .write_error(&format!("    <warning>{}</warning>", process_error));
            self.inner.io.borrow().write_error("    The archive may contain identical file names with different capitalization (which fails on case insensitive filesystems)");
            self.inner.io.borrow().write_error(&format!(
                "    Unzip with {} command failed, falling back to ZipArchive class",
                executable.display_lossy()
            ));

            // additional debug data to try to figure out GH actions issues https://github.com/composer/composer/issues/11148
            if Platform::get_env("GITHUB_ACTIONS").is_some()
                && Platform::get_env("COMPOSER_TESTS_ARE_RUNNING").is_none()
            {
                self.inner.io.borrow().write_error("    <warning>Additional debug info, please report to https://github.com/composer/composer/issues/11148 if you see this:</warning>");
                self.inner.io.borrow().write_error(&format!(
                    "File size: {}",
                    filesize(file).map(|s| s.to_string()).unwrap_or_default()
                ));
                self.inner.io.borrow().write_error(&format!(
                    "File SHA1: {}",
                    hash_file("sha1", file).unwrap_or_default().display_lossy()
                ));
                self.inner.io.borrow().write_error(&format!(
                    "First 100 bytes (hex): {}",
                    bin2hex(substr(
                        file_get_contents(file).unwrap_or_default(),
                        0,
                        Some(100)
                    ))
                    .display_lossy()
                ));
                self.inner.io.borrow().write_error(&format!(
                    "Last 100 bytes (hex): {}",
                    bin2hex(substr(
                        file_get_contents(file).unwrap_or_default(),
                        -100,
                        None
                    ))
                    .display_lossy()
                ));
                if strlen(package.get_dist_url().unwrap_or_default()) > 0 {
                    self.inner.io.borrow().write_error(&format!(
                        "Origin URL: {}",
                        self.inner.process_url(
                            package.clone(),
                            &legacy_lossy(&package.get_dist_url().unwrap_or_default())
                        )?
                    ));
                    let headers = {
                        let response_headers = crate::downloader::file_downloader::RESPONSE_HEADERS
                            .lock()
                            .unwrap();
                        match response_headers.get(package.get_name().as_bstr()) {
                            Some(list) => PhpMixed::List(
                                list.iter().map(|s| PhpMixed::string(s.clone())).collect(),
                            ),
                            None => PhpMixed::List(vec![]),
                        }
                    };
                    self.inner.io.borrow().write_error(&format!(
                        "Response Headers: {}",
                        json_encode(&headers).unwrap_or_default()
                    ));
                }
            }
        }

        self.extract_with_zip_archive(package, file, path).await
    }

    async fn extract_with_zip_archive(
        &self,
        package: PackageInterfaceHandle,
        file: &std::path::Path,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let mut zip_archive = self
            .zip_archive_object
            .borrow_mut()
            .take()
            .unwrap_or_default();

        let result: anyhow::Result<Option<PhpMixed>> = (|| {
            let retval = if !file_exists(file) || filesize(file).is_none_or(|s| s == 0) {
                Err(-1i64)
            } else {
                zip_archive.open(file, 0)
            };

            match retval {
                Ok(_) => {
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
                                random_int(0..total_files)
                            };
                            if let Some(stat) = zip_archive.stat_index(stat_index) {
                                total_size += stat.size;
                                if !inspect_all && stat.size > stat.comp_size * 200 {
                                    total_size = 0;
                                    inspect_all = true;
                                    i = -1;
                                    files_to_inspect = total_files;
                                }
                            }
                            i += 1;
                        }
                        if let Some(archive_sz) = archive_size
                            && total_size > archive_sz * 100
                            && total_size > 50 * 1024 * 1024
                        {
                            return Err(RuntimeException::new(format!(
                                "Invalid zip file for \"{}\" with compression ratio >99% (possible zip bomb)",
                                package.get_name().display_lossy(),
                            )).into());
                        }
                    }

                    let extract_result = zip_archive.extract_to(path)?;

                    if extract_result {
                        zip_archive.close();
                        return Ok(None);
                    }

                    Err(RuntimeException::new(format!(
                        "There was an error extracting the ZIP file for \"{}\", it is either corrupted or using an invalid format.",
                        package.get_name().display_lossy(),
                    )).into())
                }
                Err(code) => Err(UnexpectedValueException::with_code(
                    self.get_error_message(code, file).trim_end().to_string(),
                    code,
                )
                .into()),
            }
        })();

        result.map_err(|e| {
            if let Some(err) = e.catch::<ErrorException>() {
                let message = format!(
                    "The archive for \"{}\" may contain identical file names with different capitalization (which fails on case insensitive filesystems): {}",
                    package.get_name().display_lossy(),
                    err.get_message(),
                );
                RuntimeException::with_code_and_previous(message, 0, Some(std::sync::Arc::new(e)))
                    .into()
            } else {
                e
            }
        })
    }

    pub fn get_error_message(&self, retval: i64, file: &std::path::Path) -> String {
        match retval {
            ZipArchive::ER_EXISTS => format!("File '{}' already exists.", file.display()),
            ZipArchive::ER_INCONS => format!("Zip archive '{}' is inconsistent.", file.display()),
            ZipArchive::ER_INVAL => format!("Invalid argument ({})", file.display()),
            ZipArchive::ER_MEMORY => format!("Malloc failure ({})", file.display()),
            ZipArchive::ER_NOENT => format!("No such zip file: '{}'", file.display()),
            ZipArchive::ER_NOZIP => format!("'{}' is not a zip archive.", file.display()),
            ZipArchive::ER_OPEN => format!("Can't open zip file: {}", file.display()),
            ZipArchive::ER_READ => format!("Zip read error ({})", file.display()),
            ZipArchive::ER_SEEK => format!("Zip seek error ({})", file.display()),
            -1 => format!(
                "'{}' is a corrupted zip archive (0 bytes), try again.",
                file.display()
            ),
            _ => format!(
                "'{}' is not a valid zip archive, got error code: {}",
                file.display(),
                retval
            ),
        }
    }

    /// For testing only. Mirrors the test's `setPrivateProperty('hasZipArchive', ...)` reflection on
    /// the `ZipDownloader::$hasZipArchive` static.
    pub fn __set_has_zip_archive(value: Option<bool>) {
        *HAS_ZIP_ARCHIVE.lock().unwrap() = value;
    }

    /// For testing only. Mirrors the test's `setPrivateProperty('isWindows', ...)` reflection.
    pub fn __set_is_windows(value: Option<bool>) {
        *IS_WINDOWS.lock().unwrap() = value;
    }

    /// For testing only. Mirrors the test's `setPrivateProperty('unzipCommands', ...)` reflection.
    pub fn __set_unzip_commands(value: Option<Vec<Vec<BString>>>) {
        *UNZIP_COMMANDS.lock().unwrap() = value;
    }

    /// For testing only. Mirrors the test's `setPrivateProperty('zipArchiveObject', $zipArchive, $obj)`
    /// reflection on the instance's `$zipArchiveObject` property.
    pub fn __set_zip_archive_object(&self, value: Option<ZipArchive>) {
        *self.zip_archive_object.borrow_mut() = value;
    }
}

impl ArchiveDownloader for ZipDownloader {
    fn inner(&self) -> &FileDownloader {
        &self.inner
    }

    fn cleanup_executed(&self) -> &std::cell::RefCell<IndexMap<bstr::BString, bool>> {
        &self.cleanup_executed
    }

    async fn extract(
        &self,
        package: PackageInterfaceHandle,
        file: &std::path::Path,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.extract_with_system_unzip(package, file, path).await
    }
}

#[async_trait::async_trait(?Send)]
impl ChangeReportInterface for ZipDownloader {
    async fn get_local_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<bstr::BString>> {
        self.inner.base_get_local_changes(self, package, path).await
    }
}

#[async_trait::async_trait(?Send)]
impl crate::downloader::DownloaderInterface for ZipDownloader {
    fn get_installation_source(&self) -> String {
        self.inner.get_installation_source()
    }

    fn as_change_report_interface(&self) -> Option<&dyn crate::downloader::ChangeReportInterface> {
        Some(self)
    }

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        {
            let mut unzip_commands = UNZIP_COMMANDS.lock().unwrap();
            if unzip_commands.is_none() {
                *unzip_commands = Some(vec![]);
                let finder = ExecutableFinder::new();
                let commands = unzip_commands.as_mut().unwrap();
                if Platform::is_windows()
                    && let Some(cmd) =
                        finder.find("7z", None, &[r"C:\Program Files\7-Zip".to_string()])
                {
                    commands.push(vec![
                        "7z".into(),
                        BString::from(cmd),
                        "x".into(),
                        "-bb0".into(),
                        "-y".into(),
                        "%file%".into(),
                        "-o%path%".into(),
                    ]);
                }
                if let Some(cmd) = finder.find("unzip", None, &[]) {
                    commands.push(vec![
                        "unzip".into(),
                        BString::from(cmd),
                        "-qq".into(),
                        "%file%".into(),
                        "-d".into(),
                        "%path%".into(),
                    ]);
                }
                if !Platform::is_windows() {
                    if let Some(cmd) = finder.find("7z", None, &[]) {
                        // 7z linux/macOS support is only used if unzip is not present
                        commands.push(vec![
                            "7z".into(),
                            BString::from(cmd),
                            "x".into(),
                            "-bb0".into(),
                            "-y".into(),
                            "%file%".into(),
                            "-o%path%".into(),
                        ]);
                    } else if let Some(cmd) = finder.find("7zz", None, &[]) {
                        // 7zz linux/macOS support is only used if unzip is not present
                        commands.push(vec![
                            "7zz".into(),
                            BString::from(cmd),
                            "x".into(),
                            "-bb0".into(),
                            "-y".into(),
                            "%file%".into(),
                            "-o%path%".into(),
                        ]);
                    } else if let Some(cmd) = finder.find("7za", None, &[]) {
                        // 7za linux/macOS support is only used if unzip is not present
                        commands.push(vec![
                            "7za".into(),
                            BString::from(cmd),
                            "x".into(),
                            "-bb0".into(),
                            "-y".into(),
                            "%file%".into(),
                            "-o%path%".into(),
                        ]);
                    }
                }
            }
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
            .is_none_or(|v| v.is_empty());

        if !has_zip_archive && unzip_commands_empty {
            let ini_message = IniHelper::get_message();
            let error = format!(
                "The zip extension and unzip/7z commands are both missing, skipping.\n{}",
                ini_message
            );
            return Err(RuntimeException::new(error).into());
        }

        {
            let mut is_windows_guard = IS_WINDOWS.lock().unwrap();
            if is_windows_guard.is_none() {
                *is_windows_guard = Some(Platform::is_windows());

                if !is_windows_guard.unwrap() && unzip_commands_empty {
                    self.inner.io.borrow().write_error("<warning>As there is no 'unzip' nor '7z' command installed zip files are being unpacked using the PHP zip extension.</warning>");
                    self.inner.io.borrow().write_error("<warning>This may cause invalid reports of corrupted archives. Besides, any UNIX permissions (e.g. executable) defined in the archives will be lost.</warning>");
                    self.inner.io.borrow().write_error("<warning>Installing 'unzip' or '7z' (21.01+) may remediate them.</warning>");
                }
            }
        }

        self.inner
            .download(package, path, prev_package, output)
            .await
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::install(self, package, path, output).await
    }

    async fn update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let appendix = self.get_install_operation_appendix(target.clone(), path);
        self.inner
            .base_update(self, appendix, initial, target, path)
            .await
    }

    async fn remove(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.inner.remove(package, path, output).await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as ArchiveDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
