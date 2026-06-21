//! ref: composer/tests/Composer/Test/Downloader/ZipDownloaderTest.php

use shirabe::util::filesystem::Filesystem;
use tempfile::TempDir;

struct SetUp {
    test_dir: TempDir,
    filename: std::path::PathBuf,
}

fn set_up() -> SetUp {
    let test_dir = TempDir::new().unwrap();
    // The IO/Config/HttpDownloader/Package mocks are not ported; HttpDownloader construction
    // additionally reaches curl_multi_init (todo!()).
    let () = todo!();
    #[allow(unreachable_code)]
    {
        let filename = test_dir.path().join("composer-test.zip");
        std::fs::write(&filename, "zip").unwrap();
        SetUp { test_dir, filename }
    }
}

fn tear_down(test_dir: &std::path::Path) {
    let mut fs = Filesystem::new(None);
    fs.remove_directory(test_dir).unwrap();
    // setPrivateProperty('hasZipArchive', null) resets a ZipDownloader static via reflection;
    // the static is not reachable from a test here.
    todo!()
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

// These construct a ZipDownloader with a mocked IO/HttpDownloader/ProcessExecutor and rely
// on ZipArchive extraction (todo!() in the php-shim) plus mocked unzip behaviour.
#[test]
#[ignore = "mocks IO/HttpDownloader/ProcessExecutor and uses ZipArchive (todo!())"]
fn test_error_messages() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());
    let _ = (&set_up.test_dir, &set_up.filename);
    todo!()
}

#[test]
#[ignore = "mocks IO/HttpDownloader/ProcessExecutor and uses ZipArchive (todo!())"]
fn test_zip_archive_only_failed() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());
    let _ = (&set_up.test_dir, &set_up.filename);
    todo!()
}

#[test]
#[ignore = "mocks IO/HttpDownloader/ProcessExecutor and uses ZipArchive (todo!())"]
fn test_zip_archive_extract_only_failed() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());
    let _ = (&set_up.test_dir, &set_up.filename);
    todo!()
}

#[test]
#[ignore = "mocks IO/HttpDownloader/ProcessExecutor and uses ZipArchive (todo!())"]
fn test_zip_archive_only_good() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());
    let _ = (&set_up.test_dir, &set_up.filename);
    todo!()
}

#[test]
#[ignore = "mocks IO/HttpDownloader/ProcessExecutor and uses ZipArchive (todo!())"]
fn test_system_unzip_only_failed() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());
    let _ = (&set_up.test_dir, &set_up.filename);
    todo!()
}

#[test]
#[ignore = "mocks IO/HttpDownloader/ProcessExecutor and uses ZipArchive (todo!())"]
fn test_system_unzip_only_good() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());
    let _ = (&set_up.test_dir, &set_up.filename);
    todo!()
}

#[test]
#[ignore = "mocks IO/HttpDownloader/ProcessExecutor and uses ZipArchive (todo!())"]
fn test_non_windows_fallback_good() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());
    let _ = (&set_up.test_dir, &set_up.filename);
    todo!()
}

#[test]
#[ignore = "mocks IO/HttpDownloader/ProcessExecutor and uses ZipArchive (todo!())"]
fn test_non_windows_fallback_failed() {
    let set_up = set_up();
    let _tear_down = TearDown::new(set_up.test_dir.path().to_path_buf());
    let _ = (&set_up.test_dir, &set_up.filename);
    todo!()
}
