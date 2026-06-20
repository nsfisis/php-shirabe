//! ref: composer/tests/Composer/Test/CacheTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::cache::Cache;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use tempfile::TempDir;

// In PHP these mock Cache::getFinder() to feed the gc() routine a controlled set of
// files. getFinder is pub(crate) and cannot be overridden from a test, so the
// finder-driven removal paths cannot be exercised faithfully here.
#[test]
#[ignore = "mocks Cache::getFinder to drive gc(); getFinder cannot be overridden from a test"]
fn test_remove_outdated_files() {
    todo!()
}

#[test]
#[ignore = "mocks Cache::getFinder to drive gc(); getFinder cannot be overridden from a test"]
fn test_remove_files_when_cache_is_too_large() {
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
