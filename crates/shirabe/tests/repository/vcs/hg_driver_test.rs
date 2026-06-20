//! ref: composer/tests/Composer/Test/Repository/Vcs/HgDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::HgDriver;

fn supports_data_provider() -> Vec<&'static str> {
    vec![
        "ssh://bitbucket.org/user/repo",
        "ssh://hg@bitbucket.org/user/repo",
        "ssh://user@bitbucket.org/user/repo",
        "https://bitbucket.org/user/repo",
        "https://user@bitbucket.org/user/repo",
    ]
}

#[test]
fn test_supports() {
    for repository_url in supports_data_provider() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(true, None)));

        assert!(HgDriver::supports(io, config, repository_url, false).unwrap());
    }
}

// The remaining cases construct an HgDriver, which requires an HttpDownloader
// (curl_multi_init is todo!() in the php-shim) and a mocked ProcessExecutor to feed
// hg command output, neither of which is available here.
#[test]
#[ignore = "needs an HgDriver instance (HttpDownloader reaches curl_multi_init, todo!()) and a mocked ProcessExecutor"]
fn test_get_branches_filter_invalid_branch_names() {
    todo!()
}

#[test]
#[ignore = "needs an HgDriver instance (HttpDownloader reaches curl_multi_init, todo!()) and a mocked ProcessExecutor"]
fn test_file_get_content_invalid_identifier() {
    todo!()
}

#[test]
#[ignore = "needs an HgDriver instance (HttpDownloader reaches curl_multi_init, todo!()) and a mocked ProcessExecutor"]
fn test_get_change_date_invalid_identifier() {
    todo!()
}
