//! ref: composer/tests/Composer/Test/Package/Archiver/ZipArchiverTest.php

/// Creates the Filesystem/ProcessExecutor and a unique tmp testDir; that infrastructure is
/// not ported. Returns testDir.
#[allow(dead_code)]
fn set_up() -> String {
    todo!()
}

/// Unlinks the zip files collected in filesToCleanup, then (parent) removes testDir.
#[allow(dead_code)]
fn tear_down(_files_to_cleanup: &[String], _test_dir: &str) {
    todo!()
}

#[allow(dead_code)]
struct TearDown {
    files_to_cleanup: Vec<String>,
    test_dir: String,
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.files_to_cleanup, &self.test_dir);
    }
}

// ZipArchiver::archive builds a zip via ZipArchive, which is todo!() in the php-shim.

#[test]
#[ignore = "ZipArchiver::archive builds a zip via ZipArchive, which is todo!() in the php-shim"]
fn test_simple_files() {
    todo!()
}

#[test]
#[ignore = "ZipArchiver::archive builds a zip via ZipArchive, which is todo!() in the php-shim"]
fn test_gitignore_exclude_negation() {
    todo!()
}

#[test]
#[ignore = "ZipArchiver::archive builds a zip via ZipArchive, which is todo!() in the php-shim"]
fn test_folder_with_backslashes() {
    todo!()
}
