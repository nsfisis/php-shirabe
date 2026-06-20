//! ref: composer/tests/Composer/Test/Repository/Vcs/GitHubDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::GitHubDriver;

fn supports_provider() -> Vec<(bool, &'static str)> {
    vec![
        (false, "https://github.com/acme"),
        (true, "https://github.com/acme/repository"),
        (true, "git@github.com:acme/repository.git"),
        (false, "https://github.com/acme/repository/releases"),
        (false, "https://github.com/acme/repository/pulls"),
    ]
}

#[test]
#[ignore = "GitHubDriver::supports reaches non-strict in_array, which is todo!() in the php-shim"]
fn test_supports() {
    for (expected, repo_url) in supports_provider() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(true, None)));

        assert_eq!(
            expected,
            GitHubDriver::supports(io, config, repo_url, false).unwrap()
        );
    }
}

// The remaining cases construct a GitHubDriver and mock the HttpDownloader/IO to return
// GitHub API responses; mocking is not available, and a real HttpDownloader reaches
// curl_multi_init (todo!()).
#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_private_repository() {
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_public_repository() {
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_public_repository2() {
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_invalid_support_data() {
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_funding_format() {
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_public_repository_archived() {
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_private_repository_no_interaction() {
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_initialize_invalid_repo_url() {
    todo!()
}

#[test]
#[ignore = "constructs a GitHubDriver and mocks the HttpDownloader/IO (curl_multi_init todo!())"]
fn test_get_empty_file_content() {
    todo!()
}
