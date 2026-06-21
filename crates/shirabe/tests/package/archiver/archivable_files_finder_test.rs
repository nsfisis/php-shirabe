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
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "needs a temp directory tree and git-backed fixtures to drive ArchivableFilesFinder; not ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_manual_excludes);
stub!(test_git_excludes);
stub!(test_skip_excludes);
