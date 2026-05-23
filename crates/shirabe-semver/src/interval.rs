//! ref: composer/vendor/composer/semver/src/Interval.php

use crate::constraint::SimpleConstraint;

#[derive(Debug, Clone)]
pub struct DevConstraintSet {
    pub names: Vec<String>,
    pub exclude: bool,
}

#[derive(Debug, Clone)]
pub struct Interval {
    start: SimpleConstraint,
    end: SimpleConstraint,
}

impl Interval {
    pub fn new(start: SimpleConstraint, end: SimpleConstraint) -> Self {
        Self { start, end }
    }

    pub fn get_start(&self) -> &SimpleConstraint {
        &self.start
    }

    pub fn get_end(&self) -> &SimpleConstraint {
        &self.end
    }

    pub fn from_zero() -> SimpleConstraint {
        SimpleConstraint::new(">=".to_string(), "0.0.0.0-dev".to_string(), None)
    }

    pub fn until_positive_infinity() -> SimpleConstraint {
        SimpleConstraint::new("<".to_string(), format!("{}.0.0.0", i64::MAX), None)
    }

    pub fn any() -> Self {
        Self::new(
            Self::from_zero().clone(),
            Self::until_positive_infinity().clone(),
        )
    }

    pub fn any_dev() -> DevConstraintSet {
        DevConstraintSet {
            names: vec![],
            exclude: true,
        }
    }

    pub fn no_dev() -> DevConstraintSet {
        DevConstraintSet {
            names: vec![],
            exclude: false,
        }
    }
}
