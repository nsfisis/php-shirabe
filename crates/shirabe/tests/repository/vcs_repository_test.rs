//! ref: composer/tests/Composer/Test/Repository/VcsRepositoryTest.php

// testLoadVersions initialises a real git repository on disk and drives a VcsRepository over
// it, then asserts the loaded package versions; the git fixture setup and constraint parsing
// (look-around regex) are not ported.
#[test]
#[ignore = "not yet ported (initialises a git repo on disk and loads versions; constraint parsing uses a look-around regex)"]
fn test_load_versions() {
    todo!()
}
