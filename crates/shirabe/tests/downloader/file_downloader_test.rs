//! ref: composer/tests/Composer/Test/Downloader/FileDownloaderTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use serial_test::serial;
use shirabe::config::Config;
use shirabe::downloader::DownloaderInterface;
use shirabe::downloader::FileDownloader;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::HttpDownloader;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::r#loop::Loop;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, UnexpectedValueException,
};
use shirabe_semver::VersionParser;
use tempfile::TempDir;

use crate::http_downloader_mock::get_http_downloader_mock;
use shirabe::util::http_downloader::HttpDownloaderMockHandler;

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

/// ref: TestCase::getConfig
fn get_config(config_options: IndexMap<String, PhpMixed>) -> Rc<RefCell<Config>> {
    let mut config = Config::new(false, None);
    if !config_options.is_empty() {
        let mut merged: IndexMap<String, PhpMixed> = IndexMap::new();
        merged.insert("config".to_string(), PhpMixed::Array(config_options));
        config.merge(&merged, "test");
    }
    Rc::new(RefCell::new(config))
}

/// The PHP `getDownloader` builds a HttpDownloader mock whose `addCopy` resolves to a 200 Response
/// with body `'file~'`. Here that is mirrored by a non-strict HttpDownloaderMock with a default
/// handler returning a 200/`file~` response for any URL. The mock never writes the destination file,
/// so the verification step throws "could not be saved to" exactly as in PHP.
fn get_downloader(
    io: Option<Rc<RefCell<dyn IOInterface>>>,
    config: Option<Rc<RefCell<Config>>>,
) -> (
    FileDownloader,
    Rc<RefCell<HttpDownloader>>,
    crate::http_downloader_mock::HttpDownloaderMockGuard,
) {
    let io = io.unwrap_or_else(|| Rc::new(RefCell::new(NullIO::new())));
    let config = config.unwrap_or_else(|| Rc::new(RefCell::new(Config::new(false, None))));

    let (http_downloader, guard) = get_http_downloader_mock(
        Vec::new(),
        false,
        HttpDownloaderMockHandler {
            status: 200,
            body: "file~".to_string(),
            headers: Vec::new(),
        },
    );

    let downloader =
        FileDownloader::new(io, config, http_downloader.clone(), None, None, None, None);

    (downloader, http_downloader, guard)
}

#[test]
#[serial]
fn test_download_for_package_without_dist_reference() {
    let package = get_package("dummy/pkg", "1.0.0");

    let (mut downloader, _http_downloader, _guard) = get_downloader(None, None);
    let result = run(downloader.download(package, "/path", None, true));

    let e = result.expect_err("expected InvalidArgumentException");
    assert!(
        e.downcast_ref::<InvalidArgumentException>().is_some(),
        "expected InvalidArgumentException, got: {e}"
    );
}

#[test]
#[serial]
fn test_download_to_existing_file() {
    let package = get_package("dummy/pkg", "1.0.0");
    package.set_dist_url(Some("url".to_string()));

    // createTempFile(getUniqueTmpDirectory()): a regular file used as the download target path.
    let tmp_dir = TempDir::new().unwrap();
    let path = tmp_dir.path().join("composer_test_file");
    std::fs::write(&path, b"").unwrap();
    let path = path.to_string_lossy().into_owned();

    let (mut downloader, _http_downloader, _guard) = get_downloader(None, None);

    let result = run(downloader.download(package, &path, None, true));

    let e = result.expect_err("download to an existing file was expected to throw");
    assert!(
        e.downcast_ref::<RuntimeException>().is_some(),
        "expected RuntimeException, got: {e}"
    );
    assert!(
        e.to_string().contains("exists and is not a directory"),
        "unexpected message: {e}"
    );
}

#[test]
#[serial]
fn test_get_file_name() {
    let package = get_package("dummy/pkg", "1.0.0");
    package.set_dist_url(Some("http://example.com/script.js".to_string()));

    let mut config_options: IndexMap<String, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".to_string(),
        PhpMixed::String("/vendor".to_string()),
    );
    let config = get_config(config_options);

    let (downloader, _http_downloader, _guard) = get_downloader(None, Some(config));

    let file_name = downloader.__get_file_name(package, "/path");
    let re = regex::Regex::new(r"/vendor/composer/tmp-[a-z0-9]+\.js").unwrap();
    assert!(re.is_match(&file_name), "unexpected file name: {file_name}");
}

