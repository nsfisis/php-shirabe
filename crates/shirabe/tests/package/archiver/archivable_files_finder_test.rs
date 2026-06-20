//! ref: composer/tests/Composer/Test/Package/Archiver/ArchivableFilesFinderTest.php

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
