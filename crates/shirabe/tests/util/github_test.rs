//! ref: composer/tests/Composer/Test/Util/GitHubTest.php

// Both cases construct GitHub with a mocked IO/Config/JsonConfigSource and a mocked
// HttpDownloader to drive the username/password authentication flow. Mocking is not
// available, and a real HttpDownloader reaches curl_multi_init (todo!()).

#[test]
#[ignore = "requires getIOMock/getHttpDownloaderMock and getMockBuilder mocks of Config/JsonConfigSource with expects()/willReturn(); no mocking infrastructure exists"]
fn test_username_password_authentication_flow() {
    todo!()
}

#[test]
#[ignore = "requires getIOMock/getHttpDownloaderMock and getMockBuilder mocks of Config/JsonConfigSource with expects()/willReturn(); no mocking infrastructure exists"]
fn test_username_password_failure() {
    todo!()
}
