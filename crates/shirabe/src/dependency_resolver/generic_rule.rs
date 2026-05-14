//! ref: composer/src/Composer/DependencyResolver/GenericRule.php

use anyhow::Result;
use shirabe_php_shim::{hash_raw, implode, unpack, RuntimeException, PHP_VERSION_ID};
use crate::dependency_resolver::rule::Rule;

pub struct GenericRule {
    inner: Rule,
    pub(crate) literals: Vec<i64>,
}

impl GenericRule {
    pub fn new(mut literals: Vec<i64>, reason: shirabe_php_shim::PhpMixed, reason_data: shirabe_php_shim::PhpMixed) -> Self {
        let inner = Rule::new(reason, reason_data);
        literals.sort();
        Self { inner, literals }
    }

    pub fn get_literals(&self) -> &Vec<i64> {
        &self.literals
    }

    pub fn get_hash(&self) -> Result<i64> {
        let joined = self.literals.iter().map(|l| l.to_string()).collect::<Vec<_>>().join(",");
        let algo = if PHP_VERSION_ID > 80100 { "xxh3" } else { "sha1" };
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
                    }.into())
                }
            }
            None => Err(RuntimeException {
                message: format!("Failed unpacking: {}", joined),
                code: 0,
            }.into()),
        }
    }

    pub fn equals(&self, rule: &dyn RuleLiterals) -> bool {
        self.literals == *rule.get_literals()
    }

    pub fn is_assertion(&self) -> bool {
        self.literals.len() == 1
    }

    pub fn to_string(&self) -> String {
        let prefix = if self.inner.is_disabled() { "disabled(" } else { "(" };
        let mut result = prefix.to_string();
        for (i, literal) in self.literals.iter().enumerate() {
            if i != 0 {
                result.push('|');
            }
            result.push_str(&literal.to_string());
        }
        result.push(')');
        result
    }
}

pub trait RuleLiterals {
    fn get_literals(&self) -> &Vec<i64>;
}

impl RuleLiterals for GenericRule {
    fn get_literals(&self) -> &Vec<i64> {
        &self.literals
    }
}
