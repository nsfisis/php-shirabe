//! ref: composer/tests/Composer/Test/Filter/PlatformRequirementFilter/IgnoreAllPlatformRequirementFilterTest.php sha1:b70b6b64bcf6ebf18909b609f53a695d1a5e5ace

use bstr::BStr;
use shirabe::filter::platform_requirement_filter::{
    IgnoreAllPlatformRequirementFilter, PlatformRequirementFilterInterface,
};

#[test]
fn test_is_ignored() {
    for (req, expect_ignored) in data_is_ignored() {
        let platform_requirement_filter = IgnoreAllPlatformRequirementFilter;

        assert_eq!(
            expect_ignored,
            platform_requirement_filter.is_ignored(bstr::BStr::new(req))
        );
        assert_eq!(
            expect_ignored,
            platform_requirement_filter.is_upper_bound_ignored(bstr::BStr::new(req))
        );
    }
}

fn data_is_ignored() -> Vec<(&'static BStr, bool)> {
    vec![
        // 'php is ignored'
        (BStr::new("php"), true),
        // 'monolog/monolog is not ignored'
        (BStr::new("monolog/monolog"), false),
    ]
}
