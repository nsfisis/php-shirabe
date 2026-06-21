//! ref: composer/tests/Composer/Test/CacheTest.php

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use shirabe::cache::Cache;
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

    // The finder/filesystem/IO mocks and the Cache mock overriding getFinder are not ported.
    let cache: Cache = todo!();

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

// In PHP these mock Cache::getFinder() to feed the gc() routine a controlled set of
// files. getFinder is pub(crate) and cannot be overridden from a test, so the
// finder-driven removal paths cannot be exercised faithfully here.
#[ignore = "requires mocking Cache::get_finder (pub(crate), PHPUnit MockObject) to feed gc() a controlled Finder iterator; not overridable from a test"]
#[test]
fn test_remove_outdated_files() {
    let SetUp { root, files, cache } = set_up();
    let _tear_down = TearDown::new(root.path().to_path_buf());
    let _ = (&files, &cache);
    todo!()
}

#[ignore = "requires mocking Cache::get_finder (pub(crate), PHPUnit MockObject) to feed gc() a controlled Finder iterator; not overridable from a test"]
#[test]
fn test_remove_files_when_cache_is_too_large() {
    let SetUp { root, files, cache } = set_up();
    let _tear_down = TearDown::new(root.path().to_path_buf());
    let _ = (&files, &cache);
    todo!()
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
