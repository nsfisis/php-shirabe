//! ref: composer/tests/Composer/Test/Package/Archiver/PharArchiverTest.php

use shirabe::package::archiver::{ArchiverInterface, PharArchiver};
use shirabe::package::handle::CompletePackageHandle;
use shirabe::util::{Filesystem, Platform};
use shirabe_php_shim::{dirname, file_exists, file_put_contents, mkdir, realpath};
use tempfile::TempDir;

// ref: ArchiverTestCase.
struct ArchiverTestCase {
    filesystem: Filesystem,
    test_dir: String,
    _test_dir_guard: TempDir,
}

impl ArchiverTestCase {
    fn set_up() -> Self {
        let guard = TempDir::new().unwrap();
        let test_dir = guard.path().to_string_lossy().to_string();
        Self {
            filesystem: Filesystem::new(None),
            test_dir,
            _test_dir_guard: guard,
        }
    }

    fn setup_package(&self) -> CompletePackageHandle {
        let package = CompletePackageHandle::new(
            "archivertest/archivertest".to_string(),
            "master".to_string(),
            "master".to_string(),
        );
        package.set_source_url(Some(realpath(&self.test_dir).unwrap_or_default()));
        package.set_source_reference(Some("master".to_string()));
        package.__set_source_type(Some("git".to_string()));

        package
    }

    fn setup_dummy_repo(&self) {
        let current_work_dir = Platform::get_cwd(false).unwrap();
        std::env::set_current_dir(&self.test_dir).unwrap();

        self.write_file("file.txt", "content", &current_work_dir);
        self.write_file("foo/bar/baz", "content", &current_work_dir);
        self.write_file("foo/bar/ignoreme", "content", &current_work_dir);
        self.write_file("x/baz", "content", &current_work_dir);
        self.write_file("x/includeme", "content", &current_work_dir);

        std::env::set_current_dir(&current_work_dir).unwrap();
    }

    fn write_file(&self, path: &str, content: &str, current_work_dir: &str) {
        if !file_exists(dirname(path)) {
            mkdir(&dirname(path), 0o777, true);
        }

        let result = file_put_contents(path, content.as_bytes());
        if result.is_none() {
            std::env::set_current_dir(current_work_dir).unwrap();
            panic!("Could not save file.");
        }
    }
}

#[ignore = "PharArchiver::archive builds the archive via PharData, which is todo!() in the php-shim"]
#[test]
fn test_tar_archive() {
    let mut test_case = ArchiverTestCase::set_up();

    test_case.setup_dummy_repo();
    let package = test_case.setup_package();
    let target_dir = TempDir::new().unwrap();
    let target = format!(
        "{}/composer_archiver_test.tar",
        target_dir.path().to_string_lossy()
    );

    let archiver = PharArchiver::new();
    archiver
        .archive(
            package.get_source_url().unwrap(),
            target.clone(),
            "tar".to_string(),
            vec![
                "foo/bar".to_string(),
                "baz".to_string(),
                "!/foo/bar/baz".to_string(),
            ],
            false,
        )
        .unwrap();
    assert!(file_exists(&target));

    test_case
        .filesystem
        .remove_directory(dirname(&target))
        .unwrap();
}

#[ignore = "PharArchiver::archive builds the archive via PharData, which is todo!() in the php-shim"]
#[test]
fn test_zip_archive() {
    let mut test_case = ArchiverTestCase::set_up();

    test_case.setup_dummy_repo();
    let package = test_case.setup_package();
    let target_dir = TempDir::new().unwrap();
    let target = format!(
        "{}/composer_archiver_test.zip",
        target_dir.path().to_string_lossy()
    );

    let archiver = PharArchiver::new();
    archiver
        .archive(
            package.get_source_url().unwrap(),
            target.clone(),
            "zip".to_string(),
            vec![],
            false,
        )
        .unwrap();
    assert!(file_exists(&target));

    test_case
        .filesystem
        .remove_directory(dirname(&target))
        .unwrap();
}
