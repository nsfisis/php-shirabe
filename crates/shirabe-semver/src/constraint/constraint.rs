//! ref: composer/vendor/composer/semver/src/Constraint/Constraint.php

use std::cell::RefCell;

use anyhow::bail;
use shirabe_php_shim as php;

use crate::constraint::bound::Bound;
use crate::constraint::constraint_interface::ConstraintInterface;

#[derive(Debug, Clone)]
pub struct Constraint {
    pub(crate) operator: i64,
    pub(crate) version: String,
    pub(crate) pretty_string: Option<String>,
    pub(crate) lower_bound: RefCell<Option<Bound>>,
    pub(crate) upper_bound: RefCell<Option<Bound>>,
}

impl Constraint {
    pub const OP_EQ: i64 = 0;
    pub const OP_LT: i64 = 1;
    pub const OP_LE: i64 = 2;
    pub const OP_GT: i64 = 3;
    pub const OP_GE: i64 = 4;
    pub const OP_NE: i64 = 5;

    pub const STR_OP_EQ: &'static str = "==";
    pub const STR_OP_EQ_ALT: &'static str = "=";
    pub const STR_OP_LT: &'static str = "<";
    pub const STR_OP_LE: &'static str = "<=";
    pub const STR_OP_GT: &'static str = ">";
    pub const STR_OP_GE: &'static str = ">=";
    pub const STR_OP_NE: &'static str = "!=";
    pub const STR_OP_NE_ALT: &'static str = "<>";

    fn trans_op_str(op: &str) -> Option<i64> {
        match op {
            "=" => Some(Self::OP_EQ),
            "==" => Some(Self::OP_EQ),
            "<" => Some(Self::OP_LT),
            "<=" => Some(Self::OP_LE),
            ">" => Some(Self::OP_GT),
            ">=" => Some(Self::OP_GE),
            "<>" => Some(Self::OP_NE),
            "!=" => Some(Self::OP_NE),
            _ => None,
        }
    }

