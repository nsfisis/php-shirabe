//! ref: composer/tests/Composer/Test/Repository/Vcs/SvnDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::SvnDriver;

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

        assert_eq!(assertion, SvnDriver::supports(io, config, url, false).unwrap());
    }
}

// Constructs an SvnDriver and runs an svn command via a mocked ProcessExecutor; mocking is
// not available here.
#[test]
#[ignore = "constructs an SvnDriver and mocks a ProcessExecutor for the svn invocation"]
fn test_wrong_credentials_in_url() {
    todo!()
}
