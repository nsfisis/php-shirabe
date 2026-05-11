//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/PlatformRequirementFilterInterface.php

pub trait PlatformRequirementFilterInterface {
    fn is_ignored(&self, req: &str) -> bool;

    fn is_upper_bound_ignored(&self, req: &str) -> bool;
}
