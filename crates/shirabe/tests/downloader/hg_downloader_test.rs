//! ref: composer/tests/Composer/Test/Downloader/HgDownloaderTest.php

use crate::io_stub::IOStub;
use crate::process_executor_mock::{cmd, get_process_executor_mock};
use shirabe::config::Config;
use shirabe::downloader::VcsDownloader;
use shirabe::downloader::hg_downloader::HgDownloader;
use shirabe::io::IOInterface;
use shirabe::package::handle::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::ProcessExecutor;
use shirabe::util::filesystem::{Filesystem, FilesystemMock};
use shirabe_semver::VersionParser;
use std::cell::RefCell;
use std::rc::Rc;
use tempfile::TempDir;

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
        .block_on(future)
}

fn set_up() -> TempDir {
    TempDir::new().unwrap()
}

fn tear_down(working_dir: &std::path::Path) {
    if working_dir.is_dir() {
        let mut fs = Filesystem::new(None);
        fs.remove_directory(working_dir).unwrap();
    }
}

struct TearDown {
    working_dir: std::path::PathBuf,
}

impl TearDown {
    fn new(working_dir: std::path::PathBuf) -> Self {
        TearDown { working_dir }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.working_dir);
    }
}

/// ref: TestCase::getMockBuilder('Composer\Package\PackageInterface')->getMock()
///
/// A real CompletePackage seeded with the stubbed values is a faithful stand-in
/// for a PackageInterface mock as long as `getSourceUrls()` equals
/// `[getSourceUrl()]`, which holds for every non-ignored case here.
fn get_package(source_reference: Option<&str>, source_url: Option<&str>) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize("1.0.0", None).unwrap();
    let package =
        CompletePackageHandle::new("dummy/pkg".to_string(), norm_version, "1.0.0".to_string());
    package.__set_source_type(Some("hg".to_string()));
    package.set_source_reference(source_reference.map(|s| s.to_string()));
    package.set_source_url(source_url.map(|s| s.to_string()));
    package.into()
}

/// ref: HgDownloaderTest::getDownloaderMock
///
/// PHP uses a `getMockBuilder('Composer\Config')->getMock()`, whose `get()`
/// returns null for everything; a default real `Config` resolves the only
/// relevant key (`secure-http`) compatibly for the https URLs used here.
fn get_downloader_mock(
    io: Option<Rc<RefCell<dyn IOInterface>>>,
    config: Option<Config>,
    process: Rc<RefCell<ProcessExecutor>>,
    filesystem: Option<Rc<RefCell<Filesystem>>>,
) -> HgDownloader {
    let io =
        io.unwrap_or_else(|| Rc::new(RefCell::new(IOStub::new())) as Rc<RefCell<dyn IOInterface>>);
    let config = Rc::new(RefCell::new(
        config.unwrap_or_else(|| Config::new(false, None)),
    ));
    let fs = filesystem.unwrap_or_else(|| Rc::new(RefCell::new(Filesystem::new(None))));
    HgDownloader::new(io, config, process, fs)
}

#[test]
fn test_download_for_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;

    let package = get_package(None, None);

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let mut downloader = get_downloader_mock(None, None, process, None);

    let result = run(downloader.install(package, "/path"));
    let e = result.expect_err("missing source reference should throw");
    assert!(e.to_string().contains("missing reference information"));
}

#[test]
fn test_download() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let working_dir_str = working_dir.path().to_string_lossy().into_owned();
    let package = get_package(Some("ref"), Some("https://mercurial.dev/l3l0/composer"));

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(vec![
                "hg",
                "clone",
                "--",
                "https://mercurial.dev/l3l0/composer",
                &working_dir_str,
            ]),
            cmd(vec!["hg", "up", "--", "ref"]),
        ],
        true,
        Default::default(),
    );

    let mut downloader = get_downloader_mock(None, None, process, None);
    run(downloader.install(package, &working_dir_str)).unwrap();
}

#[test]
fn test_updatefor_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;

    let initial_package = get_package(Some("ref"), None);
    let source_package = get_package(None, None);

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let mut downloader = get_downloader_mock(None, None, process, None);

    let result = run(async {
        downloader
            .prepare(
                "update",
                source_package.clone(),
                "/path",
                Some(initial_package.clone()),
            )
            .await?;
        downloader
            .update(initial_package.clone(), source_package.clone(), "/path")
            .await?;
        downloader
            .cleanup("update", source_package, "/path", Some(initial_package))
            .await
    });

    let e = result.expect_err("missing source reference should throw");
    assert!(e.to_string().contains("missing reference information"));
}

#[test]
fn test_update() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let working_dir_str = working_dir.path().to_string_lossy().into_owned();

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(&format!("{}/.hg", working_dir_str))
        .unwrap();

    let package = get_package(Some("ref"), Some("https://github.com/l3l0/composer"));

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(vec!["hg", "st"]),
            cmd(vec!["hg", "pull", "--", "https://github.com/l3l0/composer"]),
            cmd(vec!["hg", "up", "--", "ref"]),
        ],
        true,
        Default::default(),
    );

    let mut downloader = get_downloader_mock(None, None, process, None);
    run(async {
        downloader
            .prepare(
                "update",
                package.clone(),
                &working_dir_str,
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .update(package.clone(), package.clone(), &working_dir_str)
            .await
            .unwrap();
        downloader
            .cleanup("update", package.clone(), &working_dir_str, Some(package))
            .await
            .unwrap();
    });
}

#[test]
fn test_remove() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let working_dir_str = working_dir.path().to_string_lossy().into_owned();

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(&format!("{}/.hg", working_dir_str))
        .unwrap();

    let package = get_package(None, None);

    let (process, _guard) =
        get_process_executor_mock(vec![cmd(vec!["hg", "st"])], true, Default::default());

    let mut filesystem = Filesystem::new(None);
    filesystem.__set_mock(FilesystemMock {
        remove_directory_async_result: Some(true),
        ..Default::default()
    });
    let filesystem = Rc::new(RefCell::new(filesystem));

    let mut downloader = get_downloader_mock(None, None, process, Some(filesystem.clone()));
    run(async {
        downloader
            .prepare("uninstall", package.clone(), &working_dir_str, None)
            .await
            .unwrap();
        downloader
            .remove(package.clone(), &working_dir_str)
            .await
            .unwrap();
        downloader
            .cleanup("uninstall", package, &working_dir_str, None)
            .await
            .unwrap();
    });

    assert_eq!(filesystem.borrow().__remove_directory_async_calls(), 1);
}

#[test]
fn test_get_installation_source() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let downloader = get_downloader_mock(None, None, process, None);

    assert_eq!("source", downloader.get_installation_source());
}
