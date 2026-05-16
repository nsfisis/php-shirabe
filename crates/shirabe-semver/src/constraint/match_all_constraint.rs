//! ref: composer/vendor/composer/semver/src/Constraint/MatchAllConstraint.php

use crate::constraint::bound::Bound;
use crate::constraint::constraint_interface::ConstraintInterface;

#[derive(Debug)]
pub struct MatchAllConstraint {
    pub(crate) pretty_string: Option<String>,
}

impl ConstraintInterface for MatchAllConstraint {
    fn matches(&self, _provider: &dyn ConstraintInterface) -> bool {
        true
    }

    fn compile(&self, _other_operator: i64) -> String {
        "true".to_string()
    }

    fn set_pretty_string(&mut self, pretty_string: Option<String>) {
        self.pretty_string = pretty_string;
    }

    fn get_pretty_string(&self) -> String {
        if let Some(ref s) = self.pretty_string {
            if !s.is_empty() {
                return s.clone();
            }
        }
        self.__to_string()
    }

    fn __to_string(&self) -> String {
        "*".to_string()
    }

    fn clone_box(&self) -> Box<dyn ConstraintInterface> {
        Box::new(MatchAllConstraint {
            pretty_string: self.pretty_string.clone(),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_upper_bound(&self) -> Bound {
        Bound::positive_infinity()
    }

    fn get_lower_bound(&self) -> Bound {
        Bound::zero()
    }
}
