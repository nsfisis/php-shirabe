//! ref: composer/tests/Composer/Test/Downloader/HgDownloaderTest.php sha1:bdae19ace358bd18971b021f8d1e4834e452a769

use crate::async_runtime::run;
use crate::io_stub::IOStub;
use crate::process_executor_mock::{cmd, get_process_executor_mock};
use crate::test_case::get_unique_tmp_directory;
use bstr::{BStr, BString, ByteSlice};
use shirabe::config::Config;
use shirabe::downloader::{HgDownloader, VcsDownloader};
use shirabe::io::IOInterface;
use shirabe::package::{CompletePackageHandle, PackageInterfaceHandle};
use shirabe::util::{Filesystem, FilesystemMock, ProcessExecutor};
use shirabe_php_shim::{bformat, error_message, path_from_bytes};
use shirabe_semver::VersionParser;
use tempfile::TempDir;

fn set_up() -> TempDir {
    get_unique_tmp_directory()
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
fn get_package(
    source_reference: Option<&BStr>,
    source_url: Option<&BStr>,
) -> PackageInterfaceHandle {
    let norm_version = VersionParser.normalize("1.0.0", None).unwrap();
    let package = CompletePackageHandle::new(
        BString::from("dummy/pkg"),
        norm_version,
        BString::from("1.0.0"),
    );
    package.__set_source_type(Some(BString::from("hg")));
    package.set_source_reference(source_reference.map(|s| bformat!("{s}")));
    package.set_source_url(source_url.map(|s| bformat!("{s}")));
    package.into()
}

/// ref: HgDownloaderTest::getDownloaderMock
///
/// PHP uses a `getMockBuilder('Composer\Config')->getMock()`, whose `get()`
/// returns null for everything; a default real `Config` resolves the only
/// relevant key (`secure-http`) compatibly for the https URLs used here.
fn get_downloader_mock(
    io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    config: Option<Config>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
) -> HgDownloader {
    let io = io.unwrap_or_else(|| {
        std::rc::Rc::new(std::cell::RefCell::new(IOStub::new()))
            as std::rc::Rc<std::cell::RefCell<dyn IOInterface>>
    });
    let config = std::rc::Rc::new(std::cell::RefCell::new(
        config.unwrap_or_else(|| Config::new(false, None)),
    ));
    let fs = filesystem
        .unwrap_or_else(|| std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))));
    HgDownloader::new(io, config, process, fs)
}

#[test]
fn test_download_for_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;

    let package = get_package(None, None);

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let downloader = get_downloader_mock(None, None, process, None);

    let result = run(downloader.install(package, std::path::Path::new("/path")));
    let e = result.expect_err("missing source reference should throw");
    assert!(error_message(&e).contains_str("missing reference information"));
}

#[test]
fn test_download() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let working_dir_str = bformat!("{}", working_dir.path());
    let package = get_package(
        Some(BStr::new("ref")),
        Some(BStr::new("https://mercurial.dev/l3l0/composer")),
    );

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(vec![
                BStr::new("hg"),
                BStr::new("clone"),
                BStr::new("--"),
                BStr::new("https://mercurial.dev/l3l0/composer"),
                working_dir_str.as_bstr(),
            ]),
            cmd(vec!["hg", "up", "--", "ref"]),
        ],
        true,
        Default::default(),
    );

    let downloader = get_downloader_mock(None, None, process, None);
    run(downloader.install(package, path_from_bytes(&working_dir_str))).unwrap();
}

#[test]
fn test_updatefor_package_without_source_reference() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let _ = &working_dir;

    let initial_package = get_package(Some(BStr::new("ref")), None);
    let source_package = get_package(None, None);

    let (process, _guard) = get_process_executor_mock(vec![], false, Default::default());
    let downloader = get_downloader_mock(None, None, process, None);

    let result = run(async {
        downloader
            .prepare(
                BStr::new("update"),
                source_package.clone(),
                std::path::Path::new("/path"),
                Some(initial_package.clone()),
            )
            .await?;
        downloader
            .update(
                initial_package.clone(),
                source_package.clone(),
                std::path::Path::new("/path"),
            )
            .await?;
        downloader
            .cleanup(
                BStr::new("update"),
                source_package,
                std::path::Path::new("/path"),
                Some(initial_package),
            )
            .await
    });

    let e = result.expect_err("missing source reference should throw");
    assert!(error_message(&e).contains_str("missing reference information"));
}

#[test]
fn test_update() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());

    let working_dir_str = bformat!("{}", working_dir.path());

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(working_dir.path().join(".hg"))
        .unwrap();

    let package = get_package(
        Some(BStr::new("ref")),
        Some(BStr::new("https://github.com/l3l0/composer")),
    );

    let (process, _guard) = get_process_executor_mock(
        vec![
            cmd(vec!["hg", "st"]),
            cmd(vec!["hg", "pull", "--", "https://github.com/l3l0/composer"]),
            cmd(vec!["hg", "up", "--", "ref"]),
        ],
        true,
        Default::default(),
    );

    let downloader = get_downloader_mock(None, None, process, None);
    run(async {
        downloader
            .prepare(
                BStr::new("update"),
                package.clone(),
                path_from_bytes(&working_dir_str),
                Some(package.clone()),
            )
            .await
            .unwrap();
        downloader
            .update(
                package.clone(),
                package.clone(),
                path_from_bytes(&working_dir_str),
            )
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("update"),
                package.clone(),
                path_from_bytes(&working_dir_str),
                Some(package),
            )
            .await
            .unwrap();
    });
}

#[test]
fn test_remove() {
    let working_dir = set_up();
    let _tear_down = TearDown::new(working_dir.path().to_path_buf());
    let working_dir_str = bformat!("{}", working_dir.path());

    let mut fs = Filesystem::new(None);
    fs.ensure_directory_exists(working_dir.path().join(".hg"))
        .unwrap();

    let package = get_package(None, None);

    let (process, _guard) =
        get_process_executor_mock(vec![cmd(vec!["hg", "st"])], true, Default::default());

    let mut filesystem = Filesystem::new(None);
    filesystem.__set_mock(FilesystemMock {
        remove_directory_async_result: Some(true),
        ..Default::default()
    });
    let filesystem = std::rc::Rc::new(std::cell::RefCell::new(filesystem));

    let downloader = get_downloader_mock(None, None, process, Some(filesystem.clone()));
    run(async {
        downloader
            .prepare(
                BStr::new("uninstall"),
                package.clone(),
                path_from_bytes(&working_dir_str),
                None,
            )
            .await
            .unwrap();
        downloader
            .remove(package.clone(), path_from_bytes(&working_dir_str))
            .await
            .unwrap();
        downloader
            .cleanup(
                BStr::new("uninstall"),
                package,
                path_from_bytes(&working_dir_str),
                None,
            )
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
