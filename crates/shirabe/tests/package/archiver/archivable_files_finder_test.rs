//! ref: composer/tests/Composer/Test/Package/Archiver/ArchivableFilesFinderTest.php

/// Builds a temp directory tree of fixture files under a unique tmp dir; the Filesystem and
/// getUniqueTmpDirectory infrastructure is not ported.
#[allow(dead_code)]
fn set_up() -> String {
    todo!()
}

#[allow(dead_code)]
fn tear_down(_sources: &str) {
    // Removes the temp directory tree created in set_up.
    todo!()
}

#[allow(dead_code)]
struct TearDown {
    sources: String,
}

impl Drop for TearDown {
    fn drop(&mut self) {
        tear_down(&self.sources);
    }
}

// These set up a temp directory tree (including a git repo) and assert the files the finder
// selects with manual/git/skip excludes; the git-backed fixture setup is not ported.
#[ignore = "setUp needs TestCase::getUniqueTmpDirectory to build the on-disk fixture tree; not ported"]
#[test]
fn test_manual_excludes() {
    todo!()
}

#[ignore = "setUp needs TestCase::getUniqueTmpDirectory plus skipIfNotExecutable/Process::fromShellCommandline/PharData/RecursiveIteratorIterator; none ported"]
#[test]
fn test_git_excludes() {
    todo!()
}

#[ignore = "setUp needs TestCase::getUniqueTmpDirectory to build the on-disk fixture tree; not ported"]
#[test]
fn test_skip_excludes() {
    todo!()
}
