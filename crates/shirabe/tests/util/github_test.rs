//! ref: composer/tests/Composer/Test/Util/GitHubTest.php

// Both cases construct GitHub with a mocked IO/Config/JsonConfigSource and a mocked
// HttpDownloader to drive the username/password authentication flow. Mocking is not
// available, and a real HttpDownloader reaches curl_multi_init (todo!()).

#[test]
#[ignore = "mocks IO/Config/HttpDownloader for the auth flow; a real HttpDownloader reaches curl_multi_init (todo!())"]
fn test_username_password_authentication_flow() {
    todo!()
}

#[test]
#[ignore = "mocks IO/Config/HttpDownloader for the auth flow; a real HttpDownloader reaches curl_multi_init (todo!())"]
fn test_username_password_failure() {
    todo!()
}
