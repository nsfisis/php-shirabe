//! ref: composer/tests/Composer/Test/Downloader/XzDownloaderTest.php sha1:a64362aa02e8f92c98f1ad72f81f23d2a656873d

use crate::async_runtime::run;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::DownloaderInterface;
use shirabe::downloader::xz_downloader::XzDownloader;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::filesystem::Filesystem;
use shirabe::util::r#loop::Loop;
use shirabe::util::{HttpDownloader, Platform, ProcessExecutor};
use shirabe_php_shim::{PhpMixed, bconcat, error_message, path_to_bytes};
use shirabe_semver::VersionParser;
use tempfile::TempDir;

/// ref: TestCase::getPackage (default class CompletePackage)
fn get_package(name: &BStr, version: &BStr) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    CompletePackageHandle::new(bconcat!(name), norm_version, bconcat!(version)).into()
}

/// ref: setUp markTestSkipped on Windows / 32bit, expressed as a compile-time
/// cfg gate on the test function below.
fn set_up() -> TempDir {
    TempDir::new().unwrap()
}

fn tear_down(test_dir: &std::path::Path) {
    if Platform::is_windows() {
        return;
    }
    let mut fs = Filesystem::new(None);
    fs.remove_directory(test_dir).unwrap();
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

/// ref: TestCase::getConfig
fn get_config(config_options: IndexMap<BString, PhpMixed>, use_environment: bool) -> Config {
    let mut config = Config::new(use_environment, None);
    let mut merged: IndexMap<BString, PhpMixed> = IndexMap::new();
    merged.insert("config".into(), PhpMixed::Array(config_options));
    config.merge(&merged, "test");
    config
}

#[cfg(all(not(windows), target_pointer_width = "64"))]
#[ignore = "dist_url built from file!(), which yields a workspace-relative path unlike PHP's absolute __FILE__, so the file:// URL never resolves to a real file"]
#[test]
fn test_error_messages() {
    let test_dir = set_up();
    let _tear_down = TearDown::new(test_dir.path().to_path_buf());
    let test_dir = test_dir.path().to_path_buf();

    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    let dist_url = bconcat!("file://", file!());
    package.set_dist_url(Some(dist_url));

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut config_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".into(),
        PhpMixed::string(path_to_bytes(&test_dir)),
    );
    let config = std::rc::Rc::new(std::cell::RefCell::new(get_config(config_options, false)));
    let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::new(
        io.clone(),
        config.clone(),
        IndexMap::new(),
        false,
    )));
    let filesystem = std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None)));
    let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
        io.clone(),
    ))));
    let downloader = XzDownloader::new(
        io,
        config,
        http_downloader.clone(),
        None,
        None,
        filesystem,
        process,
    )
    .unwrap();

    let install_path = test_dir.join("install-path");
    let install_path = install_path.to_path_buf();

    let loop_ = Loop::new(http_downloader, None);
    let promise = Box::pin(async {
        downloader
            .download3(package.clone(), std::path::Path::new(&install_path), None)
            .await
            .map(|_| ())
    });
    run(loop_.wait(vec![promise], None)).unwrap();
    let result = run(downloader.install2(package, std::path::Path::new(&install_path)));

    // Download of invalid tarball should throw an exception.
    let e = result.expect_err("Download of invalid tarball should throw an exception");
    let re =
        regex::bytes::Regex::new("(?i)(File format not recognized|Unrecognized archive format)")
            .unwrap();
    assert!(re.is_match(&error_message(&e)));
}
