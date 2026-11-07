//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/IgnoreNothingPlatformRequirementFilter.php sha1:51be3fe6eddf70b103044057fd67b040e81ecc1d

use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
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