    fn trans_op_int(op: i64) -> &'static str {
        match op {
            Self::OP_EQ => "==",
            Self::OP_LT => "<",
            Self::OP_LE => "<=",
            Self::OP_GT => ">",
            Self::OP_GE => ">=",
            Self::OP_NE => "!=",
            _ => panic!("unknown operator: {}", op),
        }
    }

    pub fn new(operator: String, version: String) -> anyhow::Result<Self> {
        let op_int = Self::trans_op_str(&operator).ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid operator \"{}\" given, expected one of: {}",
                operator,
                Self::get_supported_operators().join(", ")
            )
        })?;

        Ok(Self {
            operator: op_int,
            version,
            pretty_string: None,
            lower_bound: RefCell::new(None),
            upper_bound: RefCell::new(None),
        })
    }

    pub fn get_version(&self) -> &str {
        &self.version
    }

    pub fn get_operator(&self) -> &'static str {
        Self::trans_op_int(self.operator)
    }

    pub fn get_supported_operators() -> Vec<&'static str> {
        vec!["=", "==", "<", "<=", ">", ">=", "<>", "!="]
    }

    pub fn get_operator_constant(operator: &str) -> i64 {
        Self::trans_op_str(operator).expect("valid operator")
    }

    pub fn version_compare(
        &self,
        a: &str,
        b: &str,
        operator: &str,
        compare_branches: bool,
    ) -> anyhow::Result<bool> {
        if Self::trans_op_str(operator).is_none() {
            bail!(
                "Invalid operator \"{}\" given, expected one of: {}",
                operator,
                Self::get_supported_operators().join(", ")
            );
        }

        let a_is_branch = a.starts_with("dev-");
        let b_is_branch = b.starts_with("dev-");

        if operator == "!=" && (a_is_branch || b_is_branch) {
            return Ok(a != b);
        }

        if a_is_branch && b_is_branch {
            return Ok(operator == "==" && a == b);
        }

        if !compare_branches && (a_is_branch || b_is_branch) {
            return Ok(false);
        }

        Ok(php::version_compare(a, b, operator))
    }

    pub fn compile_constraint(&self, other_operator: i64) -> String {
        if self.version.starts_with("dev-") {
            if Self::OP_EQ == self.operator {
                if Self::OP_EQ == other_operator {
                    return format!(
                        "$b && $v === {}",
                        php::var_export_str(&self.version, true)
                    );
                }
                if Self::OP_NE == other_operator {
                    return format!(
                        "!$b || $v !== {}",
                        php::var_export_str(&self.version, true)
                    );
                }
                return "false".to_string();
            }

            if Self::OP_NE == self.operator {
                if Self::OP_EQ == other_operator {
                    return format!(
                        "!$b || $v !== {}",
                        php::var_export_str(&self.version, true)
                    );
                }
                if Self::OP_NE == other_operator {
                    return "true".to_string();
                }
                return "!$b".to_string();
            }

            return "false".to_string();
        }

        if Self::OP_EQ == self.operator {
            if Self::OP_EQ == other_operator {
                return format!(
                    "\\version_compare($v, {}, '==')",
                    php::var_export_str(&self.version, true)
                );
            }
            if Self::OP_NE == other_operator {
                return format!(
                    "$b || \\version_compare($v, {}, '!=')",
                    php::var_export_str(&self.version, true)
                );
            }
            return format!(
                "!$b && \\version_compare({}, $v, '{}')",
                php::var_export_str(&self.version, true),
                Self::trans_op_int(other_operator)
            );
        }

        if Self::OP_NE == self.operator {
            if Self::OP_EQ == other_operator {
                return format!(
                    "$b || (!$b && \\version_compare($v, {}, '!='))",
                    php::var_export_str(&self.version, true)
                );
            }
            if Self::OP_NE == other_operator {
                return "true".to_string();
            }
            return "!$b".to_string();
        }

        if Self::OP_LT == self.operator || Self::OP_LE == self.operator {
            if Self::OP_LT == other_operator || Self::OP_LE == other_operator {
                return "!$b".to_string();
            }
        } else if Self::OP_GT == other_operator || Self::OP_GE == other_operator {
            return "!$b".to_string();
        }

        if Self::OP_NE == other_operator {
            return "true".to_string();
        }

        let code_comparison = format!(
            "\\version_compare($v, {}, '{}')",
            php::var_export_str(&self.version, true),
            Self::trans_op_int(self.operator)
        );

        if self.operator == Self::OP_LE && other_operator == Self::OP_GT {
            return format!(
                "!$b && \\version_compare($v, {}, '!=') && {}",
                php::var_export_str(&self.version, true),
                code_comparison
            );
        }

        if self.operator == Self::OP_GE && other_operator == Self::OP_LT {
            return format!(
                "!$b && \\version_compare($v, {}, '!=') && {}",
                php::var_export_str(&self.version, true),
                code_comparison
            );
        }

        format!("!$b && {}", code_comparison)
    }

    pub fn match_specific(&self, provider: &Constraint, compare_branches: bool) -> bool {
        let no_equal_op = Self::trans_op_int(self.operator).replace('=', "");
        let provider_no_equal_op = Self::trans_op_int(provider.operator).replace('=', "");

        let is_equal_op = Self::OP_EQ == self.operator;
        let is_non_equal_op = Self::OP_NE == self.operator;
        let is_provider_equal_op = Self::OP_EQ == provider.operator;
        let is_provider_non_equal_op = Self::OP_NE == provider.operator;

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
            return self
                .version_compare(&provider.version, &self.version, "!=", compare_branches)
                .expect("valid operator");
        }

        if self.operator != Self::OP_EQ && no_equal_op == provider_no_equal_op {
            return !(self.version.starts_with("dev-") || provider.version.starts_with("dev-"));
        }

        let (version1, version2, operator) = if is_equal_op {
            (&self.version, &provider.version, provider.operator)
        } else {
            (&provider.version, &self.version, self.operator)
        };

        if self
            .version_compare(version1, version2, Self::trans_op_int(operator), compare_branches)
            .expect("valid operator")
        {
            return !(Self::trans_op_int(provider.operator) == provider_no_equal_op
                && Self::trans_op_int(self.operator) != no_equal_op
                && php::version_compare(&provider.version, &self.version, "=="));
        }

        false
    }

    fn extract_bounds(&self) {
        if self.lower_bound.borrow().is_some() {
            return;
        }

        if self.version.starts_with("dev-") {
            *self.lower_bound.borrow_mut() = Some(Bound::zero());
            *self.upper_bound.borrow_mut() = Some(Bound::positive_infinity());
            return;
        }

        let (lower, upper) = match self.operator {
            Self::OP_EQ => (
                Bound::new(self.version.clone(), true),
                Bound::new(self.version.clone(), true),
            ),
            Self::OP_LT => (Bound::zero(), Bound::new(self.version.clone(), false)),
            Self::OP_LE => (Bound::zero(), Bound::new(self.version.clone(), true)),
            Self::OP_GT => (
                Bound::new(self.version.clone(), false),
                Bound::positive_infinity(),
            ),
            Self::OP_GE => (
                Bound::new(self.version.clone(), true),
                Bound::positive_infinity(),
            ),
            Self::OP_NE => (Bound::zero(), Bound::positive_infinity()),
            _ => panic!("unknown operator: {}", self.operator),
        };

        *self.lower_bound.borrow_mut() = Some(lower);
        *self.upper_bound.borrow_mut() = Some(upper);
    }
}

impl ConstraintInterface for Constraint {
    fn matches(&self, provider: &dyn ConstraintInterface) -> bool {
        if let Some(p) = provider.as_any().downcast_ref::<Constraint>() {
            return self.match_specific(p, false);
        }
        provider.matches(self)
    }

    fn compile(&self, other_operator: i64) -> String {
        self.compile_constraint(other_operator)
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
        format!("{} {}", Self::trans_op_int(self.operator), self.version)
    }

    fn get_lower_bound(&self) -> Bound {
        self.extract_bounds();
        self.lower_bound
            .borrow()
            .clone()
            .expect("extract_bounds should have set lower_bound")
    }

    fn get_upper_bound(&self) -> Bound {
        self.extract_bounds();
        self.upper_bound
            .borrow()
            .clone()
            .expect("extract_bounds should have set upper_bound")
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
