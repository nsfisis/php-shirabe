//! ref: composer/src/Composer/DependencyResolver/GenericRule.php

use crate::dependency_resolver::rule::{Rule, RuleBase};
use anyhow::Result;
use shirabe_php_shim::{PHP_VERSION_ID, PhpMixed, RuntimeException, hash_raw, implode, unpack};

use super::{request::Request, rule::ReasonData};

pub struct GenericRule {
    inner: RuleBase,
    pub(crate) literals: Vec<i64>,
}

impl GenericRule {
    pub fn new(mut literals: Vec<i64>, reason: PhpMixed, reason_data: PhpMixed) -> Self {
        let inner = RuleBase::new(reason, reason_data);
        literals.sort();
        Self { inner, literals }
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

    pub fn equals(&self, rule: &dyn RuleLiterals) -> bool {
        self.literals == *rule.get_literals()
    }

    pub fn is_assertion(&self) -> bool {
        self.literals.len() == 1
    }
}

pub trait RuleLiterals {
    fn get_literals(&self) -> &Vec<i64>;
    fn is_multi_conflict_rule(&self) -> bool {
        false
    }
}

impl RuleLiterals for GenericRule {
    fn get_literals(&self) -> &Vec<i64> {
        &self.literals
    }
}

impl Rule for GenericRule {
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

    fn equals(&self, rule: &dyn Rule) -> bool {
        todo!()
    }

    fn is_assertion(&self) -> bool {
        todo!()
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
