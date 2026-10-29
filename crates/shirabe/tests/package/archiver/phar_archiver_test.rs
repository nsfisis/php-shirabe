//! ref: composer/tests/Composer/Test/Package/Archiver/PharArchiverTest.php

use serial_test::serial;
use shirabe::package::archiver::{ArchiverInterface, PharArchiver};
use shirabe::package::handle::CompletePackageHandle;
use shirabe::util::{Filesystem, Platform};
use shirabe_php_shim::{
    dirname, file_exists, file_put_contents, legacy_lossy, mkdir, path_from_bytes, path_to_bytes,
    realpath,
};
use tempfile::TempDir;

// ref: ArchiverTestCase.
struct ArchiverTestCase {
    filesystem: Filesystem,
    test_dir: std::path::PathBuf,
    _test_dir_guard: TempDir,
}

impl ArchiverTestCase {
    fn set_up() -> Self {
        let guard = TempDir::new().unwrap();
        let test_dir = guard.path().to_path_buf();
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
        package.set_source_url(Some(
            realpath(&self.test_dir)
                .map(|p| legacy_lossy(path_to_bytes(&p)).into_owned())
                .unwrap_or_default(),
        ));
        package.set_source_reference(Some("master".to_string()));
        package.__set_source_type(Some("git".to_string()));

        package
    }

    fn setup_dummy_repo(&self) {
        let current_work_dir = Platform::get_cwd(false).unwrap();
        std::env::set_current_dir(&self.test_dir).unwrap();

        self.write_file(
            "file.txt",
            "content",
            &legacy_lossy(path_to_bytes(&current_work_dir)),
        );
        self.write_file(
            "foo/bar/baz",
            "content",
            &legacy_lossy(path_to_bytes(&current_work_dir)),
        );
        self.write_file(
            "foo/bar/ignoreme",
            "content",
            &legacy_lossy(path_to_bytes(&current_work_dir)),
        );
        self.write_file(
            "x/baz",
            "content",
            &legacy_lossy(path_to_bytes(&current_work_dir)),
        );
        self.write_file(
            "x/includeme",
            "content",
            &legacy_lossy(path_to_bytes(&current_work_dir)),
        );

        std::env::set_current_dir(&current_work_dir).unwrap();
    }

    fn write_file(&self, path: &str, content: &str, current_work_dir: &str) {
        if !file_exists(path_from_bytes(&dirname(path))) {
            mkdir(path_from_bytes(&dirname(path)), 0o777, true);
        }

        let result = file_put_contents(path, content.as_bytes());
        if result.is_none() {
            std::env::set_current_dir(current_work_dir).unwrap();
            panic!("Could not save file.");
        }
    }
}

#[test]
#[serial]
fn test_tar_archive() {
    let mut test_case = ArchiverTestCase::set_up();

    test_case.setup_dummy_repo();
    let package = test_case.setup_package();
    let target_dir = TempDir::new().unwrap();
    let target = target_dir
        .path()
        .join("composer_archiver_test.tar")
        .to_str()
        .unwrap()
        .to_string();

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
        .remove_directory(path_from_bytes(&dirname(&target)))
        .unwrap();
}

#[test]
#[serial]
fn test_zip_archive() {
    let mut test_case = ArchiverTestCase::set_up();

    test_case.setup_dummy_repo();
    let package = test_case.setup_package();
    let target_dir = TempDir::new().unwrap();
    let target = target_dir
        .path()
        .join("composer_archiver_test.zip")
        .to_str()
        .unwrap()
        .to_string();

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
        .remove_directory(path_from_bytes(&dirname(&target)))
        .unwrap();
}
