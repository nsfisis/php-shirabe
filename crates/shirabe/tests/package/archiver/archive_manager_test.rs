//! ref: composer/tests/Composer/Test/Package/Archiver/ArchiveManagerTest.php sha1:8b1349d1c99f9a96c2f8fd633d64b26a6d364446

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::factory::Factory;
use shirabe::io::{IOInterface, NullIO};
use shirabe::package::archiver::ArchiveManager;
use shirabe::package::{CompletePackageHandle, CompletePackageInterfaceHandle};
use shirabe::util::r#loop::Loop;
use shirabe::util::{ProcessExecutor, StreamContextOptions};
use shirabe_php_shim::{
    Catch as _, PhpMixed, RuntimeException, bconcat, file_exists, file_put_contents,
    path_from_bytes, path_to_bytes, realpath, sys_get_temp_dir, unlink,
};
use shirabe_symfony_process::Process;
use tempfile::TempDir;

// ref: ArchiverTestCase::setUp + ArchiveManagerTest::setUp.
struct TestCase {
    manager: ArchiveManager,
    test_dir: std::path::PathBuf,
    target_dir: std::path::PathBuf,
    _test_dir_guard: TempDir,
}

impl TestCase {
    fn set_up() -> Self {
        let guard = TempDir::new().unwrap();
        let test_dir = guard.path().to_path_buf();

        let factory = Factory::default();
        let io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
        let config = std::rc::Rc::new(std::cell::RefCell::new(
            Factory::__create_config_mock(None).unwrap(),
        ));
        let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(
            Factory::create_http_downloader(io.clone(), &config, StreamContextOptions::new())
                .unwrap(),
        ));
        let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
            io.clone(),
        ))));
        let dm = factory
            .create_download_manager(io, &config, &http_downloader, &process, None)
            .unwrap();
        let r#loop = std::rc::Rc::new(std::cell::RefCell::new(Loop::new(http_downloader, None)));
        let manager = factory
            .create_archive_manager(&Factory::create_config(None, None).unwrap(), &dm, &r#loop)
            .unwrap();

        let target_dir = test_dir.join("composer_archiver_tests");

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

        package.into()
    }

    // ref: ArchiveManagerTest::getTargetName.
    fn get_target_name(
        &self,
        package: CompletePackageInterfaceHandle,
        format: &BStr,
    ) -> std::path::PathBuf {
        let package_name = self.manager.get_package_filename(package).unwrap();
        self.target_dir
            .join(path_from_bytes(&bconcat!(package_name, ".", format)))
    }

    // ref: ArchiveManagerTest::setupGitRepo.
    //
    // PHP runs the git commands individually through ProcessExecutor; the chained form here is
    // effect-equivalent for setting up the local repository the archive path clones from.
    fn setup_git_repo(&self) {
        file_put_contents(
            self.test_dir.join("composer.json"),
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
            Some(std::path::Path::new(&self.test_dir)),
            None,
            PhpMixed::Bool(false),
            Some(60.0),
        )
        .unwrap();
        let result = process.run(None, IndexMap::new()).unwrap();
        if result > 0 {
            panic!(
                "Could not set up git repo: {}",
                process
                    .get_error_output()
                    .unwrap_or_default()
                    .display_lossy()
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

    let result = shirabe::util::sync_executor::block_on(test_case.manager.archive(
        package,
        "__unknown_format__".into(),
        test_case.target_dir.clone(),
        None,
        false,
    ));

    let err = result.expect_err("expected RuntimeException for unknown format");
    assert!(err.is_instanceof::<RuntimeException>());
}

// ref: ArchiveManagerTest::testArchiveTar / testArchiveCustomFileName.
#[test]
fn test_archive_tar() {
    if !git_is_executable() {
        return;
    }

    let mut test_case = TestCase::set_up();

    test_case.setup_git_repo();

    let package = test_case.setup_package();

    shirabe::util::sync_executor::block_on(test_case.manager.archive(
        package.clone(),
        "tar".into(),
        test_case.target_dir.clone(),
        None,
        false,
    ))
    .unwrap();

    let target = test_case.get_target_name(package.clone(), BStr::new("tar"));
    assert!(file_exists(&target));

    let tmppath = sys_get_temp_dir()
        .join("composer_archiver")
        .join(path_from_bytes(
            &test_case.manager.get_package_filename(package).unwrap(),
        ));
    assert!(!file_exists(&tmppath));

    unlink(&target);
}

#[test]
fn test_archive_custom_file_name() {
    if !git_is_executable() {
        return;
    }

    let mut test_case = TestCase::set_up();

    test_case.setup_git_repo();

    let package = test_case.setup_package();

    let file_name = "testArchiveName";

    shirabe::util::sync_executor::block_on(test_case.manager.archive(
        package.clone(),
        "tar".into(),
        test_case.target_dir.clone(),
        Some(file_name.into()),
        false,
    ))
    .unwrap();

    let target = test_case
        .target_dir
        .join(path_from_bytes(&bconcat!(file_name, ".tar")));

    assert!(file_exists(&target));

    let tmppath = sys_get_temp_dir()
        .join("composer_archiver")
        .join(path_from_bytes(
            &test_case.manager.get_package_filename(package).unwrap(),
        ));
    assert!(!file_exists(&tmppath));

    unlink(&target);
}

#[test]
fn test_get_package_filename_parts() {
    let test_case = TestCase::set_up();
    let package = test_case.setup_package();

    let mut expected: IndexMap<bstr::BString, bstr::BString> = IndexMap::new();
    expected.insert("base".into(), "archivertest-archivertest".into());
    expected.insert("version".into(), "master".into());
    expected.insert("source_reference".into(), "4f26ae".into());

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
