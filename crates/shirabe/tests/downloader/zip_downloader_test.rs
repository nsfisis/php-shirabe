//! ref: composer/tests/Composer/Test/Downloader/ZipDownloaderTest.php

use crate::io_stub::IOStub;
use serial_test::serial;
use shirabe::config::Config;
use shirabe::downloader::ArchiveDownloader;
use shirabe::downloader::zip_downloader::ZipDownloader;
use shirabe::io::IOInterface;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::HttpDownloader;
use shirabe::util::ProcessExecutor;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::{ZipArchive, ZipArchiveMock};
use shirabe_semver::VersionParser;
use std::cell::RefCell;
use std::rc::Rc;
use tempfile::TempDir;

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

struct SetUp {
    test_dir: TempDir,
    io: Rc<RefCell<dyn IOInterface>>,
    config: Rc<RefCell<Config>>,
    http_downloader: Rc<RefCell<HttpDownloader>>,
    package: PackageInterfaceHandle,
    filename: std::path::PathBuf,
}

/// ref: ZipDownloaderTest::setUp.
///
/// The PHP test mocks IOInterface/Config/HttpDownloader/PackageInterface via PHPUnit. Here the
/// IO/Config use the existing stubs, the HttpDownloader is built as a mock (no network is touched on
/// the `extract` path exercised by the ported tests), and the package is a real CompletePackage whose
/// `getName()` is `test/pkg`, matching the PHP mock's `->method('getName')->willReturn('test/pkg')`.
fn set_up() -> SetUp {
    let test_dir = TempDir::new().unwrap();

    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));
    let config = Rc::new(RefCell::new(Config::new(false, None)));
    let dl_config = Rc::new(RefCell::new(Config::new(false, None)));
    let http_downloader = Rc::new(RefCell::new(HttpDownloader::__new_mock(
        io.clone(),
        dl_config,
    )));

    let norm_version = VersionParser.normalize("1.0.0", None).unwrap();
    let package: PackageInterfaceHandle =
        CompletePackageHandle::new("test/pkg".to_string(), norm_version, "1.0.0".to_string())
            .into();

    let filename = test_dir.path().join("composer-test.zip");
    std::fs::write(&filename, "zip").unwrap();

    SetUp {
        test_dir,
        io,
        config,
        http_downloader,
        package,
        filename,
    }
}

fn tear_down(test_dir: &std::path::Path) {
    let mut fs = Filesystem::new(None);
    fs.remove_directory(test_dir).unwrap();
    // setPrivateProperty('hasZipArchive', null)
    ZipDownloader::__set_has_zip_archive(None);
}

struct TearDown {
    test_dir: std::path::PathBuf,
}

impl TearDown {
    fn new(test_dir: std::path::PathBuf) -> Self {
        TearDown { test_dir }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.test_dir);
    }
}

fn make_downloader(set_up: &SetUp) -> ZipDownloader {
    let filesystem = Rc::new(RefCell::new(Filesystem::new(None)));
    let process = Rc::new(RefCell::new(ProcessExecutor::new(Some(set_up.io.clone()))));
    ZipDownloader::new(
        set_up.io.clone(),
        set_up.config.clone(),
        set_up.http_downloader.clone(),
        None,
        None,
        filesystem,
        process,
    )
}

// The system-unzip / non-windows-fallback paths route through ProcessExecutor::execute_async, whose
// mock branch is an unimplemented todo!() (no Process mock seam exists in the external-packages
// crate). The PHP tests below mock Process/ProcessExecutor::executeAsync, which is not reproducible
// here, so they remain ignored.
//
// testErrorMessages drives a real HttpDownloader + Loop (curl_multi_init todo!()), also out of reach.

#[ignore = "drives a real HttpDownloader + Loop (download/install), which reaches curl_multi_init todo!()"]
#[test]
fn test_error_messages() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());
    let _ = (&set_up.io, &set_up.config, &set_up.http_downloader);
    todo!()
}

#[test]
#[serial]
fn test_zip_archive_only_failed() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());

    ZipDownloader::__set_has_zip_archive(Some(true));
    let mut downloader = make_downloader(&set_up);
    let zip_archive = ZipArchive::__mock(ZipArchiveMock {
        open: Ok(()),
        count: 0,
        extract_to: Ok(false),
    });
    downloader.__set_zip_archive_object(Some(zip_archive));

    let filename = set_up.filename.to_string_lossy().into_owned();
    let result = run(downloader.extract(set_up.package.clone(), &filename, "vendor/dir"));

    let e = result.expect_err("expected RuntimeException");
    assert!(
        e.to_string()
            .contains("There was an error extracting the ZIP file"),
        "got: {e}"
    );
}

#[test]
#[serial]
fn test_zip_archive_extract_only_failed() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());

    ZipDownloader::__set_has_zip_archive(Some(true));
    let mut downloader = make_downloader(&set_up);
    let zip_archive = ZipArchive::__mock(ZipArchiveMock {
        open: Ok(()),
        count: 0,
        extract_to: Err("Not a directory".to_string()),
    });
    downloader.__set_zip_archive_object(Some(zip_archive));

    let filename = set_up.filename.to_string_lossy().into_owned();
    let result = run(downloader.extract(set_up.package.clone(), &filename, "vendor/dir"));

    let e = result.expect_err("expected RuntimeException");
    assert!(
        e.to_string().contains(
            "The archive for \"test/pkg\" may contain identical file names with different \
             capitalization (which fails on case insensitive filesystems): Not a directory"
        ),
        "got: {e}"
    );
}

#[test]
#[serial]
fn test_zip_archive_only_good() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());

    ZipDownloader::__set_has_zip_archive(Some(true));
    let mut downloader = make_downloader(&set_up);
    let zip_archive = ZipArchive::__mock(ZipArchiveMock {
        open: Ok(()),
        count: 0,
        extract_to: Ok(true),
    });
    downloader.__set_zip_archive_object(Some(zip_archive));

    let filename = set_up.filename.to_string_lossy().into_owned();
    let result = run(downloader.extract(set_up.package.clone(), &filename, "vendor/dir"));

    result.expect("extract should succeed");
}

#[ignore = "routes through ProcessExecutor::execute_async whose mock branch is todo!() (no Process mock seam in external-packages)"]
#[test]
fn test_system_unzip_only_failed() {
    let _ = set_up();
    todo!()
}

#[ignore = "routes through ProcessExecutor::execute_async whose mock branch is todo!() (no Process mock seam in external-packages)"]
#[test]
fn test_system_unzip_only_good() {
    let _ = set_up();
    todo!()
}

#[ignore = "routes through ProcessExecutor::execute_async whose mock branch is todo!() (no Process mock seam in external-packages)"]
#[test]
fn test_non_windows_fallback_good() {
    let _ = set_up();
    todo!()
}

#[ignore = "routes through ProcessExecutor::execute_async whose mock branch is todo!() (no Process mock seam in external-packages)"]
#[test]
fn test_non_windows_fallback_failed() {
    let _ = set_up();
    todo!()
}
