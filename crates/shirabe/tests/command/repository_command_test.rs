//! ref: composer/tests/Composer/Test/Command/RepositoryCommandTest.php

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "requires the ApplicationTester/initTempComposer harness, which is not yet ported"]
        fn $name() {
            todo!()
        }
    };
}

stub!(test_list_with_no_repositories);
stub!(test_list_with_repositories_as_list);
stub!(test_list_with_repositories_as_assoc);
stub!(test_add_repository_with_type_and_url);
stub!(test_add_repository_with_json);
stub!(test_remove_repository);
stub!(test_set_and_get_url_in_repository_assoc);
stub!(test_set_and_get_url_in_repository_list);
stub!(test_disable_and_enable_packagist);
stub!(test_invalid_arg_combination_throws);
stub!(test_prepend_repository_by_name_list_to_assoc);
stub!(test_append_repository_by_name_list_to_assoc);
stub!(test_prepend_repository_assoc_with_packagist_disabled);
stub!(test_append_repository_assoc_with_packagist_disabled);
stub!(test_add_before_and_after_by_name);
stub!(test_add_same_name_replaces_existing);
