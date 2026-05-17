//! ref: composer/vendor/composer/semver/src/Constraint/MatchNoneConstraint.php

use crate::constraint::bound::Bound;
use crate::constraint::constraint_interface::ConstraintInterface;

#[derive(Debug)]
pub struct MatchNoneConstraint {
    pub(crate) pretty_string: Option<String>,
}

impl ConstraintInterface for MatchNoneConstraint {
    fn matches(&self, _provider: &dyn ConstraintInterface) -> bool {
        false
    }

    fn compile(&self, _other_operator: i64) -> String {
        "false".to_string()
    }

    fn set_pretty_string(&mut self, pretty_string: Option<String>) {
        self.pretty_string = pretty_string;
    }

    fn get_pretty_string(&self) -> String {
        if let Some(ref s) = self.pretty_string
            && !s.is_empty()
        {
            return s.clone();
        }
        self.__to_string()
    }

    fn __to_string(&self) -> String {
        "[]".to_string()
    }

    fn clone_box(&self) -> Box<dyn ConstraintInterface> {
        Box::new(MatchNoneConstraint {
            pretty_string: self.pretty_string.clone(),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_upper_bound(&self) -> Bound {
        Bound::new("0.0.0.0-dev".to_string(), false)
    }

    fn get_lower_bound(&self) -> Bound {
        Bound::new("0.0.0.0-dev".to_string(), false)
    }
}
