//! ref: composer/tests/Composer/Test/DependencyResolver/SecurityAdvisoryPoolFilterTest.php

// The filter parses each advisory's affectedVersions (e.g. ">=1.0.0,<1.1.0") into a
// constraint, which goes through the version parser's look-around regex that the regex
// crate cannot compile. The fixtures also build PackageRepository security-advisory data
// and run the Auditor.

#[test]
#[ignore = "filtering parses affectedVersions via a look-around regex the regex crate cannot compile"]
fn test_filter_packages_by_advisories() {
    todo!()
}

#[test]
#[ignore = "filtering parses affectedVersions via a look-around regex the regex crate cannot compile"]
fn test_dont_filter_packages_by_ignored_advisories() {
    todo!()
}

#[test]
#[ignore = "filtering parses affectedVersions via a look-around regex the regex crate cannot compile"]
fn test_dont_filter_packages_with_block_insecure_disabled() {
    todo!()
}

#[test]
#[ignore = "filtering parses affectedVersions via a look-around regex the regex crate cannot compile"]
fn test_dont_filter_packages_with_abandoned_package() {
    todo!()
}
