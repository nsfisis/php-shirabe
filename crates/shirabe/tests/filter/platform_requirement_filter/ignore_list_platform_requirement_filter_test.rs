//! ref: composer/tests/Composer/Test/Filter/PlatformRequirementFilter/IgnoreListPlatformRequirementFilterTest.php

use shirabe::filter::platform_requirement_filter::{
    IgnoreListPlatformRequirementFilter, PlatformRequirementFilterInterface,
};

#[test]
fn test_is_ignored() {
    for (req_list, req, expect_ignored) in data_is_ignored() {
        let platform_requirement_filter = IgnoreListPlatformRequirementFilter::new(
            req_list.iter().map(|s| s.to_string()).collect(),
        )
        .unwrap();

        assert_eq!(expect_ignored, platform_requirement_filter.is_ignored(req));
    }
}

fn data_is_ignored() -> Vec<(Vec<&'static str>, &'static str, bool)> {
    vec![
        // 'ext-json is ignored if listed'
        (vec!["ext-json", "monolog/monolog"], "ext-json", true),
        // 'php is not ignored if not listed'
        (vec!["ext-json", "monolog/monolog"], "php", false),
        // 'monolog/monolog is not ignored even if listed'
        (
            vec!["ext-json", "monolog/monolog"],
            "monolog/monolog",
            false,
        ),
        // 'ext-json is ignored if ext-* is listed'
        (vec!["ext-*"], "ext-json", true),
        // 'php is ignored if php* is listed'
        (vec!["ext-*", "php*"], "php", true),
        // 'ext-json is ignored if * is listed'
        (vec!["foo", "*"], "ext-json", true),
        // 'php is ignored if * is listed'
        (vec!["*", "foo"], "php", true),
        // 'monolog/monolog is not ignored even if * or monolog/* are listed'
        (vec!["*", "monolog/*"], "monolog/monolog", false),
        // 'empty list entry does not ignore'
        (vec![""], "ext-foo", false),
        // 'empty array does not ignore'
        (vec![], "ext-foo", false),
        // 'list entries are not completing each other'
        (vec!["ext-", "foo"], "ext-foo", false),
    ]
}

#[test]
fn test_is_upper_bound_ignored() {
    for (req_list, req, expect_ignored) in data_is_upper_bound_ignored() {
        let platform_requirement_filter = IgnoreListPlatformRequirementFilter::new(
            req_list.iter().map(|s| s.to_string()).collect(),
        )
        .unwrap();

        assert_eq!(
            expect_ignored,
            platform_requirement_filter.is_upper_bound_ignored(req)
        );
    }
}

fn data_is_upper_bound_ignored() -> Vec<(Vec<&'static str>, &'static str, bool)> {
    vec![
        // 'ext-json is ignored if listed and fully ignored'
        (vec!["ext-json", "monolog/monolog"], "ext-json", true),
        // 'ext-json is ignored if listed and upper bound ignored'
        (vec!["ext-json+", "monolog/monolog"], "ext-json", true),
        // 'php is not ignored if not listed'
        (vec!["ext-json+", "monolog/monolog"], "php", false),
        // 'monolog/monolog is not ignored even if listed'
        (vec!["monolog/monolog"], "monolog/monolog", false),
        // 'ext-json is ignored if ext-* is listed'
        (vec!["ext-*+"], "ext-json", true),
        // 'php is ignored if php* is listed'
        (vec!["ext-*+", "php*+"], "php", true),
        // 'ext-json is ignored if * is listed'
        (vec!["foo", "*+"], "ext-json", true),
        // 'php is ignored if * is listed'
        (vec!["*+", "foo"], "php", true),
        // 'monolog/monolog is not ignored even if * or monolog/* are listed'
        (vec!["*+", "monolog/*+"], "monolog/monolog", false),
        // 'empty list entry does not ignore'
        (vec![""], "ext-foo", false),
        // 'empty array does not ignore'
        (vec![], "ext-foo", false),
        // 'list entries are not completing each other'
        (vec!["ext-", "foo"], "ext-foo", false),
    ]
}
