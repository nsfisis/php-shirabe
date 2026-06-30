//! ref: composer/tests/Composer/Test/Package/Archiver/ArchiveManagerTest.php

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::{DownloadManager, GitDownloader};
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::package::CompletePackageInterfaceHandle;
use shirabe::package::archiver::{ArchiveManager, PharArchiver, ZipArchiver};
use shirabe::package::handle::CompletePackageHandle;
use shirabe::util::Filesystem;
use shirabe::util::ProcessExecutor;
use shirabe::util::http_downloader::HttpDownloader;
use shirabe::util::r#loop::Loop;
use shirabe_external_packages::symfony::process::Process;
use shirabe_php_shim::{
    PhpMixed, file_exists, file_put_contents, realpath, sys_get_temp_dir, unlink,
};
use std::cell::RefCell;
use std::rc::Rc;
use tempfile::TempDir;

// ref: ArchiverTestCase::setUp + ArchiveManagerTest::setUp.
//
// The PHP test builds the ArchiveManager via Factory/FactoryMock; here we construct it directly
// with the same archivers (ZipArchiver + PharArchiver) that Factory::createArchiveManager adds.
struct TestCase {
    manager: ArchiveManager,
    test_dir: String,
    target_dir: String,
    _test_dir_guard: TempDir,
}

impl TestCase {
    fn set_up() -> Self {
        let guard = TempDir::new().unwrap();
        let test_dir = guard.path().to_string_lossy().to_string();

        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(false, None)));
        // The filename/unknown-format tests never drive the download path, so a mock
        // HttpDownloader (no curl backend) is sufficient to satisfy Loop's dependency.
        let http_downloader = Rc::new(RefCell::new(HttpDownloader::__new_mock(
            io.clone(),
            config.clone(),
        )));
        let process = Rc::new(RefCell::new(ProcessExecutor::new(Some(io.clone()))));
        let fs = Rc::new(RefCell::new(Filesystem::new(Some(process.clone()))));
        let mut dm = DownloadManager::new(io.clone(), false, Some(fs.clone()));
        // Factory::createDownloadManager registers a git downloader; the archive tests clone the
        // package source (source type 'git') through it.
        dm.set_downloader(
            "git",
            Rc::new(RefCell::new(GitDownloader::new(
                io.clone(),
                config.clone(),
                Some(process.clone()),
                Some(fs.clone()),
            ))),
        );
        let dm = Rc::new(RefCell::new(dm));
        let r#loop = Rc::new(RefCell::new(Loop::new(http_downloader, None)));

        let mut manager = ArchiveManager::new(dm, r#loop);
        manager.add_archiver(Box::new(ZipArchiver::new()));
        manager.add_archiver(Box::new(PharArchiver::new()));

        let target_dir = format!("{}/composer_archiver_tests", test_dir);

        Self {
            manager,
            test_dir,
            target_dir,
            _test_dir_guard: guard,
        }
    }

    // ref: ArchiverTestCase::setupPackage.
    fn setup_package(&self) -> CompletePackageInterfaceHandle {
        let package = CompletePackageHandle::new(
            "archivertest/archivertest".to_string(),
            "master".to_string(),
            "master".to_string(),
        );
        package.set_source_url(Some(realpath(&self.test_dir).unwrap_or_default()));
        package.set_source_reference(Some("master".to_string()));
        package.__set_source_type(Some("git".to_string()));

        package.into()
    }

    // ref: ArchiveManagerTest::getTargetName.
    fn get_target_name(&self, package: CompletePackageInterfaceHandle, format: &str) -> String {
        let package_name = self.manager.get_package_filename(package).unwrap();
        format!("{}/{}.{}", self.target_dir, package_name, format)
    }

    // ref: ArchiveManagerTest::setupGitRepo.
    //
    // PHP runs the git commands individually through ProcessExecutor; the chained form here is
    // effect-equivalent for setting up the local repository the archive path clones from.
    fn setup_git_repo(&self) {
        file_put_contents(
            &format!("{}/composer.json", self.test_dir),
            br#"{"name":"faker/faker", "description": "description", "license": "MIT"}"#,
        );

        let mut process = Process::from_shell_commandline(
            "git init -q && \
             git checkout -b master && \
             git config user.email \"you@example.com\" && \
             git config commit.gpgsign false && \
             git config user.name \"Your Name\" && \
             git add composer.json && \
             git commit -m \"commit composer.json\" -q",
            Some(&self.test_dir),
            None,
            PhpMixed::Bool(false),
            Some(60.0),
        )
        .unwrap();
        let result = process.run(None, IndexMap::new()).unwrap();
        if result > 0 {
            panic!(
                "Could not set up git repo: {}",
                process.get_error_output().unwrap_or_default()
            );
        }
    }
}

