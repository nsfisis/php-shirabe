//! ref: composer/tests/Composer/Test/Config/JsonConfigSourceTest.php

use shirabe::util::filesystem::Filesystem;
use std::path::PathBuf;
use tempfile::TempDir;

fn set_up() -> TearDown {
    let fs = Filesystem::new(None);
    // getUniqueTmpDirectory creates a fresh unique temp directory.
    let working_dir = TempDir::new().unwrap();
    TearDown { fs, working_dir }
}

struct TearDown {
    fs: Filesystem,
    working_dir: TempDir,
}

impl TearDown {
    fn working_dir(&self) -> PathBuf {
        self.working_dir.path().to_path_buf()
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        let working_dir = self.working_dir.path();
        if working_dir.is_dir() {
            self.fs.remove_directory(working_dir).unwrap();
        }
    }
}

#[test]
#[ignore = "test body not yet ported (todo!() stub)"]
fn test_add_repository() {
    let _tear_down = set_up();
    todo!()
}

#[test]
#[ignore = "test body not yet ported (todo!() stub)"]
fn test_add_repository_as_list() {
    let _tear_down = set_up();
    todo!()
}

#[test]
#[ignore = "test body not yet ported (todo!() stub)"]
fn test_add_repository_with_options() {
    let _tear_down = set_up();
    todo!()
}

#[test]
#[ignore = "test body not yet ported (todo!() stub)"]
fn test_remove_repository() {
    let _tear_down = set_up();
    todo!()
}

#[test]
#[ignore = "test body not yet ported (todo!() stub)"]
fn test_add_packagist_repository_with_false_value() {
    let _tear_down = set_up();
    todo!()
}

#[test]
#[ignore = "test body not yet ported (todo!() stub)"]
fn test_remove_packagist() {
    let _tear_down = set_up();
    todo!()
}

#[test]
#[ignore = "test body not yet ported (todo!() stub)"]
fn test_add_link() {
    let _tear_down = set_up();
    todo!()
}

#[test]
#[ignore = "test body not yet ported (todo!() stub)"]
fn test_remove_link() {
    let _tear_down = set_up();
    todo!()
}
