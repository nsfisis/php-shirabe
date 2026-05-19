//! ref: composer/vendor/composer/semver/src/Constraint/ConstraintInterface.php

use crate::constraint::bound::Bound;

pub trait ConstraintInterface: std::fmt::Debug {
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

    /// Rust-specific helper: PHP `$c instanceof Constraint` check.
    fn is_constraint(&self) -> bool {
        false
    }

    /// Rust-specific helper: PHP `$c->getOperator()`. Only meaningful when `is_constraint()` is true.
    fn get_operator(&self) -> &'static str {
        ""
    }

    /// Rust-specific helper: PHP `$c->getVersion()`. Only meaningful when `is_constraint()` is true.
    fn get_version(&self) -> &str {
        ""
    }

    fn clone_box(&self) -> Box<dyn ConstraintInterface>;

    fn as_any(&self) -> &dyn std::any::Any;
}
