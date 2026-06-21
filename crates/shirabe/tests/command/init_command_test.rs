//! ref: composer/tests/Composer/Test/Command/InitCommandTest.php

// The author/namespace/git-config helpers are protected methods exercised via reflection
// in PHP; the run cases need the ApplicationTester. Neither is available here.

use shirabe_php_shim::server_set;

fn set_up() {
    server_set("COMPOSER_DEFAULT_AUTHOR", "John Smith".to_string());
    server_set("COMPOSER_DEFAULT_EMAIL", "john@example.com".to_string());
}

macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "needs the ApplicationTester harness or reflection into protected InitCommand helpers"]
        fn $name() {
            set_up();

            todo!()
        }
    };
}

stub!(test_parse_valid_author_string);
stub!(test_parse_empty_author_string);
stub!(test_parse_author_string_with_invalid_email);
stub!(test_namespace_from_valid_package_name);
stub!(test_namespace_from_invalid_package_name);
stub!(test_namespace_from_missing_package_name);
stub!(test_run_command);
stub!(test_run_command_invalid);
stub!(test_run_guess_name_from_dir_sanitizes_dir);
stub!(test_interactive_run);
stub!(test_format_authors);
stub!(test_get_git_config);
stub!(test_add_vendor_ignore);
stub!(test_has_vendor_ignore);
