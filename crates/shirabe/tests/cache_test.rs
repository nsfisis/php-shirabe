//! ref: composer/tests/Composer/Test/CacheTest.php

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use shirabe::cache::{Cache, CacheMock, GcFinderMock};
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::util::filesystem::Filesystem;
use tempfile::TempDir;

struct SetUp {
    root: TempDir,
    files: Vec<std::path::PathBuf>,
    cache: Cache,
}

fn set_up() -> SetUp {
    let root = TempDir::new().unwrap();
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let zeros = "0".repeat(1000);

    for i in 0..4 {
        let path = root.path().join(format!("cached.file{}.zip", i));
        fs::write(&path, &zeros).unwrap();
        files.push(path);
    }

    // PHP mocks Cache::getFinder and keeps the real Filesystem; here the CacheMock finder seam plays
    // that role and the Cache otherwise operates on the real temp directory.
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let cache = Cache::new(io, root.path().to_str().unwrap(), None, None, false);

    SetUp { root, files, cache }
}

fn tear_down(root: &std::path::Path) {
    if root.is_dir() {
        let mut fs = Filesystem::new(None);
        fs.remove_directory(root).unwrap();
    }
}

struct TearDown {
    root: std::path::PathBuf,
}

impl TearDown {
    fn new(root: std::path::PathBuf) -> Self {
        TearDown { root }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.root);
    }
}

#[test]
fn test_remove_outdated_files() {
    let SetUp {
        root,
        files,
        mut cache,
    } = set_up();
    let _tear_down = TearDown::new(root.path().to_path_buf());

    // The date('until ...') finder yields the outdated entries (files 1..3).
    let outdated = files[1..].to_vec();
    cache.__set_mock(CacheMock {
        finder: Some(GcFinderMock {
            outdated,
            by_accessed_time: Vec::new(),
        }),
        ..Default::default()
    });

    cache.gc(600, 1024 * 1024 * 1024);

    for (i, file) in files.iter().enumerate().skip(1) {
        assert!(!file.exists(), "cached.file{i}.zip should be removed");
    }
    assert!(files[0].exists(), "cached.file0.zip should still exist");
}

#[test]
fn test_remove_files_when_cache_is_too_large() {
    let SetUp {
        root,
        files,
        mut cache,
    } = set_up();
    let _tear_down = TearDown::new(root.path().to_path_buf());

    // The date filter matches nothing; the size-bound pass walks all files by accessed time.
    cache.__set_mock(CacheMock {
        finder: Some(GcFinderMock {
            outdated: Vec::new(),
            by_accessed_time: files.clone(),
        }),
        ..Default::default()
    });

    cache.gc(600, 1500);

    for (i, file) in files.iter().enumerate().take(3) {
        assert!(!file.exists(), "cached.file{i}.zip should be removed");
    }
    assert!(files[3].exists(), "cached.file3.zip should still exist");
}

#[test]
fn test_clear_cache() {
    let root = TempDir::new().unwrap();
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let mut cache = Cache::new(
        io,
        root.path().to_str().unwrap(),
        Some("a-z0-9."),
        None,
        false,
    );

    assert!(cache.clear());
}
