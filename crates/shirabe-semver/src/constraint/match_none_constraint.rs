//! ref: composer/vendor/composer/semver/src/Constraint/MatchNoneConstraint.php sha1:fd6e00f957b8eabd78601e8d5543225ec77cd496

use crate::constraint::Bound;
use bstr::BString;
use shirabe_php_shim::{CmpOp, Stringable};

#[derive(Debug, Clone)]
pub struct MatchNoneConstraint {
    pub(crate) pretty_string: Option<BString>,
}

impl MatchNoneConstraint {
    pub fn new(pretty_string: Option<BString>) -> Self {
        Self { pretty_string }
    }

    pub fn compile(&self, _other_operator: CmpOp) -> BString {
        "false".into()
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
        Bound::new("0.0.0.0-dev", false)
    }

    pub fn get_lower_bound(&self) -> Bound {
        Bound::new("0.0.0.0-dev", false)
    }

    /// Appends PHP's `__toString()` to `out`.
    pub fn append_php_string(&self, out: &mut BString) {
        out.extend_from_slice(b"[]");
    }
}

impl Stringable for MatchNoneConstraint {
    fn to_php_string(&self) -> BString {
        "[]".into()
    }
}
