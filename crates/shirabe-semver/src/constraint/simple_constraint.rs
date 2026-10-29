//! ref: composer/vendor/composer/semver/src/Constraint/Constraint.php

use crate::constraint::Bound;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{CmpOp, bconcat, var_export_str, version_compare};

/// Corresponds to PHP's `Constraint`.
#[derive(Debug, Clone)]
pub struct SimpleConstraint {
    operator: CmpOp,
    version: BString,
    pub(crate) pretty_string: Option<BString>,
}

impl SimpleConstraint {
    fn trans_op_str(op: &[u8]) -> Option<CmpOp> {
        match op {
            b"=" => Some(CmpOp::Eq),
            b"==" => Some(CmpOp::Eq),
            b"<" => Some(CmpOp::Lt),
            b"<=" => Some(CmpOp::Le),
            b">" => Some(CmpOp::Gt),
            b">=" => Some(CmpOp::Ge),
            b"<>" => Some(CmpOp::Ne),
            b"!=" => Some(CmpOp::Ne),
            _ => None,
        }
    }

    pub fn new(
        operator: impl AsRef<[u8]>,
        version: impl Into<BString>,
        pretty_string: Option<BString>,
    ) -> Self {
        let operator = operator.as_ref();
        let op = Self::trans_op_str(operator).unwrap_or_else(|| {
            // PHP raises InvalidArgumentException; in the Rust port keep that as a panic
            // because invalid operators are programmer errors caught during porting.
            panic!(
                "Invalid operator \"{}\" given, expected one of: {}",
                operator.as_bstr().display_lossy(),
                Self::get_supported_operators().join(", ")
            )
        });

        Self {
            operator: op,
            version: version.into(),
            pretty_string,
        }
    }

    pub fn get_version(&self) -> &BStr {
        self.version.as_bstr()
    }

    pub fn get_operator(&self) -> CmpOp {
        self.operator
    }

