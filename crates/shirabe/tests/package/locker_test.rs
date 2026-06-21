//! ref: composer/tests/Composer/Test/Package/LockerTest.php

// These construct a Locker with a mocked JsonFile/InstallationManager/repository and a
// mocked ProcessExecutor to drive lock read/write and freshness checks; mocking is not
// available here.
#[test]
#[ignore = "requires PHPUnit mock of JsonFile (exists/read) which is not available"]
fn test_is_locked() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of JsonFile (exists) which is not available"]
fn test_get_not_locked_packages() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of JsonFile (exists/read) which is not available"]
fn test_get_locked_packages() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of JsonFile (write) which is not available"]
fn test_set_lock_data() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of JsonFile and PackageInterface (createPackageMock) which is not available"]
fn test_lock_bad_packages() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of JsonFile (read) which is not available"]
fn test_is_fresh() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of JsonFile (read) which is not available"]
fn test_is_fresh_false() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of JsonFile (read) which is not available"]
fn test_is_fresh_with_content_hash() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of JsonFile (read) which is not available"]
fn test_is_fresh_with_content_hash_and_no_hash() {
    todo!()
}

#[test]
#[ignore = "requires PHPUnit mock of JsonFile (read) which is not available"]
fn test_is_fresh_false_with_content_hash() {
    todo!()
}
