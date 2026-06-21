//! ref: composer/tests/Composer/Test/Package/Loader/ArrayLoaderTest.php

use shirabe::package::loader::ArrayLoader;

fn set_up() -> ArrayLoader {
    ArrayLoader::new(None, false)
}

// ArrayLoader::load parses version/link constraints through a look-around regex the regex
// crate cannot compile.
#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_self_version() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_type_default() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_normalized_version_optimization() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_parse_dump_default_load_config() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_parse_dump_true_load_config() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_parse_dump_false_load_config() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_package_with_branch_alias() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_package_aliasing_without_branch_alias() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_abandoned() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_not_abandoned() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_plugin_api_version_are_kept_as_declared() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_plugin_api_version_does_support_self_version() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_parse_links_integer_target() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_parse_links_invalid_version() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_none_string_version() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_none_string_source_dist_reference() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_branch_alias_integer_index() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_package_links_require() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_package_links_require_invalid() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_package_links_replace() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_package_links_replace_invalid() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_support_string_value() {
    let _loader = set_up();
    todo!()
}

#[test]
#[ignore = "ArrayLoader::load parses constraints via a look-around regex the regex crate cannot compile"]
fn test_invalid_version() {
    let _loader = set_up();
    todo!()
}
