//! ref: composer/tests/Composer/Test/Repository/Vcs/PerforceDriverTest.php

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::config::Config;
use shirabe::io::IOInterface;
use shirabe::io::null_io::NullIO;
use shirabe::repository::vcs::PerforceDriver;

#[test]
fn test_supports_returns_false_no_deep_check() {
    let io: Rc<RefCell<dyn IOInterface>> = Rc::new(RefCell::new(NullIO::new()));
    let config = Rc::new(RefCell::new(Config::new(true, None)));

    assert!(!PerforceDriver::supports(io, config, "existing.url", false).unwrap());
}

// The remaining cases mock Perforce, the repository config and IO to drive initialization,
// composer-file detection and cleanup; mocking is not available here.
#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_initialize_captures_variables_from_repo_config() {
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_initialize_logs_in_and_connects_client() {
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_has_composer_file_returns_false_on_no_composer_file() {
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_has_composer_file_returns_true_with_one_or_more_composer_files() {
    todo!()
}

#[test]
#[ignore = "mocks Perforce/repository/IO; mocking is not available"]
fn test_cleanup() {
    todo!()
}
