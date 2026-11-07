//! ref: composer/tests/Composer/Test/Downloader/FileDownloaderTest.php sha1:09f792b7ead57d2b7a5fa9dc5d80e0335e34b7fb

use crate::async_runtime::run;
use crate::http_downloader_mock::{HttpDownloaderMockGuard, get_http_downloader_mock};
use crate::io_mock::{Expectation, get_io_mock};
use crate::test_case::get_unique_tmp_directory;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use serial_test::serial;
use shirabe::cache::{Cache, CacheMock};
use shirabe::config::Config;
use shirabe::downloader::{DownloaderInterface, FileDownloader};
use shirabe::io::{IOInterface, NullIO, io_interface};
use shirabe::package::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::r#loop::Loop;
use shirabe::util::{Filesystem, FilesystemMock, HttpDownloader, HttpDownloaderMockHandler};
use shirabe_php_shim::{
    Catch as _, InvalidArgumentException, PhpMixed, RuntimeException, UnexpectedValueException,
    bconcat, error_message, path_to_bytes,
};
use shirabe_semver::VersionParser;

/// ref: TestCase::getPackage (default class CompletePackage)
fn get_package(name: &BStr, version: &BStr) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize(version, None).unwrap();
    CompletePackageHandle::new(bconcat!(name), norm_version, bconcat!(version)).into()
}

/// ref: TestCase::getConfig
fn get_config(
    config_options: IndexMap<BString, PhpMixed>,
) -> std::rc::Rc<std::cell::RefCell<Config>> {
    let mut config = Config::new(false, None);
    if !config_options.is_empty() {
        let mut merged: IndexMap<BString, PhpMixed> = IndexMap::new();
        merged.insert("config".into(), PhpMixed::Array(config_options));
        config.merge(&merged, "test");
    }
    std::rc::Rc::new(std::cell::RefCell::new(config))
}

/// The PHP `getDownloader` builds a HttpDownloader mock whose `addCopy` resolves to a 200 Response
/// with body `'file~'`. Here that is mirrored by a non-strict HttpDownloaderMock with a default
/// handler returning a 200/`file~` response for any URL. The mock never writes the destination file,
/// so the verification step throws "could not be saved to" exactly as in PHP.
fn get_downloader(
    io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    config: Option<std::rc::Rc<std::cell::RefCell<Config>>>,
) -> (
    FileDownloader,
    std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    HttpDownloaderMockGuard,
) {
    let io = io.unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(NullIO::new())));
    let config = config
        .unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(Config::new(false, None))));

    let (http_downloader, guard) = get_http_downloader_mock(
        Vec::new(),
        false,
        HttpDownloaderMockHandler {
            status: 200,
            body: BString::from("file~"),
            headers: Vec::new(),
        },
    );

    let downloader =
        FileDownloader::new(io, config, http_downloader.clone(), None, None, None, None).unwrap();

    (downloader, http_downloader, guard)
}

#[test]
#[serial]
fn test_download_for_package_without_dist_reference() {
    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));

    let (downloader, _http_downloader, _guard) = get_downloader(None, None);
    let result =
        run(downloader.download_with_output(package, std::path::Path::new("/path"), None, true));

    let e = result.expect_err("expected InvalidArgumentException");
    assert!(
        e.is_instanceof::<InvalidArgumentException>(),
        "expected InvalidArgumentException, got: {e}"
    );
}

#[test]
#[serial]
fn test_download_to_existing_file() {
    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    package.set_dist_url(Some(BString::from("url")));

    // createTempFile(getUniqueTmpDirectory()): a regular file used as the download target path.
    let tmp_dir = get_unique_tmp_directory();
    let path = tmp_dir.path().join("composer_test_file");
    std::fs::write(&path, b"").unwrap();
    let path = path.to_path_buf();

    let (downloader, _http_downloader, _guard) = get_downloader(None, None);

    let result =
        run(downloader.download_with_output(package, std::path::Path::new(&path), None, true));

    let e = result.expect_err("download to an existing file was expected to throw");
    assert!(
        e.is_instanceof::<RuntimeException>(),
        "expected RuntimeException, got: {e}"
    );
    assert!(
        error_message(&e).contains_str("exists and is not a directory"),
        "unexpected message: {e}"
    );
}

