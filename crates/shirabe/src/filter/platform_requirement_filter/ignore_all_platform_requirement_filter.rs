//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/IgnoreAllPlatformRequirementFilter.php

use crate::filter::platform_requirement_filter::platform_requirement_filter_interface::PlatformRequirementFilterInterface;
use crate::repository::platform_repository::PlatformRepository;

#[derive(Debug)]
pub struct IgnoreAllPlatformRequirementFilter;

impl PlatformRequirementFilterInterface for IgnoreAllPlatformRequirementFilter {
    fn is_ignored(&self, req: &str) -> bool {
        PlatformRepository::is_platform_package(req)
    }

    fn is_upper_bound_ignored(&self, req: &str) -> bool {
        self.is_ignored(req)
    }
}
