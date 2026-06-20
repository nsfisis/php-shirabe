//! ref: composer/tests/Composer/Test/Filter/PlatformRequirementFilter/IgnoreNothingPlatformRequirementFilterTest.php

use shirabe::filter::platform_requirement_filter::{
    IgnoreNothingPlatformRequirementFilter, PlatformRequirementFilterInterface,
};

#[test]
fn test_is_ignored() {
    for req in data_is_ignored() {
        let platform_requirement_filter = IgnoreNothingPlatformRequirementFilter;

        assert!(!platform_requirement_filter.is_ignored(req));
        assert!(!platform_requirement_filter.is_upper_bound_ignored(req));
    }
}

fn data_is_ignored() -> Vec<&'static str> {
    vec![
        // 'php is not ignored'
        "php",
        // 'monolog/monolog is not ignored'
        "monolog/monolog",
    ]
}