#[test]
#[serial]
fn test_get_file_name() {
    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    package.set_dist_url(Some(BString::from("http://example.com/script.js")));

    let mut config_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".into(),
        PhpMixed::string(BString::from("/vendor")),
    );
    let config = get_config(config_options);

    let (downloader, _http_downloader, _guard) = get_downloader(None, Some(config));

    let file_name = downloader
        .__get_file_name(package, std::path::Path::new("/path"))
        .unwrap();
    let re = regex::bytes::Regex::new(r"/vendor/composer/tmp-[a-z0-9]+\.js").unwrap();
    assert!(
        re.is_match(path_to_bytes(&file_name)),
        "unexpected file name: {}",
        file_name.display()
    );
}

#[test]
#[serial]
fn test_download_but_file_is_unsaved() {
    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    package.set_dist_url(Some(BString::from("http://example.com/script.js")));

    let tmp_dir = get_unique_tmp_directory();
    let path = tmp_dir.path().to_path_buf();

    // The PHP IOMock write callback unlinks the half-written script.js; the Rust mock never writes
    // the destination file in the first place, so a NullIO suffices to reproduce the unsaved-file path.
    let mut config_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".into(),
        PhpMixed::string(path_to_bytes(&tmp_dir.path().join("vendor"))),
    );
    let config = get_config(config_options);

    let (downloader, http_downloader, _guard) = get_downloader(None, Some(config));

    let loop_ = Loop::new(http_downloader, None);
    let promise = Box::pin(async {
        downloader
            .download_with_output(package.clone(), std::path::Path::new(&path), None, true)
            .await
            .map(|_| ())
    });
    let result = run(loop_.wait(vec![promise], None));

    let e = result.expect_err("download was expected to throw");
    assert!(
        e.is_instanceof::<UnexpectedValueException>(),
        "expected UnexpectedValueException, got: {e}"
    );
    assert!(
        error_message(&e).contains_str("could not be saved to"),
        "unexpected message: {e}"
    );
}

#[test]
#[ignore = "the listener is a closure that mutates the event (setProcessedUrl), but Callable::Closure receives `&dyn EventInterface`, so it cannot; and CacheMock has no copy_to/copy_from hooks to assert the cache key on"]
fn test_download_with_custom_processed_url() {
    // TODO(mock): the PRE_FILE_DOWNLOAD listener is a closure calling
    // PreFileDownloadEvent::setProcessedUrl, but Callable::Closure is
    // `Fn(&dyn EventInterface)`, so a listener cannot mutate the event it receives. The Cache
    // half is likewise inexpressible: CacheMock carries only finder/gc overrides, with no
    // copy_to/copy_from hook to assert the cache key on.
    todo!()
}

#[test]
#[ignore = "the listener is a closure that mutates the event (setCustomCacheKey), but Callable::Closure receives `&dyn EventInterface`, so it cannot; and CacheMock has no copy_to/copy_from hooks to assert the cache key on"]
fn test_download_with_custom_cache_key() {
    // TODO(mock): the PRE_FILE_DOWNLOAD listener is a closure calling
    // PreFileDownloadEvent::setCustomCacheKey, but Callable::Closure is
    // `Fn(&dyn EventInterface)`, so a listener cannot mutate the event it receives. The Cache
    // half is likewise inexpressible: CacheMock carries only finder/gc overrides, with no
    // copy_to/copy_from hook to assert the cache key on.
    todo!()
}

