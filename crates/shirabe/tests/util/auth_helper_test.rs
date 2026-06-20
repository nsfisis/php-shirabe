//! ref: composer/tests/Composer/Test/Util/AuthHelperTest.php

// These mock IO/Config to drive AuthHelper's header/option building and interactive auth
// storage; mocking is not available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO/Config to drive AuthHelper; mocking is not available"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_add_authentication_header_without_auth_credentials);
stub!(test_add_authentication_header_with_bearer_password);
stub!(test_add_authentication_header_with_github_token);
stub!(test_add_authentication_header_with_gitlab_oath_token);
stub!(test_add_authentication_options_for_client_certificate);
stub!(test_add_authentication_header_with_gitlab_private_token);
stub!(test_add_authentication_header_with_bitbucket_oath_token);
stub!(test_add_authentication_header_with_bitbucket_public_url);
stub!(test_add_authentication_header_with_basic_http_authentication);
stub!(test_add_authentication_header_with_custom_headers);
stub!(test_is_public_bit_bucket_download_with_bitbucket_public_url);
stub!(test_is_public_bit_bucket_download_with_non_bitbucket_public_url);
stub!(test_store_auth_automatically);
stub!(test_store_auth_with_prompt_yes_answer);
stub!(test_store_auth_with_prompt_no_answer);
stub!(test_store_auth_with_prompt_invalid_answer);
stub!(test_prompt_auth_if_needed_git_lab_no_auth_change);
stub!(test_prompt_auth_if_needed_multiple_bitbucket_downloads);
stub!(test_add_authentication_header_is_working);
stub!(test_add_authentication_header_deprecation);
