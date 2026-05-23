//! ref: composer/vendor/composer/semver/src/Constraint/MatchAllConstraint.php

use crate::constraint::Bound;

#[derive(Debug, Clone, Default)]
pub struct MatchAllConstraint {
    pub(crate) pretty_string: Option<String>,
}

impl MatchAllConstraint {
    pub fn new(pretty_string: Option<String>) -> Self {
        Self { pretty_string }
    }

    pub fn compile(&self, _other_operator: i64) -> String {
        "true".to_string()
    }

    pub fn get_pretty_string(&self) -> String {
        if let Some(ref s) = self.pretty_string
            && !s.is_empty()
        {
            return s.clone();
        }
        self.to_string()
    }

    pub fn get_upper_bound(&self) -> Bound {
        Bound::positive_infinity()
    }

    pub fn get_lower_bound(&self) -> Bound {
        Bound::zero()
    }
}

impl std::fmt::Display for MatchAllConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "*")
    }
}
