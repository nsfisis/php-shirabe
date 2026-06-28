//! ref: composer/tests/Composer/Test/Repository/Vcs/SvnDriverTest.php

use crate::http_downloader_mock::{HttpDownloaderMockGuard, get_http_downloader_mock};
use crate::io_stub::IOStub;
use crate::process_executor_mock::{ProcessExecutorMockGuard, cmd_full, get_process_executor_mock};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::SvnDriver;
use shirabe::util::filesystem::Filesystem;
use shirabe::util::http_downloader::HttpDownloaderMockHandler;
use shirabe::util::process_executor::MockHandler;
use shirabe_php_shim::{PhpMixed, RuntimeException};
use std::cell::RefCell;
use std::rc::Rc;
use tempfile::TempDir;

struct SetUp {
    home: TempDir,
    config: Config,
}

fn set_up() -> SetUp {
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

    SetUp { home, config }
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

fn support_provider() -> Vec<(&'static str, bool)> {
    vec![
        ("http://svn.apache.org", true),
        ("https://svn.sf.net", true),
        ("svn://example.org", true),
        ("svn+ssh://example.org", true),
    ]
}

#[test]
fn test_support() {
    for (url, assertion) in support_provider() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(true, None)));

        assert_eq!(
            assertion,
            SvnDriver::supports(io, config, url, false).unwrap()
        );
    }
}

#[test]
fn test_wrong_credentials_in_url() {
    let SetUp { home, config } = set_up();
    let _tear_down = TearDown::new(home.path().to_path_buf());

    let config = Rc::new(RefCell::new(config));
    let console: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(IOStub::new()));

    let (http_downloader, _http_guard): (_, HttpDownloaderMockGuard) =
        get_http_downloader_mock(vec![], false, HttpDownloaderMockHandler::default());

    let mut output = String::from("svn: OPTIONS of 'https://corp.svn.local/repo':");
    output.push_str(" authorization failed: Could not authenticate to server:");
    output.push_str(" rejected Basic challenge (https://corp.svn.local/)");

    let authed_command = [
        "svn",
        "ls",
        "--verbose",
        "--non-interactive",
        "--username",
        "till",
        "--password",
        "secret",
        "--",
        "https://till:secret@corp.svn.local/repo/trunk",
    ];

    let (process, _process_guard): (_, ProcessExecutorMockGuard) = get_process_executor_mock(
        vec![
            cmd_full(authed_command, 1, "", output.clone()),
            cmd_full(authed_command, 1, "", output.clone()),
            cmd_full(authed_command, 1, "", output.clone()),
            cmd_full(authed_command, 1, "", output.clone()),
            cmd_full(authed_command, 1, "", output.clone()),
            cmd_full(authed_command, 1, "", output.clone()),
            cmd_full(["svn", "--version"], 0, "1.2.3", ""),
        ],
        true,
        MockHandler::default(),
    );

    let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
    repo_config.insert(
        "url".to_string(),
        PhpMixed::String("https://till:secret@corp.svn.local/repo".to_string()),
    );

    let mut svn = SvnDriver::new(repo_config, console, config, http_downloader, process);
    let err = svn.initialize().unwrap_err();
    let runtime = err
        .downcast_ref::<RuntimeException>()
        .expect("expected RuntimeException");
    assert_eq!(
        "Repository https://till:secret@corp.svn.local/repo could not be processed, wrong credentials provided (svn: OPTIONS of 'https://corp.svn.local/repo': authorization failed: Could not authenticate to server: rejected Basic challenge (https://corp.svn.local/))",
        runtime.message
    );
}
