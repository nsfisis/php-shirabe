//! ref: composer/tests/Composer/Test/Package/LockerTest.php

// These construct a Locker with a mocked JsonFile/InstallationManager/repository and a
// mocked ProcessExecutor to drive lock read/write and freshness checks; mocking is not
// available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks JsonFile/InstallationManager/repository/ProcessExecutor to drive Locker; mocking is not available"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_is_locked);
stub!(test_get_not_locked_packages);
stub!(test_get_locked_packages);
stub!(test_set_lock_data);
stub!(test_lock_bad_packages);
stub!(test_is_fresh);
stub!(test_is_fresh_false);
stub!(test_is_fresh_with_content_hash);
stub!(test_is_fresh_with_content_hash_and_no_hash);
stub!(test_is_fresh_false_with_content_hash);
