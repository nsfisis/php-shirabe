//! ref: composer/src/Composer/Filter/PlatformRequirementFilter/PlatformRequirementFilterInterface.php

pub trait PlatformRequirementFilterInterface: std::fmt::Debug {
    fn is_ignored(&self, req: &str) -> bool;

    fn is_upper_bound_ignored(&self, req: &str) -> bool;

    fn as_any(&self) -> &dyn std::any::Any;

    fn clone_box(&self) -> Box<dyn PlatformRequirementFilterInterface> {
        todo!()
    }
}
