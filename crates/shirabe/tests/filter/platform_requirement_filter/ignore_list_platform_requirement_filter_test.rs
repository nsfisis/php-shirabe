//! ref: composer/tests/Composer/Test/Filter/PlatformRequirementFilter/IgnoreListPlatformRequirementFilterTest.php sha1:81b3c5d9b7ef599d1b17fbc11b96581e5cb425ea

use bstr::BStr;
use shirabe::filter::platform_requirement_filter::{
    IgnoreListPlatformRequirementFilter, PlatformRequirementFilterInterface,
};

#[test]
fn test_is_ignored() {
    for (req_list, req, expect_ignored) in data_is_ignored() {
        let platform_requirement_filter = IgnoreListPlatformRequirementFilter::new(
            req_list.iter().map(|s| (*s).into()).collect(),
        )
        .unwrap();

        assert_eq!(
            expect_ignored,
            platform_requirement_filter.is_ignored(bstr::BStr::new(req))
        );
    }
}

fn data_is_ignored() -> Vec<(Vec<&'static BStr>, &'static BStr, bool)> {
    vec![
        // 'ext-json is ignored if listed'
        (
            vec![BStr::new("ext-json"), BStr::new("monolog/monolog")],
            BStr::new("ext-json"),
            true,
        ),
        // 'php is not ignored if not listed'
        (
            vec![BStr::new("ext-json"), BStr::new("monolog/monolog")],
            BStr::new("php"),
            false,
        ),
        // 'monolog/monolog is not ignored even if listed'
        (
            vec![BStr::new("ext-json"), BStr::new("monolog/monolog")],
            BStr::new("monolog/monolog"),
            false,
        ),
        // 'ext-json is ignored if ext-* is listed'
        (vec![BStr::new("ext-*")], BStr::new("ext-json"), true),
        // 'php is ignored if php* is listed'
        (
            vec![BStr::new("ext-*"), BStr::new("php*")],
            BStr::new("php"),
            true,
        ),
        // 'ext-json is ignored if * is listed'
        (
            vec![BStr::new("foo"), BStr::new("*")],
            BStr::new("ext-json"),
            true,
        ),
        // 'php is ignored if * is listed'
        (
            vec![BStr::new("*"), BStr::new("foo")],
            BStr::new("php"),
            true,
        ),
        // 'monolog/monolog is not ignored even if * or monolog/* are listed'
        (
            vec![BStr::new("*"), BStr::new("monolog/*")],
            BStr::new("monolog/monolog"),
            false,
        ),
        // 'empty list entry does not ignore'
        (vec![BStr::new("")], BStr::new("ext-foo"), false),
        // 'empty array does not ignore'
        (vec![], BStr::new("ext-foo"), false),
        // 'list entries are not completing each other'
        (
            vec![BStr::new("ext-"), BStr::new("foo")],
            BStr::new("ext-foo"),
            false,
        ),
    ]
}

#[test]
fn test_is_upper_bound_ignored() {
    for (req_list, req, expect_ignored) in data_is_upper_bound_ignored() {
        let platform_requirement_filter = IgnoreListPlatformRequirementFilter::new(
            req_list.iter().map(|s| (*s).into()).collect(),
        )
        .unwrap();

        assert_eq!(
            expect_ignored,
            platform_requirement_filter.is_upper_bound_ignored(bstr::BStr::new(req))
        );
    }
}

fn data_is_upper_bound_ignored() -> Vec<(Vec<&'static BStr>, &'static BStr, bool)> {
    vec![
        // 'ext-json is ignored if listed and fully ignored'
        (
            vec![BStr::new("ext-json"), BStr::new("monolog/monolog")],
            BStr::new("ext-json"),
            true,
        ),
        // 'ext-json is ignored if listed and upper bound ignored'
        (
            vec![BStr::new("ext-json+"), BStr::new("monolog/monolog")],
            BStr::new("ext-json"),
            true,
        ),
        // 'php is not ignored if not listed'
        (
            vec![BStr::new("ext-json+"), BStr::new("monolog/monolog")],
            BStr::new("php"),
            false,
        ),
        // 'monolog/monolog is not ignored even if listed'
        (
            vec![BStr::new("monolog/monolog")],
            BStr::new("monolog/monolog"),
            false,
        ),
        // 'ext-json is ignored if ext-* is listed'
        (vec![BStr::new("ext-*+")], BStr::new("ext-json"), true),
        // 'php is ignored if php* is listed'
        (
            vec![BStr::new("ext-*+"), BStr::new("php*+")],
            BStr::new("php"),
            true,
        ),
        // 'ext-json is ignored if * is listed'
        (
            vec![BStr::new("foo"), BStr::new("*+")],
            BStr::new("ext-json"),
            true,
        ),
        // 'php is ignored if * is listed'
        (
            vec![BStr::new("*+"), BStr::new("foo")],
            BStr::new("php"),
            true,
        ),
        // 'monolog/monolog is not ignored even if * or monolog/* are listed'
        (
            vec![BStr::new("*+"), BStr::new("monolog/*+")],
            BStr::new("monolog/monolog"),
            false,
        ),
        // 'empty list entry does not ignore'
        (vec![BStr::new("")], BStr::new("ext-foo"), false),
        // 'empty array does not ignore'
        (vec![], BStr::new("ext-foo"), false),
        // 'list entries are not completing each other'
        (
            vec![BStr::new("ext-"), BStr::new("foo")],
            BStr::new("ext-foo"),
            false,
        ),
    ]
}
