//! ref: composer/src/Composer/DependencyResolver/GenericRule.php

use super::rule::ReasonData;
use crate::dependency_resolver::{Rule, RuleBase};
use shirabe_php_shim::{RuntimeException, hash_raw};

#[derive(Debug)]
pub struct GenericRule {
    inner: RuleBase,
    literals: Vec<i64>,
}

impl GenericRule {
    pub fn new(mut literals: Vec<i64>, reason: i64, reason_data: ReasonData) -> Self {
        let inner = RuleBase::new(reason, reason_data);
        literals.sort();
        Self { inner, literals }
    }

    pub(crate) fn base(&self) -> &RuleBase {
        &self.inner
    }

    pub(crate) fn base_mut(&mut self) -> &mut RuleBase {
        &mut self.inner
    }

    pub fn get_literals(&self) -> &Vec<i64> {
        &self.literals
    }

    pub fn get_hash(&self) -> anyhow::Result<i64> {
        let joined = self
            .literals
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let binary = hash_raw("xxh3", &joined);
        match binary.get(..4) {
            Some(chunk) => Ok(i32::from_ne_bytes(chunk.try_into().unwrap()) as i64),
            None => Err(RuntimeException::new(format!("Failed unpacking: {}", joined)).into()),
        }
    }

    pub fn equals(&self, rule: &Rule) -> bool {
        self.literals == rule.get_literals()
    }

    pub fn is_assertion(&self) -> bool {
        self.literals.len() == 1
    }
}

impl std::fmt::Display for GenericRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            if self.inner.is_disabled() {
                "disabled("
            } else {
                "("
            }
        )?;

        for (i, literal) in self.literals.iter().enumerate() {
            if i != 0 {
                write!(f, "|")?;
            }
            write!(f, "{}", literal)?;
        }
        write!(f, ")")
    }
}
