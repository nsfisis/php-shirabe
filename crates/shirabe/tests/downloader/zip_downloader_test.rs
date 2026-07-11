//! ref: composer/tests/Composer/Test/Downloader/ZipDownloaderTest.php

use crate::io_stub::IOStub;
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::config::Config;
use shirabe::downloader::ArchiveDownloader;
use shirabe::downloader::DownloaderInterface;
use shirabe::downloader::zip_downloader::ZipDownloader;
use shirabe::io::IOInterface;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::HttpDownloader;
use shirabe::util::ProcessExecutor;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::r#loop::Loop;
use shirabe_php_shim::{PhpMixed, ZipArchive, ZipArchiveMock};
use shirabe_semver::VersionParser;
use tempfile::TempDir;

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

struct SetUp {
    test_dir: TempDir,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
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

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));
    let config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let dl_config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::__new_mock(
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
    let filesystem = std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None)));
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        set_up.io.clone(),
    ))));
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
// testErrorMessages drives a real HttpDownloader + Loop, but RemoteFilesystem::get_remote_contents
// is a phase-c stub returning None, so the file:// dist download fails before the ZipArchive path.

#[ignore = "drives a real HttpDownloader whose RemoteFilesystem::get_remote_contents is a phase-c stub returning None, so the file:// dist download fails with a \"could not be downloaded\" TransportException before reaching the ZipArchive \"is not a zip archive\" path"]
#[test]
#[serial]
fn test_error_messages() {
    // class_exists('ZipArchive') is always true in the shim, so the zip-extension-missing skip never
    // applies here.
    let test_dir = TempDir::new().unwrap();
    let _tear_down = TearDown::new(test_dir.path().to_path_buf());

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()));

    // $this->config->method('get')->with('vendor-dir')->willReturn($this->testDir)
    let mut config = Config::new(false, None);
    let mut config_options: IndexMap<String, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".to_string(),
        PhpMixed::String(test_dir.path().to_string_lossy().into_owned()),
    );
    let mut merged: IndexMap<String, PhpMixed> = IndexMap::new();
    merged.insert("config".to_string(), PhpMixed::Array(config_options));
    config.merge(&merged, "test");
    let config = std::rc::Rc::new(std::cell::RefCell::new(config));

    // new HttpDownloader($this->io, $dlConfig): a real downloader (not the extract-path mock).
    let dl_config = std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None)));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::new(
        io.clone(),
        dl_config,
        IndexMap::new(),
        false,
    )));

    // $distUrl = 'file://'.__FILE__: an existing, non-zip file referenced by a file:// URL.
    let dist_url = format!("file://{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"));
    let norm_version = VersionParser.normalize("1.0.0", None).unwrap();
    let package: PackageInterfaceHandle =
        CompletePackageHandle::new("test/pkg".to_string(), norm_version, "1.0.0".to_string())
            .into();
    package.set_dist_url(Some(dist_url));

    let filesystem = std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None)));
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        io.clone(),
    ))));
    let mut downloader = ZipDownloader::new(
        io,
        config,
        http_downloader.clone(),
        None,
        None,
        filesystem,
        process,
    );

    let path = std::env::temp_dir()
        .join("composer-zip-test")
        .to_string_lossy()
        .into_owned();

    let result: anyhow::Result<()> = (|| {
        let mut loop_ = Loop::new(http_downloader.clone(), None);
        let promise = Box::pin(async {
            DownloaderInterface::download(&mut downloader, package.clone(), &path, None, true)
                .await
                .map(|_| ())
        });
        run(loop_.wait(vec![promise], None))?;
        run(DownloaderInterface::install(
            &mut downloader,
            package.clone(),
            &path,
            true,
        ))
        .map(|_| ())
    })();

    let e = result.expect_err("Download of invalid zip files should throw an exception");
    assert!(e.to_string().contains("is not a zip archive"), "got: {e}");
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
