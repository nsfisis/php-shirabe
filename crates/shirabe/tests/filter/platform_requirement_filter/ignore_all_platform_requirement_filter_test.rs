//! ref: composer/tests/Composer/Test/Filter/PlatformRequirementFilter/IgnoreAllPlatformRequirementFilterTest.php

use shirabe::filter::platform_requirement_filter::{
    IgnoreAllPlatformRequirementFilter, PlatformRequirementFilterInterface,
};

#[test]
fn test_is_ignored() {
    for (req, expect_ignored) in data_is_ignored() {
        let platform_requirement_filter = IgnoreAllPlatformRequirementFilter;

        assert_eq!(expect_ignored, platform_requirement_filter.is_ignored(req));
        assert_eq!(
            expect_ignored,
            platform_requirement_filter.is_upper_bound_ignored(req)
        );
    }
}

fn data_is_ignored() -> Vec<(&'static str, bool)> {
    vec![
        // 'php is ignored'
        ("php", true),
        // 'monolog/monolog is not ignored'
        ("monolog/monolog", false),
    ]
}
