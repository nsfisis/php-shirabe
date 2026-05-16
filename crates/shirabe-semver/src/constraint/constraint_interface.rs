//! ref: composer/vendor/composer/semver/src/Constraint/ConstraintInterface.php

use crate::constraint::bound::Bound;

pub trait ConstraintInterface {
    fn matches(&self, provider: &dyn ConstraintInterface) -> bool;

    fn compile(&self, other_operator: i64) -> String;

    fn get_upper_bound(&self) -> Bound;

    fn get_lower_bound(&self) -> Bound;

    fn get_pretty_string(&self) -> String;

    fn set_pretty_string(&mut self, pretty_string: Option<String>);

    fn __to_string(&self) -> String;

    // Rust-specific helpers for instanceof checks in MultiConstraint::matches and optimizeConstraints.
    fn is_disjunctive(&self) -> bool {
        false
    }

    fn as_any(&self) -> &dyn std::any::Any;
}
