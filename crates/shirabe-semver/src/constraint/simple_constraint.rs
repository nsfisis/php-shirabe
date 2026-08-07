//! ref: composer/vendor/composer/semver/src/Constraint/Constraint.php

use crate::constraint::Bound;
use shirabe_php_shim::{CmpOp, var_export_str, version_compare};

/// Corresponds to PHP's `Constraint`.
#[derive(Debug, Clone)]
pub struct SimpleConstraint {
    pub(crate) operator: CmpOp,
    pub(crate) version: String,
    pub(crate) pretty_string: Option<String>,
}

impl SimpleConstraint {
    fn trans_op_str(op: &str) -> Option<CmpOp> {
        match op {
            "=" => Some(CmpOp::Eq),
            "==" => Some(CmpOp::Eq),
            "<" => Some(CmpOp::Lt),
            "<=" => Some(CmpOp::Le),
            ">" => Some(CmpOp::Gt),
            ">=" => Some(CmpOp::Ge),
            "<>" => Some(CmpOp::Ne),
            "!=" => Some(CmpOp::Ne),
            _ => None,
        }
    }

    pub fn new(operator: String, version: String, pretty_string: Option<String>) -> Self {
        let op = Self::trans_op_str(&operator).unwrap_or_else(|| {
            // PHP raises InvalidArgumentException; in the Rust port keep that as a panic
            // because invalid operators are programmer errors caught during porting.
            panic!(
                "Invalid operator \"{}\" given, expected one of: {}",
                operator,
                Self::get_supported_operators().join(", ")
            )
        });

        Self {
            operator: op,
            version,
            pretty_string,
        }
    }

    pub fn get_version(&self) -> &str {
        &self.version
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
        a: &str,
        b: &str,
        operator: CmpOp,
        compare_branches: bool,
    ) -> bool {
        let a_is_branch = a.starts_with("dev-");
        let b_is_branch = b.starts_with("dev-");

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

    pub fn compile_constraint(&self, other_operator: CmpOp) -> String {
        if self.version.starts_with("dev-") {
            if CmpOp::Eq == self.operator {
                if CmpOp::Eq == other_operator {
                    return format!("$b && $v === {}", var_export_str(&self.version, true));
                }
                if CmpOp::Ne == other_operator {
                    return format!("!$b || $v !== {}", var_export_str(&self.version, true));
                }
                return "false".to_string();
            }

            if CmpOp::Ne == self.operator {
                if CmpOp::Eq == other_operator {
                    return format!("!$b || $v !== {}", var_export_str(&self.version, true));
                }
                if CmpOp::Ne == other_operator {
                    return "true".to_string();
                }
                return "!$b".to_string();
            }

            return "false".to_string();
        }

        if CmpOp::Eq == self.operator {
            if CmpOp::Eq == other_operator {
                return format!(
                    "\\version_compare($v, {}, '==')",
                    var_export_str(&self.version, true)
                );
            }
            if CmpOp::Ne == other_operator {
                return format!(
                    "$b || \\version_compare($v, {}, '!=')",
                    var_export_str(&self.version, true)
                );
            }
            return format!(
                "!$b && \\version_compare({}, $v, '{}')",
                var_export_str(&self.version, true),
                other_operator
            );
        }

        if CmpOp::Ne == self.operator {
            if CmpOp::Eq == other_operator {
                return format!(
                    "$b || (!$b && \\version_compare($v, {}, '!='))",
                    var_export_str(&self.version, true)
                );
            }
            if CmpOp::Ne == other_operator {
                return "true".to_string();
            }
            return "!$b".to_string();
        }

        if CmpOp::Lt == self.operator || CmpOp::Le == self.operator {
            if CmpOp::Lt == other_operator || CmpOp::Le == other_operator {
                return "!$b".to_string();
            }
        } else if CmpOp::Gt == other_operator || CmpOp::Ge == other_operator {
            return "!$b".to_string();
        }

        if CmpOp::Ne == other_operator {
            return "true".to_string();
        }

        let code_comparison = format!(
            "\\version_compare($v, {}, '{}')",
            var_export_str(&self.version, true),
            self.operator
        );

        if self.operator == CmpOp::Le && other_operator == CmpOp::Gt {
            return format!(
                "!$b && \\version_compare($v, {}, '!=') && {}",
                var_export_str(&self.version, true),
                code_comparison
            );
        }

        if self.operator == CmpOp::Ge && other_operator == CmpOp::Lt {
            return format!(
                "!$b && \\version_compare($v, {}, '!=') && {}",
                var_export_str(&self.version, true),
                code_comparison
            );
        }

        format!("!$b && {}", code_comparison)
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
                && provider.version.starts_with("dev-")
            {
                return false;
            }

            if is_provider_non_equal_op
                && !is_non_equal_op
                && !is_equal_op
                && self.version.starts_with("dev-")
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
            return !(self.version.starts_with("dev-") || provider.version.starts_with("dev-"));
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
        if self.version.starts_with("dev-") {
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

    pub fn compile(&self, other_operator: CmpOp) -> String {
        self.compile_constraint(other_operator)
    }

    pub fn get_pretty_string(&self) -> String {
        if let Some(ref s) = self.pretty_string
            && !s.is_empty()
        {
            return s.clone();
        }
        self.to_string()
    }

    pub fn get_lower_bound(&self) -> Bound {
        self.extract_bounds().0
    }

    pub fn get_upper_bound(&self) -> Bound {
        self.extract_bounds().1
    }
}

impl std::fmt::Display for SimpleConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.operator, self.version)
    }
}
