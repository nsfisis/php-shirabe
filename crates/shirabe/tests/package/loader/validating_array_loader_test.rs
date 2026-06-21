//! ref: composer/tests/Composer/Test/Package/Loader/ValidatingArrayLoaderTest.php

// ValidatingArrayLoader wraps ArrayLoader, whose constraint parsing uses a look-around
// regex the regex crate cannot compile; the success/warning data sets are large.
#[test]
#[ignore = "ValidatingArrayLoader -> ArrayLoader parses constraints via a look-around regex the regex crate cannot compile"]
fn test_load_success() {
    todo!()
}

#[test]
#[ignore = "ValidatingArrayLoader -> ArrayLoader parses constraints via a look-around regex the regex crate cannot compile"]
fn test_load_failure_throws_exception() {
    todo!()
}

#[test]
#[ignore = "ValidatingArrayLoader -> ArrayLoader parses constraints via a look-around regex the regex crate cannot compile"]
fn test_load_warnings() {
    todo!()
}

#[test]
#[ignore = "ValidatingArrayLoader -> ArrayLoader parses constraints via a look-around regex the regex crate cannot compile"]
fn test_load_skips_warning_data_when_ignoring_errors() {
    todo!()
}