    pub fn get_supported_operators() -> Vec<&'static str> {
        vec!["=", "==", "<", "<=", ">", ">=", "<>", "!="]
    }

    pub fn get_operator_constant(operator: CmpOp) -> i64 {
        match operator {
            CmpOp::Eq => 0,
            CmpOp::Lt => 1,
            CmpOp::Le => 2,
            CmpOp::Gt => 3,
            CmpOp::Ge => 4,
            CmpOp::Ne => 5,
        }
    }

    pub fn from_operator_constant(constant: i64) -> Option<CmpOp> {
        Some(match constant {
            0 => CmpOp::Eq,
            1 => CmpOp::Lt,
            2 => CmpOp::Le,
            3 => CmpOp::Gt,
            4 => CmpOp::Ge,
            5 => CmpOp::Ne,
            _ => return None,
        })
    }

    pub fn version_compare(
        &self,
        a: &[u8],
        b: &[u8],
        operator: CmpOp,
        compare_branches: bool,
    ) -> bool {
        let a_is_branch = a.starts_with(b"dev-");
        let b_is_branch = b.starts_with(b"dev-");

        if operator == CmpOp::Ne && (a_is_branch || b_is_branch) {
            return a != b;
        }

        if a_is_branch && b_is_branch {
            return operator == CmpOp::Eq && a == b;
        }

        // when branches are not comparable, we make sure dev branches never match anything
        if !compare_branches && (a_is_branch || b_is_branch) {
            return false;
        }

        version_compare(a, b, operator)
    }

    pub fn compile_constraint(&self, other_operator: CmpOp) -> BString {
        let version = var_export_str(&self.version, true);
        if self.version.starts_with(b"dev-") {
            if CmpOp::Eq == self.operator {
                if CmpOp::Eq == other_operator {
                    return bconcat!("$b && $v === ", version);
                }
                if CmpOp::Ne == other_operator {
                    return bconcat!("!$b || $v !== ", version);
                }
                return "false".into();
            }

            if CmpOp::Ne == self.operator {
                if CmpOp::Eq == other_operator {
                    return bconcat!("!$b || $v !== ", version);
                }
                if CmpOp::Ne == other_operator {
                    return "true".into();
                }
                return "!$b".into();
            }

            return "false".into();
        }

        if CmpOp::Eq == self.operator {
            if CmpOp::Eq == other_operator {
                return bconcat!("\\version_compare($v, ", version, ", '==')");
            }
            if CmpOp::Ne == other_operator {
                return bconcat!("$b || \\version_compare($v, ", version, ", '!=')");
            }
            return bconcat!(
                "!$b && \\version_compare(",
                version,
                ", $v, '",
                other_operator.to_string(),
                "')"
            );
        }

        if CmpOp::Ne == self.operator {
            if CmpOp::Eq == other_operator {
                return bconcat!("$b || (!$b && \\version_compare($v, ", version, ", '!='))");
            }
            if CmpOp::Ne == other_operator {
                return "true".into();
            }
            return "!$b".into();
        }

        if CmpOp::Lt == self.operator || CmpOp::Le == self.operator {
            if CmpOp::Lt == other_operator || CmpOp::Le == other_operator {
                return "!$b".into();
            }
        } else if CmpOp::Gt == other_operator || CmpOp::Ge == other_operator {
            return "!$b".into();
        }

        if CmpOp::Ne == other_operator {
            return "true".into();
        }

        let code_comparison = bconcat!(
            "\\version_compare($v, ",
            version,
            ", '",
            self.operator.to_string(),
            "')"
        );

        if self.operator == CmpOp::Le && other_operator == CmpOp::Gt {
            return bconcat!(
                "!$b && \\version_compare($v, ",
                version,
                ", '!=') && ",
                code_comparison
            );
        }

        if self.operator == CmpOp::Ge && other_operator == CmpOp::Lt {
            return bconcat!(
                "!$b && \\version_compare($v, ",
                version,
                ", '!=') && ",
                code_comparison
            );
        }

        bconcat!("!$b && ", code_comparison)
    }

    pub fn match_specific(&self, provider: &SimpleConstraint, compare_branches: bool) -> bool {
        let no_equal_op = self.operator.to_string().replace('=', "");
        let provider_no_equal_op = provider.operator.to_string().replace('=', "");

        let is_equal_op = CmpOp::Eq == self.operator;
        let is_non_equal_op = CmpOp::Ne == self.operator;
        let is_provider_equal_op = CmpOp::Eq == provider.operator;
        let is_provider_non_equal_op = CmpOp::Ne == provider.operator;

        if is_non_equal_op || is_provider_non_equal_op {
            if is_non_equal_op
                && !is_provider_non_equal_op
                && !is_provider_equal_op
                && provider.version.starts_with(b"dev-")
            {
                return false;
            }

            if is_provider_non_equal_op
                && !is_non_equal_op
                && !is_equal_op
                && self.version.starts_with(b"dev-")
            {
                return false;
            }

            if !is_equal_op && !is_provider_equal_op {
                return true;
            }
            return self.version_compare(
                &provider.version,
                &self.version,
                CmpOp::Ne,
                compare_branches,
            );
        }

        if self.operator != CmpOp::Eq && no_equal_op == provider_no_equal_op {
            return !(self.version.starts_with(b"dev-") || provider.version.starts_with(b"dev-"));
        }

        let (version1, version2, operator) = if is_equal_op {
            (&self.version, &provider.version, provider.operator)
        } else {
            (&provider.version, &self.version, self.operator)
        };

        if self.version_compare(version1, version2, operator, compare_branches) {
            return !(provider.operator.to_string() == provider_no_equal_op
                && self.operator.to_string() != no_equal_op
                && version_compare(&provider.version, &self.version, CmpOp::Eq));
        }

        false
    }

    /// Composer memoizes the result; this port recomputes on every call. It is not heavy
    /// calculation so caching is a premature optimization.
    fn extract_bounds(&self) -> (Bound, Bound) {
        if self.version.starts_with(b"dev-") {
            return (Bound::zero(), Bound::positive_infinity());
        }

        match self.operator {
            CmpOp::Eq => (
                Bound::new(self.version.clone(), true),
                Bound::new(self.version.clone(), true),
            ),
            CmpOp::Lt => (Bound::zero(), Bound::new(self.version.clone(), false)),
            CmpOp::Le => (Bound::zero(), Bound::new(self.version.clone(), true)),
            CmpOp::Gt => (
                Bound::new(self.version.clone(), false),
                Bound::positive_infinity(),
            ),
            CmpOp::Ge => (
                Bound::new(self.version.clone(), true),
                Bound::positive_infinity(),
            ),
            CmpOp::Ne => (Bound::zero(), Bound::positive_infinity()),
        }
    }

    pub fn compile(&self, other_operator: CmpOp) -> BString {
        self.compile_constraint(other_operator)
    }

    pub fn get_pretty_string(&self) -> BString {
        if let Some(ref s) = self.pretty_string
            && !s.is_empty()
        {
            return s.clone();
        }
        self.to_php_string()
    }

    /// PHP's `__toString()`.
    pub fn to_php_string(&self) -> BString {
        let mut out = BString::default();
        self.append_php_string(&mut out);
        out
    }

    /// Appends PHP's `__toString()` to `out`.
    pub fn append_php_string(&self, out: &mut BString) {
        out.extend_from_slice(self.operator.as_str().as_bytes());
        out.push(b' ');
        out.extend_from_slice(&self.version);
    }

    pub fn get_lower_bound(&self) -> Bound {
        self.extract_bounds().0
    }

    pub fn get_upper_bound(&self) -> Bound {
        self.extract_bounds().1
    }
}
