//! ref: composer/tests/Composer/Test/Util/BitbucketTest.php

// These mock IO/Config/HttpDownloader to drive Bitbucket's OAuth/access-token flow; mocking
// is not available and a real HttpDownloader reaches curl_multi_init (todo!()).

#[allow(dead_code)]
fn set_up() {
    // Builds mocked IO/HttpDownloader/Config and records time(); mocking is not available.
    todo!()
}

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO/Config/HttpDownloader (curl_multi_init todo!()) for the Bitbucket OAuth flow"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_request_access_token_with_valid_oauth_consumer);
stub!(test_request_access_token_with_valid_oauth_consumer_and_valid_stored_access_token);
stub!(test_request_access_token_with_valid_oauth_consumer_and_expired_access_token);
stub!(test_request_access_token_with_username_and_password);
stub!(test_request_access_token_with_username_and_password_with_unauthorized_response);
stub!(test_request_access_token_with_username_and_password_with_not_found_response);
stub!(test_username_password_authentication_flow);
stub!(test_authorize_oauth_interactively_with_empty_username);
stub!(test_authorize_oauth_interactively_with_empty_password);
stub!(test_authorize_oauth_interactively_with_request_access_token_failure);
stub!(test_get_token_without_access_token);
stub!(test_get_token_with_access_token);
stub!(test_authorize_oauth_with_wrong_origin_url);
stub!(test_authorize_oauth_without_available_git_config_token);
stub!(test_authorize_oauth_with_available_git_config_token);
