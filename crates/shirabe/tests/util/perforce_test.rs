//! ref: composer/tests/Composer/Test/Util/PerforceTest.php

// These mock IO and a ProcessExecutor to drive Perforce client/stream/command behaviour;
// mocking is not available here.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "mocks IO/ProcessExecutor to drive Perforce; mocking is not available"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_get_client_without_stream);
stub!(test_get_client_from_stream);
stub!(test_get_stream_without_stream);
stub!(test_get_stream_with_stream);
stub!(test_get_stream_without_label_with_stream_without_label);
stub!(test_get_stream_without_label_with_stream_with_label);
stub!(test_get_client_spec);
stub!(test_generate_p4_command);
stub!(test_query_p4_user_with_user_already_set);
stub!(test_query_p4_user_with_user_set_in_p4_variables_with_windows_os);
stub!(test_query_p4_user_with_user_set_in_p4_variables_not_windows_os);
stub!(test_query_p4_user_queries_for_user);
stub!(test_query_p4_user_stores_response_to_query_for_user_with_windows);
stub!(test_query_p4_user_stores_response_to_query_for_user_without_windows);
stub!(test_query_p4_user_escapes_injection_on_windows);
stub!(test_query_p4_user_escapes_injection_on_unix);
stub!(test_query_p4_password_with_password_already_set);
stub!(test_query_p4_password_with_password_set_in_p4_variables_with_windows_os);
stub!(test_query_p4_password_with_password_set_in_p4_variables_not_windows_os);
stub!(test_query_p4_password_queries_for_password);
stub!(test_write_p4_client_spec_without_stream);
stub!(test_write_p4_client_spec_with_stream);
stub!(test_is_logged_in);
stub!(test_get_branches_with_stream);
stub!(test_get_branches_without_stream);
stub!(test_get_tags_without_stream);
stub!(test_get_tags_with_stream);
stub!(test_check_stream_without_stream);
stub!(test_check_stream_with_stream);
stub!(test_get_composer_information_without_label_without_stream);
stub!(test_get_composer_information_with_label_without_stream);
stub!(test_get_composer_information_without_label_with_stream);
stub!(test_get_composer_information_with_label_with_stream);
stub!(test_sync_code_base_without_stream);
stub!(test_sync_code_base_with_stream);
stub!(test_check_server_exists);
stub!(test_check_server_client_error);
stub!(test_cleanup_client_spec_should_delete_client);
