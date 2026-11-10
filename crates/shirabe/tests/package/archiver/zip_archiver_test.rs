//! ref: composer/tests/Composer/Test/Package/Archiver/ZipArchiverTest.php sha1:0c48cf2bc2810e10439d89b1d11ec8c133635320

use crate::test_case::get_unique_tmp_directory;
use bstr::{BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe::package::CompletePackageHandle;
use shirabe::package::archiver::{ArchiverInterface, ZipArchiver};
use shirabe::util::Platform;
use shirabe_php_shim::{
    ZipArchive, bformat, dirname, file_exists, file_put_contents, mkdir, path_from_bytes,
    path_to_bytes, realpath, sys_get_temp_dir, unlink,
};

// ref: ArchiverTestCase. Holds the unique tmp testDir plus the ZipArchiverTest cleanup list.
struct ArchiverTestCase {
    test_dir: std::path::PathBuf,
    _test_dir_guard: tempfile::TempDir,
    files_to_cleanup: Vec<std::path::PathBuf>,
}

impl Drop for ArchiverTestCase {
    fn drop(&mut self) {
        for file in &self.files_to_cleanup {
            unlink(file);
        }
    }
}

impl ArchiverTestCase {
    fn set_up() -> Self {
        let guard = get_unique_tmp_directory();
        let test_dir = guard.path().to_path_buf();
        Self {
            test_dir,
            _test_dir_guard: guard,
            files_to_cleanup: vec![],
        }
    }

    fn setup_package(&self) -> CompletePackageHandle {
        let package = CompletePackageHandle::new(
            BString::from("archivertest/archivertest"),
            BString::from("master"),
            BString::from("master"),
        );
        package.set_source_url(Some(BString::from(
            realpath(&self.test_dir)
                .as_deref()
                .map(path_to_bytes)
                .unwrap_or_default(),
        )));
        package.set_source_reference(Some(BString::from("master")));
        package.__set_source_type(Some(BString::from("git")));

        package
    }

    fn assert_zip_archive(&mut self, mut files: IndexMap<BString, Option<BString>>) {
        // class_exists('ZipArchive') is always true in Shirabe, which implements ZipArchive
        // natively, so the missing-class skip never applies here.

        self.setup_dummy_repo(&mut files);
        let package = self.setup_package();
        let target = sys_get_temp_dir().join("composer_archiver_test.zip");
        self.files_to_cleanup.push(target.clone());

        let archiver = ZipArchiver::new();
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
        let mut zip = ZipArchive::new();
        let res = zip.open(&target, 0);
        assert!(res.is_ok(), "Failed asserting that Zip file can be opened");

        let mut zip_contents: IndexMap<BString, Option<BString>> = IndexMap::new();
        for i in 0..zip.num_files {
            let path = zip.get_name_index(i);
            let contents = zip.get_from_name(&path).map(BString::from);
            zip_contents.insert(path, contents);
        }
        zip.close();

        let files: IndexMap<BString, Option<BString>> = files.into_iter().collect();
        assert_eq!(
            files, zip_contents,
            "Failed asserting that Zip created with the ZipArchiver contains all files from the repository."
        );
    }

    fn setup_dummy_repo(&self, files: &mut IndexMap<BString, Option<BString>>) {
        let current_work_dir = Platform::get_cwd(false).unwrap();
        std::env::set_current_dir(&self.test_dir).unwrap();
        let paths: Vec<BString> = files.keys().cloned().collect();
        for path in paths {
            if files[&path].is_none() {
                files.insert(path.clone(), Some(BString::from("content")));
            }
            self.write_file(
                path.as_bstr(),
                files[&path].clone().unwrap(),
                &current_work_dir,
            );
        }

        std::env::set_current_dir(&current_work_dir).unwrap();
    }

    fn write_file(&self, path: &BStr, content: BString, current_work_dir: &std::path::Path) {
        if !file_exists(path_from_bytes(&dirname(path))) {
            mkdir(path_from_bytes(&dirname(path)), 0o777, true);
        }

        let result = file_put_contents(path_from_bytes(&path), content.as_slice());
        if result.is_none() {
            std::env::set_current_dir(current_work_dir).unwrap();
            panic!("Could not save file.");
        }
    }
}

#[test]
#[serial_test::serial]
fn test_simple_files() {
    let mut test_case = ArchiverTestCase::set_up();

    let mut files: IndexMap<BString, Option<BString>> = IndexMap::new();
    files.insert(BString::from("file.txt"), None);
    files.insert(BString::from("foo/bar/baz"), None);
    files.insert(BString::from("x/baz"), None);
    files.insert(BString::from("x/includeme"), None);

    if !Platform::is_windows() {
        files.insert(
            bformat!("zfoo{}/file.txt", Platform::get_cwd(false).unwrap()),
            None,
        );
    }

    test_case.assert_zip_archive(files);
}

#[test]
#[serial_test::serial]
fn test_gitignore_exclude_negation() {
    for include in ["!/docs", "!/docs/"] {
        let mut test_case = ArchiverTestCase::set_up();

        let mut files: IndexMap<BString, Option<BString>> = IndexMap::new();
        files.insert(
            BString::from(".gitignore"),
            Some(bformat!("/*\n.*\n!.git*\n{include}")),
        );
        files.insert(
            BString::from("docs/README.md"),
            Some(BString::from("# The doc")),
        );

        test_case.assert_zip_archive(files);
    }
}

#[test]
#[serial_test::serial]
fn test_folder_with_backslashes() {
    if Platform::is_windows() {
        // markTestSkipped('Folder names cannot contain backslashes on Windows.')
        return;
    }

    let mut test_case = ArchiverTestCase::set_up();

    let mut files: IndexMap<BString, Option<BString>> = IndexMap::new();
    files.insert(
        BString::from("folder\\with\\backslashes/README.md"),
        Some(BString::from("# doc")),
    );

    test_case.assert_zip_archive(files);
}
