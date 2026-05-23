//! ref: composer/vendor/composer/semver/src/Constraint/MatchNoneConstraint.php

use crate::constraint::Bound;

#[derive(Debug, Clone)]
pub struct MatchNoneConstraint {
    pub(crate) pretty_string: Option<String>,
}

impl MatchNoneConstraint {
    pub fn new(pretty_string: Option<String>) -> Self {
        Self { pretty_string }
    }

    pub fn compile(&self, _other_operator: i64) -> String {
        "false".to_string()
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
        Bound::new("0.0.0.0-dev".to_string(), false)
    }

    pub fn get_lower_bound(&self) -> Bound {
        Bound::new("0.0.0.0-dev".to_string(), false)
    }
}

impl std::fmt::Display for MatchNoneConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[]")
    }
}
