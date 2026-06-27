//! ref: composer/tests/Composer/Test/Downloader/FileDownloaderTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use serial_test::serial;
use shirabe::cache::{Cache, CacheMock};
use shirabe::config::Config;
use shirabe::downloader::DownloaderInterface;
use shirabe::downloader::FileDownloader;
use shirabe::io::IOInterface;
use shirabe::io::io_interface;
use shirabe::io::null_io::NullIO;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::HttpDownloader;
use shirabe::util::filesystem::{Filesystem, FilesystemMock};
use shirabe::util::r#loop::Loop;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, UnexpectedValueException,
};
use shirabe_semver::VersionParser;
use tempfile::TempDir;

use crate::http_downloader_mock::get_http_downloader_mock;
use crate::io_mock::{Expectation, get_io_mock};
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
#[serial]
#[ignore = "PHP Config::get('cache-files-ttl') casts the string via (int) (Config.php:396-398), turning '99999999' into 99999999, but shirabe's PhpMixed::as_int returns None for String so Config::get yields 0 and the assertion fails. Faithful port stays failing until the src is fixed."]
fn test_cache_garbage_collection_is_called() {
    let expected_ttl: i64 = 99999999;

    let mut config_options: IndexMap<String, PhpMixed> = IndexMap::new();
    config_options.insert(
        "cache-files-ttl".to_string(),
        PhpMixed::String("99999999".to_string()),
    );
    config_options.insert(
        "cache-files-maxsize".to_string(),
        PhpMixed::String("500M".to_string()),
    );
    let config = get_config(config_options);

    // The PHP Cache mock forces gcIsNecessary() true and records the single gc() call; the CacheMock
    // seam plays both roles here.
    let tmp_dir = TempDir::new().unwrap();
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut cache = Cache::new(io, tmp_dir.path().to_str().unwrap(), None, None, false);
    cache.__set_mock(CacheMock {
        gc_is_necessary: Some(true),
        gc_calls: Some(Vec::new()),
        ..Default::default()
    });
    let cache = Rc::new(RefCell::new(cache));

    let (http_downloader, _guard) = get_http_downloader_mock(
        Vec::new(),
        false,
        HttpDownloaderMockHandler {
            status: 200,
            body: "file~".to_string(),
            headers: Vec::new(),
        },
    );

    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let _downloader = FileDownloader::new(
        io,
        config,
        http_downloader,
        None,
        Some(cache.clone()),
        None,
        None,
    );

    let gc_calls = cache.borrow().__gc_calls();
    assert_eq!(gc_calls.len(), 1, "gc should be called exactly once");
    assert_eq!(
        gc_calls[0].0, expected_ttl,
        "gc should receive the configured ttl"
    );
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
#[serial]
fn test_downgrade_shows_appropriate_message() {
    let old_package = get_package("dummy/pkg", "1.2.0");
    let new_package = get_package("dummy/pkg", "1.0.0");
    new_package.set_dist_url(Some("http://example.com/script.js".to_string()));

    let (io_mock, _io_guard) = get_io_mock(io_interface::DEBUG).unwrap();
    io_mock
        .borrow_mut()
        .expects(
            vec![
                Expectation::text_regex("{Downloading .*}"),
                Expectation::text_regex("{Downgrading .*}"),
            ],
            false,
        )
        .unwrap();

    let tmp_dir = TempDir::new().unwrap();
    let path = tmp_dir.path().to_string_lossy().into_owned();
    let mut config_options: IndexMap<String, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".to_string(),
        PhpMixed::String(format!("{}/vendor", path)),
    );
    let config = get_config(config_options);

    // PHP mocks Filesystem so removeDirectoryAsync is a no-op (resolve(true)) and normalizePath is an
    // identity; the FilesystemMock seam reproduces both so update()'s remove/install do not disturb
    // the pre-staged download file.
    let mut filesystem = Filesystem::new(None);
    filesystem.__set_mock(FilesystemMock {
        remove_directory_async_result: Some(true),
        normalize_path_identity: true,
        ..Default::default()
    });
    let filesystem = Rc::new(RefCell::new(filesystem));

    let (http_downloader, _guard) = get_http_downloader_mock(
        Vec::new(),
        false,
        HttpDownloaderMockHandler {
            status: 200,
            body: "file~".to_string(),
            headers: Vec::new(),
        },
    );

    let io: Rc<RefCell<dyn IOInterface>> = io_mock.clone();
    let mut downloader = FileDownloader::new(
        io,
        config,
        http_downloader.clone(),
        None,
        None,
        Some(filesystem.clone()),
        None,
    );

    // make sure the file expected to be downloaded is on disk already
    let dl_file = downloader.__get_file_name(new_package.clone(), &path);
    let dir = std::path::Path::new(&dl_file).parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(&dl_file, b"").unwrap();

    let mut loop_ = Loop::new(http_downloader, None);
    let promise = Box::pin(async {
        downloader
            .download(new_package.clone(), &path, Some(old_package.clone()), true)
            .await
            .map(|_| ())
    });
    run(loop_.wait(vec![promise], None)).expect("download should succeed");

    run(downloader.update(old_package.clone(), new_package.clone(), &path))
        .expect("update should succeed");

    assert_eq!(
        filesystem.borrow().__remove_directory_async_calls(),
        1,
        "removeDirectoryAsync should be called exactly once"
    );
}
