//! ref: composer/tests/Composer/Test/Downloader/XzDownloaderTest.php

use shirabe::util::Platform;
use shirabe::util::filesystem::Filesystem;
use tempfile::TempDir;

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

#[test]
#[ignore = "needs a real HttpDownloader/Loop and an XzDownloader download cycle; HttpDownloader construction reaches curl_multi_init (todo!()) in the php-shim"]
fn test_error_messages() {
    let test_dir = set_up();
    let _tear_down = TearDown::new(test_dir.path().to_path_buf());
    let _ = &test_dir;
    todo!()
}
