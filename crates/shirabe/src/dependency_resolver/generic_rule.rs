//! ref: composer/src/Composer/DependencyResolver/GenericRule.php

use crate::dependency_resolver::{Rule, RuleBase};
use anyhow::Result;
use shirabe_php_shim::{PHP_VERSION_ID, RuntimeException, hash_raw, unpack};

use super::rule::ReasonData;

#[derive(Debug)]
pub struct GenericRule {
    inner: RuleBase,
    pub(crate) literals: Vec<i64>,
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

    pub fn get_hash(&self) -> Result<i64> {
        let joined = self
            .literals
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let algo = if PHP_VERSION_ID > 80100 {
            "xxh3"
        } else {
            "sha1"
        };
        let binary = hash_raw(algo, &joined);
        let data = unpack("ihash", &binary);
        match data {
            Some(map) => {
                if let Some(val) = map.get("hash") {
                    Ok(val.as_int().unwrap_or(0))
                } else {
                    Err(RuntimeException {
                        message: format!("Failed unpacking: {}", joined),
                        code: 0,
                    }
                    .into())
                }
            }
            None => Err(RuntimeException {
                message: format!("Failed unpacking: {}", joined),
                code: 0,
            }
            .into()),
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
