//! ref: composer/tests/Composer/Test/Util/GitLabTest.php

// Both cases construct GitLab with a mocked IO/Config/JsonConfigSource and a mocked
// HttpDownloader to drive the username/password authentication flow. Mocking is not
// available, and a real HttpDownloader reaches curl_multi_init (todo!()).

#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config/JsonConfigSource (no mock infrastructure)"]
#[test]
fn test_username_password_authentication_flow() {
    todo!()
}

#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config/JsonConfigSource (no mock infrastructure)"]
#[test]
fn test_username_password_failure() {
    todo!()
}
