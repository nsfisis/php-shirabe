//! ref: composer/tests/Composer/Test/Repository/FilesystemRepositoryTest.php

// These read/write installed.json and installed.php fixtures via JsonFile and assert the
// generated output; the file IO/manipulation (JsonManipulator reaches addcslashes, todo!())
// and fixtures are not ported.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "reads/writes installed.json/installed.php fixtures via JsonFile/JsonManipulator (addcslashes todo!())"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_repository_read);
stub!(test_corrupted_repository_file);
stub!(test_unexistent_repository_file);
stub!(test_repository_write);
stub!(test_repository_writes_installed_php);
stub!(test_safely_load_installed_versions);
