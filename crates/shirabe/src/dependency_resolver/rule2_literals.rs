//! ref: composer/src/Composer/DependencyResolver/Rule2Literals.php

use shirabe_php_shim::PhpMixed;

use crate::dependency_resolver::generic_rule::RuleLiterals;
use crate::dependency_resolver::request::Request;
use crate::dependency_resolver::rule::{ReasonData, Rule};

#[derive(Debug)]
pub struct Rule2Literals {
    pub(crate) literal1: i64,
    pub(crate) literal2: i64,
    literals: Vec<i64>,
}

impl Rule2Literals {
    pub fn new(
        literal1: i64,
        literal2: i64,
        reason: shirabe_php_shim::PhpMixed,
        reason_data: shirabe_php_shim::PhpMixed,
    ) -> Self {
        let (literal1, literal2) = if literal1 < literal2 {
            (literal1, literal2)
        } else {
            (literal2, literal1)
        };

        Self {
            inner: Rule::new(reason, reason_data),
            literal1,
            literal2,
            literals: vec![literal1, literal2],
        }
    }

    pub fn get_hash(&self) -> String {
        format!("{},{}", self.literal1, self.literal2)
    }

    pub fn equals(&self, rule: &dyn RuleLiterals) -> bool {
        // PHP: specialized fast-case for instanceof self, then fallback to literal comparison
        // In Rust: use get_literals() for all cases (semantically equivalent)
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

    pub fn to_string(&self) -> String {
        let prefix = if self.inner.is_disabled() {
            "disabled("
        } else {
            "("
        };
        format!("{}{}|{})", prefix, self.literal1, self.literal2)
    }
}

impl RuleLiterals for Rule2Literals {
    fn get_literals(&self) -> &Vec<i64> {
        &self.literals
    }
}

impl Rule for Rule2Literals {
    fn bitfield(&self) -> i64 {
        todo!()
    }

    fn bitfield_mut(&mut self) -> &mut i64 {
        todo!()
    }

    fn request(&self) -> Option<&Request> {
        todo!()
    }

    fn request_mut(&mut self) -> Option<&mut Request> {
        todo!()
    }

    fn reason_data(&self) -> Option<&ReasonData> {
        todo!()
    }

    fn reason_data_mut(&mut self) -> Option<&mut ReasonData> {
        todo!()
    }

    fn get_literals(&self) -> Vec<i64> {
        todo!()
    }

    fn get_hash(&self) -> PhpMixed {
        todo!()
    }

    fn to_string(&self) -> String {
        todo!()
    }

    fn equals(&self, rule: &dyn Rule) -> bool {
        todo!()
    }

    fn is_assertion(&self) -> bool {
        todo!()
    }
}
