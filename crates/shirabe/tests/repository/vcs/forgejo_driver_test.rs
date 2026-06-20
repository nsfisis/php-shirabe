//! ref: composer/tests/Composer/Test/Repository/Vcs/ForgejoDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::ForgejoDriver;

fn supports_provider() -> Vec<(bool, &'static str)> {
    vec![
        (false, "https://example.org/acme/repo"),
        (true, "https://codeberg.org/acme/repository"),
    ]
}

#[test]
#[ignore = "ForgejoDriver::supports uses a regex with a character class the regex crate cannot compile (unclosed character class)"]
fn test_supports() {
    for (expected, repo_url) in supports_provider() {
        let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
        let config = Rc::new(RefCell::new(Config::new(true, None)));

        assert_eq!(
            expected,
            ForgejoDriver::supports(io, config, repo_url, false).unwrap()
        );
    }
}

// The remaining cases construct a ForgejoDriver and mock the HttpDownloader to return
// Forgejo API responses; mocking is not available, and a real HttpDownloader reaches
// curl_multi_init (todo!()).
#[test]
#[ignore = "constructs a ForgejoDriver and mocks the HttpDownloader (curl_multi_init todo!())"]
fn test_public_repository() {
    todo!()
}

#[test]
#[ignore = "constructs a ForgejoDriver and mocks the HttpDownloader (curl_multi_init todo!())"]
fn test_get_branches() {
    todo!()
}

#[test]
#[ignore = "constructs a ForgejoDriver and mocks the HttpDownloader (curl_multi_init todo!())"]
fn test_get_tags() {
    todo!()
}

#[test]
#[ignore = "constructs a ForgejoDriver and mocks the HttpDownloader (curl_multi_init todo!())"]
fn test_get_empty_file_content() {
    todo!()
}
