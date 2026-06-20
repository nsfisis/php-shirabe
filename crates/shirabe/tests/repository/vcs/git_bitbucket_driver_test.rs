//! ref: composer/tests/Composer/Test/Repository/Vcs/GitBitbucketDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::GitBitbucketDriver;

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
#[ignore = "constructs a GitBitbucketDriver and mocks the HttpDownloader (curl_multi_init todo!())"]
fn test_get_root_identifier_wrong_scm_type() {
    todo!()
}

#[test]
#[ignore = "constructs a GitBitbucketDriver and mocks the HttpDownloader (curl_multi_init todo!())"]
fn test_driver() {
    todo!()
}

#[test]
#[ignore = "constructs a GitBitbucketDriver and mocks the HttpDownloader (curl_multi_init todo!())"]
fn test_get_params() {
    todo!()
}

#[test]
#[ignore = "constructs a GitBitbucketDriver and mocks the HttpDownloader (curl_multi_init todo!())"]
fn test_initialize_invalid_repository_url() {
    todo!()
}

#[test]
#[ignore = "constructs a GitBitbucketDriver and mocks the HttpDownloader (curl_multi_init todo!())"]
fn test_invalid_support_data() {
    todo!()
}
