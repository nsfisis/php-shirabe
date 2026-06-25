//! ref: composer/tests/Composer/Test/Package/Archiver/ZipArchiverTest.php

use indexmap::IndexMap;
use serial_test::serial;
use shirabe::package::archiver::{ArchiverInterface, ZipArchiver};
use shirabe::package::handle::CompletePackageHandle;
use shirabe::util::Platform;
use shirabe_php_shim::{
    ZipArchive, class_exists, dirname, file_exists, file_put_contents, mkdir, realpath,
    sys_get_temp_dir, unlink,
};
use tempfile::TempDir;

// ref: ArchiverTestCase. Holds the unique tmp testDir plus the ZipArchiverTest cleanup list.
struct ArchiverTestCase {
    test_dir: String,
    _test_dir_guard: TempDir,
    files_to_cleanup: Vec<String>,
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
        let guard = TempDir::new().unwrap();
        let test_dir = guard.path().to_string_lossy().to_string();
        Self {
            test_dir,
            _test_dir_guard: guard,
            files_to_cleanup: vec![],
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

    fn assert_zip_archive(&mut self, mut files: IndexMap<String, Option<String>>) {
        if !class_exists("ZipArchive") {
            // markTestSkipped('Cannot run ZipArchiverTest, missing class "ZipArchive".')
            return;
        }

        self.setup_dummy_repo(&mut files);
        let package = self.setup_package();
        let target = format!("{}/composer_archiver_test.zip", sys_get_temp_dir());
        self.files_to_cleanup.push(target.clone());

        let archiver = ZipArchiver::new();
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
        let mut zip = ZipArchive::new();
        let res = zip.open(&target, 0);
        assert!(res.is_ok(), "Failed asserting that Zip file can be opened");

        let mut zip_contents: IndexMap<String, Option<String>> = IndexMap::new();
        for i in 0..zip.num_files {
            let path = zip.get_name_index(i);
            zip_contents.insert(path.clone(), zip.get_from_name(&path));
        }
        zip.close();

        let files: IndexMap<String, Option<String>> = files;
        assert_eq!(
            files, zip_contents,
            "Failed asserting that Zip created with the ZipArchiver contains all files from the repository."
        );
    }

    fn setup_dummy_repo(&self, files: &mut IndexMap<String, Option<String>>) {
        let current_work_dir = Platform::get_cwd(false).unwrap();
        std::env::set_current_dir(&self.test_dir).unwrap();
        let paths: Vec<String> = files.keys().cloned().collect();
        for path in paths {
            if files[&path].is_none() {
                files.insert(path.clone(), Some("content".to_string()));
            }
            self.write_file(&path, files[&path].clone().unwrap(), &current_work_dir);
        }

        std::env::set_current_dir(&current_work_dir).unwrap();
    }

    fn write_file(&self, path: &str, content: String, current_work_dir: &str) {
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

#[test]
#[serial]
fn test_simple_files() {
    let mut test_case = ArchiverTestCase::set_up();

    let mut files: IndexMap<String, Option<String>> = IndexMap::new();
    files.insert("file.txt".to_string(), None);
    files.insert("foo/bar/baz".to_string(), None);
    files.insert("x/baz".to_string(), None);
    files.insert("x/includeme".to_string(), None);

    if !Platform::is_windows() {
        files.insert(
            format!("zfoo{}/file.txt", Platform::get_cwd(false).unwrap()),
            None,
        );
    }

    test_case.assert_zip_archive(files);
}

#[test]
#[serial]
fn test_gitignore_exclude_negation() {
    for include in ["!/docs", "!/docs/"] {
        let mut test_case = ArchiverTestCase::set_up();

        let mut files: IndexMap<String, Option<String>> = IndexMap::new();
        files.insert(
            ".gitignore".to_string(),
            Some(format!("/*\n.*\n!.git*\n{}", include)),
        );
        files.insert("docs/README.md".to_string(), Some("# The doc".to_string()));

        test_case.assert_zip_archive(files);
    }
}

#[test]
#[serial]
fn test_folder_with_backslashes() {
    if Platform::is_windows() {
        // markTestSkipped('Folder names cannot contain backslashes on Windows.')
        return;
    }

    let mut test_case = ArchiverTestCase::set_up();

    let mut files: IndexMap<String, Option<String>> = IndexMap::new();
    files.insert(
        "folder\\with\\backslashes/README.md".to_string(),
        Some("# doc".to_string()),
    );

    test_case.assert_zip_archive(files);
}
