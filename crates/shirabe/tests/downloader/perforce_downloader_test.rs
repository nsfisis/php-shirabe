//! ref: composer/tests/Composer/Test/Downloader/PerforceDownloaderTest.php

use crate::io_mock::{Expectation, get_io_mock};
use crate::io_stub::IOStub;
use crate::process_executor_mock::get_process_executor_mock;
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::downloader::VcsDownloader;
use shirabe::downloader::perforce_downloader::PerforceDownloader;
use shirabe::io::IOInterface;
use shirabe::io::io_interface::NORMAL;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::filesystem::Filesystem;
use shirabe::util::process_executor::MockHandler;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::VersionParser;
use std::cell::RefCell;
use std::rc::Rc;
use tempfile::TempDir;

// A getMockBuilder('Composer\Util\Perforce') stand-in: the seam trait extracted from the
// concrete `Perforce` struct, mocked so the downloader's workflow can be verified.
mockall::mock! {
    #[derive(Debug)]
    pub Perforce {}
    impl shirabe::util::PerforceInterface for Perforce {
        fn initialize_path(&mut self, path: &str);
        fn set_stream(&mut self, stream: &str);
        fn p4_login(&mut self) -> anyhow::Result<()>;
        fn check_stream(&mut self) -> bool;
        fn write_p4_client_spec(&mut self) -> anyhow::Result<()>;
        fn connect_client(&mut self) -> anyhow::Result<()>;
        fn sync_code_base(&mut self, source_reference: Option<String>) -> anyhow::Result<()>;
        fn cleanup_client_spec(&mut self);
        fn get_commit_logs(&mut self, from_reference: &str, to_reference: &str) -> Option<String>;
        fn get_file_content(&mut self, file: &str, identifier: &str) -> Option<String>;
        fn get_branches(&mut self) -> IndexMap<String, String>;
        fn get_tags(&mut self) -> IndexMap<String, String>;
        fn get_user(&self) -> Option<String>;
        fn get_composer_information(
            &mut self,
            identifier: &str,
        ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>>;
    }
}

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

/// ref: PerforceDownloaderTest::getConfig (seeds `home` with the temp dir)
fn get_config(test_path: &std::path::Path) -> Config {
    let mut config = Config::new(true, None);
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut section: IndexMap<String, PhpMixed> = IndexMap::new();
    section.insert(
        "home".to_string(),
        PhpMixed::String(test_path.to_string_lossy().into_owned()),
    );
    top.insert("config".to_string(), PhpMixed::Array(section));
    config.merge(&top, Config::SOURCE_UNKNOWN);
    config
}

/// ref: PerforceDownloaderTest::getMockPackageInterface. A real CompletePackage stands in for
/// the PHPUnit PackageInterface mock; the source reference is returned by getSourceReference.
fn make_package(source_reference: Option<&str>) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize("1.0.0", None).unwrap();
    let package =
        CompletePackageHandle::new("test/pkg".to_string(), norm_version, "1.0.0".to_string());
    package.set_source_reference(source_reference.map(|s| s.to_string()));
    package.into()
}

#[test]
fn test_init_perforce_instantiates_a_new_perforce_object() {
    // @doesNotPerformAssertions: only verifies init_perforce instantiates a Perforce without
    // error. PHP attaches a VcsRepository whose getRepoConfig seeds the config, but in the
    // current port VcsRepository implements ConfigurableRepositoryInterface only (not
    // RepositoryInterface), so it cannot be held in a RepositoryInterfaceHandle. The package
    // is therefore built without a repository, yielding an empty repo config.
    let test_path = TempDir::new().unwrap();
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));
    let config = Rc::new(RefCell::new(get_config(test_path.path())));
    let package = make_package(None);

    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());
    let fs = Rc::new(RefCell::new(Filesystem::new(None)));
    let mut downloader = PerforceDownloader::new(io, config, process, fs);

    downloader.init_perforce(
        package,
        test_path.path().to_string_lossy().into_owned(),
        "SOURCE_REF".to_string(),
    );
}

#[test]
fn test_init_perforce_does_nothing_if_perforce_already_set() {
    let test_path = TempDir::new().unwrap();
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));
    let config = Rc::new(RefCell::new(get_config(test_path.path())));
    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());
    let fs = Rc::new(RefCell::new(Filesystem::new(None)));
    let mut downloader = PerforceDownloader::new(io, config, process, fs);

    // The already-set perforce only sees initializePath; the repository's getRepoConfig is
    // never consulted (the early return happens before reaching it).
    let mut perforce = MockPerforce::new();
    perforce.expect_initialize_path().times(1).returning(|_| ());
    downloader.set_perforce(Box::new(perforce));

    let package = make_package(None);
    downloader.init_perforce(
        package,
        test_path.path().to_string_lossy().into_owned(),
        "SOURCE_REF".to_string(),
    );
}

#[test]
fn test_do_install_with_tag() {
    do_install_workflow("SOURCE_REF@123", Some("123".to_string()));
}

#[test]
fn test_do_install_with_no_tag() {
    do_install_workflow("SOURCE_REF", None);
}

// Shared body for testDoInstallWithTag / testDoInstallWithNoTag: enforce the install workflow
// against a mocked Perforce, asserting each step is invoked exactly once.
fn do_install_workflow(source_ref: &'static str, expected_label: Option<String>) {
    let test_path = TempDir::new().unwrap();
    let test_path_str = test_path.path().to_string_lossy().into_owned();

    let (io_mock, _io_guard) = get_io_mock(NORMAL).unwrap();
    io_mock
        .borrow_mut()
        .expects(
            vec![Expectation::text_regex(format!("Cloning {}", source_ref))],
            false,
        )
        .unwrap();
    let io: Rc<RefCell<dyn IOInterface>> = io_mock.clone();
    let config = Rc::new(RefCell::new(get_config(test_path.path())));
    let (process, _process_guard) =
        get_process_executor_mock(vec![], false, MockHandler::default());
    let fs = Rc::new(RefCell::new(Filesystem::new(None)));
    let mut downloader = PerforceDownloader::new(io, config, process, fs);

    let mut perforce = MockPerforce::new();
    let expected_path = test_path_str.clone();
    perforce
        .expect_initialize_path()
        .times(1)
        .withf(move |path: &str| path == expected_path)
        .returning(|_| ());
    perforce
        .expect_set_stream()
        .times(1)
        .withf(move |stream: &str| stream == source_ref)
        .returning(|_| ());
    perforce.expect_p4_login().times(1).returning(|| Ok(()));
    perforce
        .expect_write_p4_client_spec()
        .times(1)
        .returning(|| Ok(()));
    perforce
        .expect_connect_client()
        .times(1)
        .returning(|| Ok(()));
    perforce
        .expect_sync_code_base()
        .times(1)
        .withf(move |reference: &Option<String>| *reference == expected_label)
        .returning(|_| Ok(()));
    perforce
        .expect_cleanup_client_spec()
        .times(1)
        .returning(|| ());
    downloader.set_perforce(Box::new(perforce));

    let package = make_package(Some(source_ref));
    run(downloader.do_install(package, &test_path_str, "url")).unwrap();
}
