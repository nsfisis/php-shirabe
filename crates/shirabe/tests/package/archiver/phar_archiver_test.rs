//! ref: composer/tests/Composer/Test/Package/Archiver/PharArchiverTest.php sha1:bb3df0d5b21448501a2b6f9bf54744293b1a33ec

use crate::test_case::get_unique_tmp_directory;
use bstr::{BStr, BString, ByteSlice};
use serial_test::serial;
use shirabe::package::CompletePackageHandle;
use shirabe::package::archiver::{ArchiverInterface, PharArchiver};
use shirabe::util::{Filesystem, Platform};
use shirabe_php_shim::{
    dirname, file_exists, file_put_contents, mkdir, path_from_bytes, path_to_bytes, realpath,
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
        let guard = get_unique_tmp_directory();
        let test_dir = guard.path().to_path_buf();
        Self {
            filesystem: Filesystem::new(None),
            test_dir,
            _test_dir_guard: guard,
        }
    }

    fn setup_package(&self) -> CompletePackageHandle {
        let package = CompletePackageHandle::new(
            BString::from("archivertest/archivertest"),
            BString::from("master"),
            BString::from("master"),
        );
        package.set_source_url(Some(bstr::BString::from(
            realpath(&self.test_dir)
                .as_deref()
                .map(path_to_bytes)
                .unwrap_or_default(),
        )));
        package.set_source_reference(Some(BString::from("master")));
        package.__set_source_type(Some(BString::from("git")));

        package
    }

    fn setup_dummy_repo(&self) {
        let current_work_dir = Platform::get_cwd(false).unwrap();
        std::env::set_current_dir(&self.test_dir).unwrap();

        self.write_file(
            BStr::new("file.txt"),
            BStr::new("content"),
            &current_work_dir,
        );
        self.write_file(
            BStr::new("foo/bar/baz"),
            BStr::new("content"),
            &current_work_dir,
        );
        self.write_file(
            BStr::new("foo/bar/ignoreme"),
            BStr::new("content"),
            &current_work_dir,
        );
        self.write_file(BStr::new("x/baz"), BStr::new("content"), &current_work_dir);
        self.write_file(
            BStr::new("x/includeme"),
            BStr::new("content"),
            &current_work_dir,
        );

        std::env::set_current_dir(&current_work_dir).unwrap();
    }

    fn write_file(&self, path: &BStr, content: &BStr, current_work_dir: &std::path::Path) {
        if !file_exists(path_from_bytes(&dirname(path))) {
            mkdir(path_from_bytes(&dirname(path)), 0o777, true);
        }

        let result = file_put_contents(path_from_bytes(&path), content.as_bytes());
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
    let target_dir = get_unique_tmp_directory();
    let target = target_dir.path().join("composer_archiver_test.tar");

    let archiver = PharArchiver::new();
    archiver
        .archive(
            path_from_bytes(&package.get_source_url().unwrap()),
            &target,
            "tar".into(),
            vec!["foo/bar".into(), "baz".into(), "!/foo/bar/baz".into()],
            false,
        )
        .unwrap();
    assert!(file_exists(&target));

    test_case
        .filesystem
        .remove_directory(path_from_bytes(&dirname(path_to_bytes(&target))))
        .unwrap();
}

#[test]
#[serial]
fn test_zip_archive() {
    let mut test_case = ArchiverTestCase::set_up();

    test_case.setup_dummy_repo();
    let package = test_case.setup_package();
    let target_dir = get_unique_tmp_directory();
    let target = target_dir.path().join("composer_archiver_test.zip");

    let archiver = PharArchiver::new();
    archiver
        .archive(
            path_from_bytes(&package.get_source_url().unwrap()),
            &target,
            "zip".into(),
            vec![],
            false,
        )
        .unwrap();
    assert!(file_exists(&target));

    test_case
        .filesystem
        .remove_directory(path_from_bytes(&dirname(path_to_bytes(&target))))
        .unwrap();
}