#[test]
#[serial]
fn test_cache_garbage_collection_is_called() {
    let expected_ttl: i64 = 99999999;

    let mut config_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_options.insert(
        "cache-files-ttl".into(),
        PhpMixed::string(BString::from("99999999")),
    );
    config_options.insert(
        "cache-files-maxsize".into(),
        PhpMixed::string(BString::from("500M")),
    );
    let config = get_config(config_options);

    // The PHP Cache mock forces gcIsNecessary() true and records the single gc() call; the CacheMock
    // seam plays both roles here.
    let tmp_dir = get_unique_tmp_directory();
    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let mut cache = Cache::new(io, tmp_dir.path(), None, None, false);
    cache.__set_mock(CacheMock {
        gc_is_necessary: Some(true),
        gc_calls: Some(Vec::new()),
        ..Default::default()
    });
    let cache = std::rc::Rc::new(std::cell::RefCell::new(cache));

    let (http_downloader, _guard) = get_http_downloader_mock(
        Vec::new(),
        false,
        HttpDownloaderMockHandler {
            status: 200,
            body: BString::from("file~"),
            headers: Vec::new(),
        },
    );

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
        std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
    let _downloader = FileDownloader::new(
        io,
        config,
        http_downloader,
        None,
        Some(cache.clone()),
        None,
        None,
    )
    .unwrap();

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
    let package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    package.set_dist_url(Some(BString::from("http://example.com/script.js")));
    package.__set_dist_sha1_checksum(Some(BString::from("invalid")));

    let tmp_dir = get_unique_tmp_directory();
    let path = tmp_dir.path().to_path_buf();
    let mut config_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".into(),
        PhpMixed::string(path_to_bytes(&tmp_dir.path().join("vendor"))),
    );
    let config = get_config(config_options);

    // The PHP test injects a Filesystem mock so cleanup is a no-op; a real Filesystem behaves the
    // same here since nothing under the temp dir needs preserving.
    let (downloader, http_downloader, _guard) = get_downloader(None, Some(config));

    // make sure the file expected to be downloaded is on disk already
    let dl_file = downloader
        .__get_file_name(package.clone(), std::path::Path::new(&path))
        .unwrap();
    let dir = std::path::Path::new(&dl_file).parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(&dl_file, b"").unwrap();

    let loop_ = Loop::new(http_downloader, None);
    let promise = Box::pin(async {
        downloader
            .download_with_output(package.clone(), std::path::Path::new(&path), None, true)
            .await
            .map(|_| ())
    });
    let result = run(loop_.wait(vec![promise], None));

    let e = result.expect_err("download was expected to throw");
    assert!(
        e.is_instanceof::<UnexpectedValueException>(),
        "expected UnexpectedValueException, got: {e}"
    );
    assert!(
        error_message(&e).contains_str("checksum verification"),
        "unexpected message: {e}"
    );
}

#[test]
#[serial]
fn test_downgrade_shows_appropriate_message() {
    let old_package = get_package(BStr::new("dummy/pkg"), BStr::new("1.2.0"));
    let new_package = get_package(BStr::new("dummy/pkg"), BStr::new("1.0.0"));
    new_package.set_dist_url(Some(BString::from("http://example.com/script.js")));

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

    let tmp_dir = get_unique_tmp_directory();
    let path = tmp_dir.path().to_path_buf();
    let mut config_options: IndexMap<BString, PhpMixed> = IndexMap::new();
    config_options.insert(
        "vendor-dir".into(),
        PhpMixed::string(path_to_bytes(&tmp_dir.path().join("vendor"))),
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
    let filesystem = std::rc::Rc::new(std::cell::RefCell::new(filesystem));

    let (http_downloader, _guard) = get_http_downloader_mock(
        Vec::new(),
        false,
        HttpDownloaderMockHandler {
            status: 200,
            body: BString::from("file~"),
            headers: Vec::new(),
        },
    );

    let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> = io_mock;
    let downloader = FileDownloader::new(
        io,
        config,
        http_downloader.clone(),
        None,
        None,
        Some(filesystem.clone()),
        None,
    )
    .unwrap();

    // make sure the file expected to be downloaded is on disk already
    let dl_file = downloader
        .__get_file_name(new_package.clone(), std::path::Path::new(&path))
        .unwrap();
    let dir = std::path::Path::new(&dl_file).parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(&dl_file, b"").unwrap();

    let loop_ = Loop::new(http_downloader, None);
    let promise = Box::pin(async {
        downloader
            .download_with_output(
                new_package.clone(),
                std::path::Path::new(&path),
                Some(old_package.clone()),
                true,
            )
            .await
            .map(|_| ())
    });
    run(loop_.wait(vec![promise], None)).expect("download should succeed");

    run(downloader.update(
        old_package.clone(),
        new_package.clone(),
        std::path::Path::new(&path),
    ))
    .expect("update should succeed");

    assert_eq!(
        filesystem.borrow().__remove_directory_async_calls(),
        1,
        "removeDirectoryAsync should be called exactly once"
    );
}
