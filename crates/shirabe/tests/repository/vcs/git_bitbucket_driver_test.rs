//! ref: composer/tests/Composer/Test/Repository/Vcs/GitBitbucketDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::GitBitbucketDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe_php_shim::PhpMixed;
use tempfile::TempDir;

struct SetUp {
    home: TempDir,
    config: Config,
    // The IOInterface mock and the HttpDownloaderMock are not ported.
    io: (),
    http_downloader: (),
}

fn set_up() -> SetUp {
    // The IOInterface mock is created via PHPUnit's getMockBuilder, which is not ported.
    let io: () = todo!();

    let home = TempDir::new().unwrap();
    let mut config = Config::new(true, None);
    let mut top: IndexMap<String, PhpMixed> = IndexMap::new();
    let mut config_section: IndexMap<String, PhpMixed> = IndexMap::new();
    config_section.insert(
        "home".to_string(),
        PhpMixed::String(home.path().to_string_lossy().into_owned()),
    );
    top.insert("config".to_string(), PhpMixed::Array(config_section));
    config.merge(&top, Config::SOURCE_UNKNOWN);

    // The HttpDownloaderMock is created via getHttpDownloaderMock, which is not ported.
    let http_downloader: () = todo!();

    SetUp {
        home,
        config,
        io,
        http_downloader,
    }
}

fn tear_down(home: &std::path::Path) {
    let mut fs = Filesystem::new(None);
    fs.remove_directory(home).unwrap();
}

struct TearDown {
    home: std::path::PathBuf,
}

impl TearDown {
    fn new(home: std::path::PathBuf) -> Self {
        TearDown { home }
    }
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.home);
    }
}

#[test]
fn test_supports() {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let config = Rc::new(RefCell::new(Config::new(true, None)));

    assert!(
        GitBitbucketDriver::supports(
            io.clone(),
            config.clone(),
            "https://bitbucket.org/user/repo.git",
            false
        )
        .unwrap()
    );

    // should not be changed, see https://github.com/composer/composer/issues/9400
    assert!(
        !GitBitbucketDriver::supports(
            io.clone(),
            config.clone(),
            "git@bitbucket.org:user/repo.git",
            false
        )
        .unwrap()
    );

    assert!(
        !GitBitbucketDriver::supports(io, config, "https://github.com/user/repo.git", false)
            .unwrap()
    );
}

// The remaining cases construct a GitBitbucketDriver and mock the HttpDownloader to return
// Bitbucket API responses; mocking is not available, and a real HttpDownloader reaches
// curl_multi_init (todo!()).
#[test]
#[ignore = "needs IOInterface mock (getMockBuilder) and getHttpDownloaderMock/HttpDownloaderMock (not ported); set_up()'s io and http_downloader are todo!() and real HttpDownloader hits todo!() curl I/O"]
fn test_get_root_identifier_wrong_scm_type() {
    let SetUp {
        home,
        config: _,
        io: _,
        http_downloader: _,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "needs IOInterface mock (getMockBuilder) and getHttpDownloaderMock/HttpDownloaderMock (not ported); set_up()'s io and http_downloader are todo!() and real HttpDownloader hits todo!() curl I/O"]
fn test_driver() {
    let SetUp {
        home,
        config: _,
        io: _,
        http_downloader: _,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "needs IOInterface mock (getMockBuilder) and getHttpDownloaderMock/HttpDownloaderMock (not ported); depends on test_driver's driver and set_up()'s todo!() mocks"]
fn test_get_params() {
    let SetUp {
        home,
        config: _,
        io: _,
        http_downloader: _,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "needs IOInterface mock (getMockBuilder) and getHttpDownloaderMock/HttpDownloaderMock (not ported); set_up()'s io and http_downloader are todo!()"]
fn test_initialize_invalid_repository_url() {
    let SetUp {
        home,
        config: _,
        io: _,
        http_downloader: _,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}

#[test]
#[ignore = "needs IOInterface mock (getMockBuilder) and getHttpDownloaderMock/HttpDownloaderMock (not ported); set_up()'s io and http_downloader are todo!() and real HttpDownloader hits todo!() curl I/O"]
fn test_invalid_support_data() {
    let SetUp {
        home,
        config: _,
        io: _,
        http_downloader: _,
    } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());
    todo!()
}
