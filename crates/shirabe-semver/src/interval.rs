//! ref: composer/vendor/composer/semver/src/Interval.php

use crate::constraint::SimpleConstraint;
use bstr::BString;
use shirabe_php_shim::bconcat;

#[derive(Debug, Clone)]
pub struct DevConstraintSet {
    pub names: Vec<BString>,
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
        SimpleConstraint::new(">=", "0.0.0.0-dev", None)
    }

    pub fn until_positive_infinity() -> SimpleConstraint {
        SimpleConstraint::new("<", bconcat!(i64::MAX, ".0.0.0"), None)
    }

    pub fn any() -> Self {
        Self::new(Self::from_zero(), Self::until_positive_infinity())
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