// ref: ArchiverTestCase::skipIfNotExecutable('git').
fn git_is_executable() -> bool {
    Process::from_shell_commandline(
        "git --version",
        None,
        None,
        PhpMixed::Bool(false),
        Some(60.0),
    )
    .and_then(|mut p| p.run(None, IndexMap::new()))
    .map(|code| code == 0)
    .unwrap_or(false)
}

#[test]
fn test_unknown_format() {
    let mut test_case = TestCase::set_up();
    let package = test_case.setup_package();

    let result = test_case.manager.archive(
        package,
        "__unknown_format__".to_string(),
        test_case.target_dir.clone(),
        None,
        false,
    );

    let err = result.expect_err("expected RuntimeException for unknown format");
    assert!(
        err.downcast_ref::<shirabe_php_shim::RuntimeException>()
            .is_some()
    );
}

// ref: ArchiveManagerTest::testArchiveTar / testArchiveCustomFileName.
//
// These drive ArchiveManager::archive end-to-end for the 'tar' format, which dispatches to
// PharArchiver::archive. That builds the archive via PharData, whose build_from_iterator is
// todo!() in the php-shim, so the archiving path cannot run yet.
#[test]
#[ignore = "needs PharData tar archiving (new_with_format/build_from_iterator are todo!() in the php-shim) for ArchiveManager::archive('tar', ...)"]
fn test_archive_tar() {
    if !git_is_executable() {
        return;
    }

    let mut test_case = TestCase::set_up();

    test_case.setup_git_repo();

    let package = test_case.setup_package();

    test_case
        .manager
        .archive(
            package.clone(),
            "tar".to_string(),
            test_case.target_dir.clone(),
            None,
            false,
        )
        .unwrap();

    let target = test_case.get_target_name(package.clone(), "tar");
    assert!(file_exists(&target));

    let tmppath = format!(
        "{}/composer_archiver/{}",
        sys_get_temp_dir(),
        test_case.manager.get_package_filename(package).unwrap()
    );
    assert!(!file_exists(&tmppath));

    unlink(&target);
}

#[test]
#[ignore = "needs PharData tar archiving (new_with_format/build_from_iterator are todo!() in the php-shim) for ArchiveManager::archive('tar', ...)"]
fn test_archive_custom_file_name() {
    if !git_is_executable() {
        return;
    }

    let mut test_case = TestCase::set_up();

    test_case.setup_git_repo();

    let package = test_case.setup_package();

    let file_name = "testArchiveName";

    test_case
        .manager
        .archive(
            package.clone(),
            "tar".to_string(),
            test_case.target_dir.clone(),
            Some(file_name.to_string()),
            false,
        )
        .unwrap();

    let target = format!("{}/{}.tar", test_case.target_dir, file_name);

    assert!(file_exists(&target));

    let tmppath = format!(
        "{}/composer_archiver/{}",
        sys_get_temp_dir(),
        test_case.manager.get_package_filename(package).unwrap()
    );
    assert!(!file_exists(&tmppath));

    unlink(&target);
}

#[test]
fn test_get_package_filename_parts() {
    let test_case = TestCase::set_up();
    let package = test_case.setup_package();

    let mut expected: IndexMap<String, String> = IndexMap::new();
    expected.insert("base".to_string(), "archivertest-archivertest".to_string());
    expected.insert("version".to_string(), "master".to_string());
    expected.insert("source_reference".to_string(), "4f26ae".to_string());

    assert_eq!(
        expected,
        test_case
            .manager
            .get_package_filename_parts(package)
            .unwrap()
    );
}

#[test]
fn test_get_package_filename() {
    let test_case = TestCase::set_up();
    let package = test_case.setup_package();

    assert_eq!(
        "archivertest-archivertest-master-4f26ae",
        test_case.manager.get_package_filename(package).unwrap()
    );
}
