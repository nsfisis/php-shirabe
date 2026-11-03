//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/IgnoreNothingPlatformRequirementFilter.php

use super::PlatformRequirementFilterInterface;
use bstr::BStr;

#[derive(Debug)]
pub struct IgnoreNothingPlatformRequirementFilter;

impl PlatformRequirementFilterInterface for IgnoreNothingPlatformRequirementFilter {
    fn is_ignored(&self, _req: &BStr) -> bool {
        false
    }

    fn is_upper_bound_ignored(&self, _req: &BStr) -> bool {
        false
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
