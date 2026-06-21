//! ref: composer/tests/Composer/Test/Package/Loader/ArrayLoaderTest.php

use shirabe::package::loader::ArrayLoader;

fn set_up() -> ArrayLoader {
    ArrayLoader::new(None, false)
}

// ArrayLoader::load parses version/link constraints through a look-around regex the regex
// crate cannot compile.
macro_rules! stub {
    ($name:ident) => {
        #[test]
        #[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
        fn $name() {
            let _loader = set_up();
            todo!()
        }
    };
}

stub!(test_self_version);
stub!(test_type_default);
stub!(test_normalized_version_optimization);
stub!(test_parse_dump_default_load_config);
stub!(test_parse_dump_true_load_config);
stub!(test_parse_dump_false_load_config);
stub!(test_package_with_branch_alias);
stub!(test_package_aliasing_without_branch_alias);
stub!(test_abandoned);
stub!(test_not_abandoned);
stub!(test_plugin_api_version_are_kept_as_declared);
stub!(test_plugin_api_version_does_support_self_version);
stub!(test_parse_links_integer_target);
stub!(test_parse_links_invalid_version);
stub!(test_none_string_version);
stub!(test_none_string_source_dist_reference);
stub!(test_branch_alias_integer_index);
stub!(test_package_links_require);
stub!(test_package_links_require_invalid);
stub!(test_package_links_replace);
stub!(test_package_links_replace_invalid);
stub!(test_support_string_value);
stub!(test_invalid_version);
