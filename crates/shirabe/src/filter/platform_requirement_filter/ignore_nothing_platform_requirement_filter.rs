//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/IgnoreNothingPlatformRequirementFilter.php

use super::platform_requirement_filter_interface::PlatformRequirementFilterInterface;

#[derive(Debug)]
pub struct IgnoreNothingPlatformRequirementFilter;

impl PlatformRequirementFilterInterface for IgnoreNothingPlatformRequirementFilter {
    fn is_ignored(&self, _req: &str) -> bool {
        false
    }

    fn is_upper_bound_ignored(&self, _req: &str) -> bool {
        false
    }
}
