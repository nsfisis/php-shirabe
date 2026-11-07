//! ref: composer/src/Composer/DependencyResolver/Rule2Literals.php sha1:4e7a2f52b5a4abe8daa958d8593c91f1b8864afb

use crate::dependency_resolver::{ReasonData, Rule, RuleBase};
use bstr::BString;
use shirabe_php_shim::{Stringable, bconcat};

#[derive(Debug)]
pub struct Rule2Literals {
    inner: RuleBase,
    pub(crate) literal1: i64,
    pub(crate) literal2: i64,
}

impl Rule2Literals {
    pub fn new(literal1: i64, literal2: i64, reason: i64, reason_data: ReasonData) -> Self {
        let (literal1, literal2) = if literal1 < literal2 {
            (literal1, literal2)
        } else {
            (literal2, literal1)
        };

        Self {
            inner: RuleBase::new(reason, reason_data),
            literal1,
            literal2,
        }
    }

    pub(crate) fn base(&self) -> &RuleBase {
        &self.inner
    }

    pub(crate) fn base_mut(&mut self) -> &mut RuleBase {
        &mut self.inner
    }

    pub fn get_hash(&self) -> BString {
        bconcat!(self.literal1, ",", self.literal2)
    }

    pub fn equals(&self, rule: &Rule) -> bool {
        // PHP: specialized fast-case when `$rule instanceof self`.
        if let Rule::TwoLiterals(other) = rule {
            if self.literal1 != other.literal1 {
                return false;
            }
            if self.literal2 != other.literal2 {
                return false;
            }
            return true;
        }

        let literals = rule.get_literals();
        if literals.len() != 2 {
            return false;
        }
        if self.literal1 != literals[0] {
            return false;
        }
        if self.literal2 != literals[1] {
            return false;
        }
        true
    }

    pub fn is_assertion(&self) -> bool {
        false
    }
}

impl Stringable for Rule2Literals {
    fn to_php_string(&self) -> BString {
        let mut result = BString::from(if self.inner.is_disabled() {
            "disabled("
        } else {
            "("
        });

        result.extend_from_slice(&bconcat!(self.literal1, "|", self.literal2, ")"));

        result
    }
}
