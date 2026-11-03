//! ref: composer/tests/Composer/Test/Filter/PlatformRequirementFilter/IgnoreNothingPlatformRequirementFilterTest.php

use bstr::BStr;
use shirabe::filter::platform_requirement_filter::{
    IgnoreNothingPlatformRequirementFilter, PlatformRequirementFilterInterface,
};

#[test]
fn test_is_ignored() {
    for req in data_is_ignored() {
        let platform_requirement_filter = IgnoreNothingPlatformRequirementFilter;

        assert!(!platform_requirement_filter.is_ignored(bstr::BStr::new(req)));
        assert!(!platform_requirement_filter.is_upper_bound_ignored(bstr::BStr::new(req)));
    }
}

fn data_is_ignored() -> Vec<&'static BStr> {
    vec![
        // 'php is not ignored'
        BStr::new("php"),
        // 'monolog/monolog is not ignored'
        BStr::new("monolog/monolog"),
    ]
}
