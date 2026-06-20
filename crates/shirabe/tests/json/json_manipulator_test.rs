//! ref: composer/tests/Composer/Test/Json/JsonManipulatorTest.php

// JsonManipulator's text-rewriting operations reach addcslashes, which is todo!() in the
// php-shim, so none of these cases can run yet.

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "JsonManipulator operations reach addcslashes (todo!()) in the php-shim"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_add_link);
stub!(test_add_link_and_sort_packages);
stub!(test_remove_sub_node);
stub!(test_remove_sub_node_from_require);
stub!(test_remove_sub_node_preserves_object_type_when_empty);
stub!(test_remove_sub_node_preserves_object_type_when_empty2);
stub!(test_add_sub_node_in_require);
stub!(test_add_extra_with_package);
stub!(test_add_config_with_package);
stub!(test_add_suggest_with_package);
stub!(test_add_repository_can_initialize_empty_repositories);
stub!(test_add_repository_can_initialize_from_scratch);
stub!(test_add_repository_can_append);
stub!(test_add_repository_can_prepend);
stub!(test_add_repository);
stub!(test_add_repository_can_override_deep_repos);
stub!(test_set_url_in_repository);
stub!(test_insert_repository_before_and_after_by_name);
stub!(test_remove_repository_removes_from_assoc_but_does_not_converts_from_assoc_to_list);
stub!(test_remove_repository_removes_from_list);
stub!(test_add_repository_converts_from_assoc_to_list);
stub!(test_add_config_setting_escapes);
stub!(test_add_config_setting_works_from_scratch);
stub!(test_add_config_setting_can_add);
stub!(test_add_config_setting_can_overwrite);
stub!(test_add_config_setting_can_overwrite_numbers);
stub!(test_add_config_setting_can_overwrite_arrays);
stub!(test_add_config_setting_can_add_sub_key_in_empty_config);
stub!(test_add_config_setting_can_add_sub_key_in_empty_val);
stub!(test_add_config_setting_can_add_sub_key_in_hash);
stub!(test_add_root_setting_does_not_break_dots);
stub!(test_remove_config_setting_can_remove_sub_key_in_hash);
stub!(test_remove_config_setting_can_remove_sub_key_in_hash_with_siblings);
stub!(test_add_main_key);
stub!(test_add_main_key_with_content_having_dollar_sign_followed_by_digit);
stub!(test_add_main_key_with_content_having_dollar_sign_followed_by_digit2);
stub!(test_update_main_key);
stub!(test_update_main_key2);
stub!(test_update_main_key3);
stub!(test_update_main_key_with_content_having_dollar_sign_followed_by_digit);
stub!(test_remove_main_key);
stub!(test_remove_main_key_if_empty);
stub!(test_remove_main_key_removes_key_where_value_is_null);
stub!(test_indent_detection);
stub!(test_remove_main_key_at_end_of_file);
stub!(test_add_list_item);
stub!(test_remove_list_item);
stub!(test_insert_list_item);
stub!(test_escaped_unicode_does_not_cause_backtrack_limit_error_github_issue8131);
stub!(test_large_file_does_not_cause_backtrack_limit_error_github_issue9595);
