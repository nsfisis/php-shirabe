//! ref: composer/vendor/composer/semver/src/Intervals.php

use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct Intervals;

impl Intervals {
    pub fn is_subset_of(
        _constraint_a: &AnyConstraint,
        _constraint_b: &AnyConstraint,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    pub fn compact_constraint(_constraint: AnyConstraint) -> AnyConstraint {
        todo!()
    }

    pub fn compact(_constraint: &AnyConstraint) -> AnyConstraint {
        todo!()
    }

    pub fn get(_constraint: &AnyConstraint) -> anyhow::Result<PhpMixed> {
        todo!()
    }

    pub fn clear() {
        todo!()
    }
}
