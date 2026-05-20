//! ref: composer/src/Composer/Downloader/ZipDownloader.php

use crate::downloader::ArchiveDownloader;
use crate::downloader::DownloaderInterface;
use crate::downloader::FileDownloader;
use crate::package::PackageInterface;
use crate::util::IniHelper;
use crate::util::Platform;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::react::promise::PromiseInterface;
use shirabe_external_packages::symfony::component::process::ExecutableFinder;
use shirabe_external_packages::symfony::component::process::Process;
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
    inner: FileDownloader,
    cleanup_executed: IndexMap<String, bool>,
    // @phpstan-ignore property.onlyRead (helper property that is set via reflection for testing purposes)
    zip_archive_object: Option<ZipArchive>,
}

impl ZipDownloader {
    pub fn new(
        io: Box<dyn crate::io::IOInterface>,
        config: std::rc::Rc<std::cell::RefCell<crate::config::Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
        event_dispatcher: Option<
            std::rc::Rc<std::cell::RefCell<crate::event_dispatcher::EventDispatcher>>,
        >,
        cache: Option<crate::cache::Cache>,
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
            cleanup_executed: IndexMap::new(),
            zip_archive_object: None,
        }
    }

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
                    if let Some(cmd) =
                        finder.find("7z", None, &[r"C:\Program Files\7-Zip".to_string()])
                    {
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
                        self.inner.io.write_error("<warning>proc_open is disabled so 'unzip' and '7z' commands cannot be used, zip files are being unpacked using the PHP zip extension.</warning>");
                        self.inner.io.write_error("<warning>This may cause invalid reports of corrupted archives. Besides, any UNIX permissions (e.g. executable) defined in the archives will be lost.</warning>");
                        self.inner.io.write_error("<warning>Enabling proc_open and installing 'unzip' or '7z' (21.01+) may remediate them.</warning>");
                    } else {
                        self.inner.io.write_error("<warning>As there is no 'unzip' nor '7z' command installed zip files are being unpacked using the PHP zip extension.</warning>");
                        self.inner.io.write_error("<warning>This may cause invalid reports of corrupted archives. Besides, any UNIX permissions (e.g. executable) defined in the archives will be lost.</warning>");
                        self.inner.io.write_error("<warning>Installing 'unzip' or '7z' (21.01+) may remediate them.</warning>");
                    }
                }
            }
        }

        self.inner.download(package, path, prev_package, output)
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
                .process
                .borrow_mut()
                .execute(&[command_spec[1].as_str()], &mut output, None::<&str>)
                .unwrap_or(1)
                == 0
            {
                let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                if Preg::is_match_strict_groups3(
                    r"^\s*7-Zip(?:\s\[64\])?\s([0-9.]+)",
                    &output,
                    Some(&mut m),
                )
                .unwrap_or(false)
                {
                    let m1 = m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default();
                    if version_compare(&m1, "21.01", "<") {
                        self.inner.io.write_error(&format!(
                            "    <warning>Unzipping using {} {} may result in incorrect file permissions. Install {} 21.01+ or unzip to ensure you get correct permissions.</warning>",
                            executable, m1, executable,
                        ));
                    }
                }
            }
        }

        // TODO(phase-b): full try_fallback closure deferred — PHP captures `$io`, `$self`
        // and several locals by reference, conflicting with Rust's borrow checker because
        // `extract_with_zip_archive` later needs `&mut self`. Restructure once the
        // promise/closure plumbing supports that shape.
        let _ = (
            is_last_chance,
            file,
            path,
            executable,
            package,
            &command,
            &self.inner.io,
        );
        match self.inner.process.borrow_mut().execute_async(&command, ()) {
            Ok(_promise) => todo!("phase-b: chain promise.then with fallback closure"),
            Err(_e) => todo!("phase-b: pipe execute_async error into try_fallback"),
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

// TODO(phase-b): ZipDownloader::download is overridden with extra setup (UNZIP_COMMANDS init,
// etc.). The trait method here delegates straight to the inner FileDownloader; the bespoke
// override on the struct itself takes &mut self and is not yet routed through the trait.
impl crate::downloader::DownloaderInterface for ZipDownloader {
    fn get_installation_source(&self) -> String {
        self.inner.get_installation_source()
    }

    fn download(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.download(package, path, prev_package, output)
    }

    fn prepare(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.prepare(r#type, package, path, prev_package)
    }

    fn install(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.install(package, path, output)
    }

    fn update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.update(initial, target, path)
    }

    fn remove(
        &self,
        package: &dyn PackageInterface,
        path: &str,
        output: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.remove(package, path, output)
    }

    fn cleanup(
        &self,
        r#type: &str,
        package: &dyn PackageInterface,
        path: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.cleanup(r#type, package, path, prev_package)
    }
}
