//! ref: composer/vendor/composer/semver/src/Comparator.php

use crate::constraint::constraint::Constraint;

pub struct Comparator;

impl Comparator {
    pub fn greater_than(version1: String, version2: String) -> bool {
        Self::compare(version1, ">".to_string(), version2)
    }

    pub fn greater_than_or_equal_to(version1: String, version2: String) -> bool {
        Self::compare(version1, ">=".to_string(), version2)
    }

    pub fn less_than(version1: String, version2: String) -> bool {
        Self::compare(version1, "<".to_string(), version2)
    }

    pub fn less_than_or_equal_to(version1: String, version2: String) -> bool {
        Self::compare(version1, "<=".to_string(), version2)
    }

    pub fn equal_to(version1: String, version2: String) -> bool {
        Self::compare(version1, "==".to_string(), version2)
    }

    pub fn not_equal_to(version1: String, version2: String) -> bool {
        Self::compare(version1, "!=".to_string(), version2)
    }

    pub fn compare(version1: String, operator: String, version2: String) -> bool {
        let constraint = Constraint::new(operator, version2).unwrap();
        constraint.match_specific(&Constraint::new("==".to_string(), version1).unwrap(), true)
    }
}
