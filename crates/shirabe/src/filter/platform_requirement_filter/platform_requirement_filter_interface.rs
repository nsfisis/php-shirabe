//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/PlatformRequirementFilterInterface.php sha1:490c0cb96843f0c969bbecb89ff8957c028d7cbc

use bstr::BStr;

pub trait PlatformRequirementFilterInterface: std::fmt::Debug {
    fn is_ignored(&self, req: &BStr) -> bool;

    fn is_upper_bound_ignored(&self, req: &BStr) -> bool;

    fn as_any(&self) -> &dyn std::any::Any;
}
