//! ref: composer/tests/Composer/Test/Downloader/XzDownloaderTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::DownloaderInterface;
use shirabe::downloader::xz_downloader::XzDownloader;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::HttpDownloader;
use shirabe::util::Platform;
use shirabe::util::ProcessExecutor;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::r#loop::Loop;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::version_parser::VersionParser;
use tempfile::TempDir;

/// ref: TestCase::getPackage (default class CompletePackage)
fn get_package(name: &str, version: &str) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    CompletePackageHandle::new(name.to_string(), norm_version, version.to_string()).into()
}

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

fn set_up() -> TempDir {
    if Platform::is_windows() {
        // markTestSkipped('Skip test on Windows')
        todo!()
    }
    if std::mem::size_of::<usize>() == 4 {
        // markTestSkipped('Skip test on 32bit')
        todo!()
    }
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
fn get_config(config_options: IndexMap<String, PhpMixed>, use_environment: bool) -> Config {
    let mut config = Config::new(use_environment, None);
    let mut merged: IndexMap<String, PhpMixed> = IndexMap::new();
    merged.insert("config".to_string(), PhpMixed::Array(config_options));
    config.merge(&merged, "test");
    config
}

#[ignore]
#[test]
fn test_error_messages() {
    let test_dir = set_up();
    let _tear_down = TearDown::new(test_dir.path().to_path_buf());
    let test_dir = test_dir.path().to_path_buf();

    let package = get_package("dummy/pkg", "1.0.0");
    let dist_url = format!("file://{}", file!());
    package.set_dist_url(Some(dist_url));

    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut config_options: IndexMap<String, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".to_string(),
        PhpMixed::String(test_dir.to_string_lossy().into_owned()),
    );
    let config = Rc::new(RefCell::new(get_config(config_options, false)));
    let http_downloader = Rc::new(RefCell::new(HttpDownloader::new(
        io.clone(),
        config.clone(),
        IndexMap::new(),
        false,
    )));
    let filesystem = Rc::new(RefCell::new(Filesystem::new(None)));
    let process = Rc::new(RefCell::new(ProcessExecutor::new(Some(io.clone()))));
    let mut downloader = XzDownloader::new(
        io,
        config,
        http_downloader.clone(),
        None,
        None,
        filesystem,
        process,
    );

    let install_path = test_dir.join("install-path");
    let install_path = install_path.to_string_lossy().into_owned();

    let mut loop_ = Loop::new(http_downloader, None);
    let promise = Box::pin(async {
        downloader
            .download3(package.clone(), &install_path, None)
            .await
            .map(|_| ())
    });
    run(loop_.wait(vec![promise], None)).unwrap();
    let result = run(downloader.install2(package, &install_path));

    // Download of invalid tarball should throw an exception.
    let e = result.expect_err("Download of invalid tarball should throw an exception");
    let re =
        regex::Regex::new("(?i)(File format not recognized|Unrecognized archive format)").unwrap();
    assert!(re.is_match(&e.to_string()));
}
