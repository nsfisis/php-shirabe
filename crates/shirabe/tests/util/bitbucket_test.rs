//! ref: composer/tests/Composer/Test/Util/BitbucketTest.php

// These mock IO/Config/HttpDownloader to drive Bitbucket's OAuth/access-token flow; mocking
// is not available and a real HttpDownloader reaches curl_multi_init (todo!()).

#[allow(dead_code)]
fn set_up() {
    // Builds mocked IO/HttpDownloader/Config and records time(); mocking is not available.
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config (no mock infrastructure)"]
fn test_request_access_token_with_valid_oauth_consumer() {
    todo!()
}

#[test]
#[ignore = "needs getMockBuilder mock for Config (no mock infrastructure)"]
fn test_request_access_token_with_valid_oauth_consumer_and_valid_stored_access_token() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config (no mock infrastructure)"]
fn test_request_access_token_with_valid_oauth_consumer_and_expired_access_token() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config (no mock infrastructure)"]
fn test_request_access_token_with_username_and_password() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config (no mock infrastructure)"]
fn test_request_access_token_with_username_and_password_with_unauthorized_response() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config (no mock infrastructure)"]
fn test_request_access_token_with_username_and_password_with_not_found_response() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config (no mock infrastructure)"]
fn test_username_password_authentication_flow() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mock for Config (no mock infrastructure)"]
fn test_authorize_oauth_interactively_with_empty_username() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mock for Config (no mock infrastructure)"]
fn test_authorize_oauth_interactively_with_empty_password() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config (no mock infrastructure)"]
fn test_authorize_oauth_interactively_with_request_access_token_failure() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config to construct Bitbucket (no mock infrastructure)"]
fn test_get_token_without_access_token() {
    todo!()
}

#[test]
#[ignore = "needs getMockBuilder mock for Config and @depends-injected Bitbucket from a mock-based test (no mock infrastructure)"]
fn test_get_token_with_access_token() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock and getMockBuilder mocks for HttpDownloader/Config to construct Bitbucket (no mock infrastructure)"]
fn test_authorize_oauth_with_wrong_origin_url() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock, getProcessExecutorMock and getMockBuilder mocks for HttpDownloader/Config (no mock infrastructure)"]
fn test_authorize_oauth_without_available_git_config_token() {
    todo!()
}

#[test]
#[ignore = "needs getIOMock/IOMock, getProcessExecutorMock and getMockBuilder mocks for HttpDownloader/Config (no mock infrastructure)"]
fn test_authorize_oauth_with_available_git_config_token() {
    todo!()
}
