//! ref: composer/vendor/composer/semver/src/Constraint/MatchAllConstraint.php

use crate::constraint::Bound;
use bstr::BString;
use shirabe_php_shim::CmpOp;

#[derive(Debug, Clone, Default)]
pub struct MatchAllConstraint {
    pub(crate) pretty_string: Option<BString>,
}

impl MatchAllConstraint {
    pub fn new(pretty_string: Option<BString>) -> Self {
        Self { pretty_string }
    }

    pub fn compile(&self, _other_operator: CmpOp) -> BString {
        "true".into()
    }

    pub fn get_pretty_string(&self) -> BString {
        if let Some(ref s) = self.pretty_string
            && !s.is_empty()
        {
            return s.clone();
        }
        self.to_php_string()
    }

    pub fn get_upper_bound(&self) -> Bound {
        Bound::positive_infinity()
    }

    pub fn get_lower_bound(&self) -> Bound {
        Bound::zero()
    }

    /// PHP's `__toString()`.
    pub fn to_php_string(&self) -> BString {
        "*".into()
    }

    /// Appends PHP's `__toString()` to `out`.
    pub fn append_php_string(&self, out: &mut BString) {
        out.extend_from_slice(b"*");
    }
}

impl std::fmt::Display for MatchAllConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "*")
    }
}
