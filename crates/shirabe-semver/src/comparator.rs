//! ref: composer/vendor/composer/semver/src/Comparator.php

use crate::constraint::SimpleConstraint;

pub struct Comparator;

impl Comparator {
    pub fn greater_than(version1: impl AsRef<[u8]>, version2: impl AsRef<[u8]>) -> bool {
        Self::compare(version1, ">", version2)
    }

    pub fn greater_than_or_equal_to(
        version1: impl AsRef<[u8]>,
        version2: impl AsRef<[u8]>,
    ) -> bool {
        Self::compare(version1, ">=", version2)
    }

    pub fn less_than(version1: impl AsRef<[u8]>, version2: impl AsRef<[u8]>) -> bool {
        Self::compare(version1, "<", version2)
    }

    pub fn less_than_or_equal_to(version1: impl AsRef<[u8]>, version2: impl AsRef<[u8]>) -> bool {
        Self::compare(version1, "<=", version2)
    }

    pub fn equal_to(version1: impl AsRef<[u8]>, version2: impl AsRef<[u8]>) -> bool {
        Self::compare(version1, "==", version2)
    }

    pub fn not_equal_to(version1: impl AsRef<[u8]>, version2: impl AsRef<[u8]>) -> bool {
        Self::compare(version1, "!=", version2)
    }

    pub fn compare(
        version1: impl AsRef<[u8]>,
        operator: impl AsRef<[u8]>,
        version2: impl AsRef<[u8]>,
    ) -> bool {
        let constraint = SimpleConstraint::new(operator, version2.as_ref(), None);
        constraint.match_specific(&SimpleConstraint::new("==", version1.as_ref(), None), true)
    }
}