#[test]
#[serial]
fn test_download_but_file_is_unsaved() {
    let package = get_package("dummy/pkg", "1.0.0");
    package.set_dist_url(Some("http://example.com/script.js".to_string()));

    let tmp_dir = TempDir::new().unwrap();
    let path = tmp_dir.path().to_string_lossy().into_owned();

    // The PHP IOMock write callback unlinks the half-written script.js; the Rust mock never writes
    // the destination file in the first place, so a NullIO suffices to reproduce the unsaved-file path.
    let mut config_options: IndexMap<String, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".to_string(),
        PhpMixed::String(format!("{}/vendor", path)),
    );
    let config = get_config(config_options);

    let (mut downloader, http_downloader, _guard) = get_downloader(None, Some(config));

    let mut loop_ = Loop::new(http_downloader, None);
    let promise = Box::pin(async {
        downloader
            .download(package.clone(), &path, None, true)
            .await
            .map(|_| ())
    });
    let result = run(loop_.wait(vec![promise], None));

    let e = result.expect_err("download was expected to throw");
    assert!(
        e.downcast_ref::<UnexpectedValueException>().is_some(),
        "expected UnexpectedValueException, got: {e}"
    );
    assert!(
        e.to_string().contains("could not be saved to"),
        "unexpected message: {e}"
    );
}

#[test]
#[ignore = "requires PHPUnit mocks of Cache::copyTo/copyFrom asserting on $cacheKey plus PreFileDownloadEvent::setProcessedUrl dispatch, which is TODO(plugin) in FileDownloader::download"]
fn test_download_with_custom_processed_url() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mocks of Cache::copyTo/copyFrom asserting on $cacheKey plus PreFileDownloadEvent::setCustomCacheKey dispatch, which is TODO(plugin) in FileDownloader::download"]
fn test_download_with_custom_cache_key() {
    todo!()
}

#[test]
#[ignore = "requires a Cache mock with gcIsNecessary/gc expectation tracking; Cache is a concrete struct with no test hook for asserting gc() was called once"]
fn test_cache_garbage_collection_is_called() {
    todo!()
}

#[test]
#[serial]
fn test_download_file_with_invalid_checksum() {
    let package = get_package("dummy/pkg", "1.0.0");
    package.set_dist_url(Some("http://example.com/script.js".to_string()));
    package.__set_dist_sha1_checksum(Some("invalid".to_string()));

    let tmp_dir = TempDir::new().unwrap();
    let path = tmp_dir.path().to_string_lossy().into_owned();
    let mut config_options: IndexMap<String, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".to_string(),
        PhpMixed::String(format!("{}/vendor", path)),
    );
    let config = get_config(config_options);

    // The PHP test injects a Filesystem mock so cleanup is a no-op; a real Filesystem behaves the
    // same here since nothing under the temp dir needs preserving.
    let (mut downloader, http_downloader, _guard) = get_downloader(None, Some(config));

    // make sure the file expected to be downloaded is on disk already
    let dl_file = downloader.__get_file_name(package.clone(), &path);
    let dir = std::path::Path::new(&dl_file).parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(&dl_file, b"").unwrap();

    let mut loop_ = Loop::new(http_downloader, None);
    let promise = Box::pin(async {
        downloader
            .download(package.clone(), &path, None, true)
            .await
            .map(|_| ())
    });
    let result = run(loop_.wait(vec![promise], None));

    let e = result.expect_err("download was expected to throw");
    assert!(
        e.downcast_ref::<UnexpectedValueException>().is_some(),
        "expected UnexpectedValueException, got: {e}"
    );
    assert!(
        e.to_string().contains("checksum verification"),
        "unexpected message: {e}"
    );
}

#[test]
#[ignore = "requires a Filesystem mock of removeDirectoryAsync/normalizePath plus IOMock output expectations; Filesystem is a concrete struct with no async-removal test hook"]
fn test_downgrade_shows_appropriate_message() {
    let _ = Filesystem::new(None);
    todo!()
}
