//! ref: composer/tests/Composer/Test/Advisory/AuditorTest.php

// These run the Auditor against a mocked HttpDownloader/IO and packages built from version
// constraints (parsed via a look-around regex the regex crate cannot compile).
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "not yet ported (Auditor with mocked HttpDownloader/IO; constraint parsing uses a look-around regex)"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_audit);
stub!(test_audit_with_ignore);
stub!(test_audit_with_ignore_unreachable);
stub!(test_audit_with_ignore_severity);
stub!(test_needs_complete_advisory_load);
