//! ref: composer/vendor/composer/semver/src/Interval.php

use std::sync::OnceLock;

use crate::constraint::constraint::Constraint;

#[derive(Debug, Clone)]
pub struct DevConstraintSet {
    pub names: Vec<String>,
    pub exclude: bool,
}

#[derive(Debug, Clone)]
pub struct Interval {
    start: Constraint,
    end: Constraint,
}

impl Interval {
    pub fn new(start: Constraint, end: Constraint) -> Self {
        Self { start, end }
    }

    pub fn get_start(&self) -> &Constraint {
        &self.start
    }

    pub fn get_end(&self) -> &Constraint {
        &self.end
    }

    pub fn from_zero() -> &'static Constraint {
        static ZERO: OnceLock<Constraint> = OnceLock::new();
        ZERO.get_or_init(|| Constraint::new(">=".to_string(), "0.0.0.0-dev".to_string()))
    }

    pub fn until_positive_infinity() -> &'static Constraint {
        static POSITIVE_INFINITY: OnceLock<Constraint> = OnceLock::new();
        POSITIVE_INFINITY
            .get_or_init(|| Constraint::new("<".to_string(), format!("{}.0.0.0", i64::MAX)))
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
