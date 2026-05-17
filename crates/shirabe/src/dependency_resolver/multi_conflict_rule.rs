//! ref: composer/src/Composer/DependencyResolver/MultiConflictRule.php

use shirabe_php_shim::PhpMixed;

use crate::dependency_resolver::generic_rule::RuleLiterals;
use crate::dependency_resolver::request::Request;
use crate::dependency_resolver::rule::{ReasonData, Rule};
use anyhow::Result;
use shirabe_php_shim::{PHP_VERSION_ID, RuntimeException, hash_raw};

#[derive(Debug)]
pub struct MultiConflictRule {
    pub(crate) literals: Vec<i64>,
}

impl MultiConflictRule {
    pub fn new(
        mut literals: Vec<i64>,
        reason: shirabe_php_shim::PhpMixed,
        reason_data: shirabe_php_shim::PhpMixed,
    ) -> Result<Self> {
        if literals.len() < 3 {
            return Err(RuntimeException {
                message: "multi conflict rule requires at least 3 literals".to_string(),
                code: 0,
            }
            .into());
        }

        // sort all packages ascending by id
        literals.sort();

        Ok(Self {
            inner: Rule::new(reason, reason_data),
            literals,
        })
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
        let binary = hash_raw(algo, &format!("c:{}", joined));
        let data = shirabe_php_shim::unpack("ihash", &binary);
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
        // PHP: if ($rule instanceof MultiConflictRule) { ... } return false;
        // Phase A: instanceof check not representable via RuleLiterals trait; literals-only comparison used
        self.literals == *rule.get_literals()
    }

    pub fn is_assertion(&self) -> bool {
        false
    }

    pub fn disable(&mut self) -> Result<()> {
        Err(RuntimeException {
            message: "Disabling multi conflict rules is not possible. Please contact composer at https://github.com/composer/composer to let us debug what lead to this situation.".to_string(),
            code: 0,
        }.into())
    }

    pub fn to_string(&self) -> String {
        let mut result = if self.inner.is_disabled() {
            "disabled(multi(".to_string()
        } else {
            "(multi(".to_string()
        };

        // TODO multi conflict?
        for (i, literal) in self.literals.iter().enumerate() {
            if i != 0 {
                result.push('|');
            }
            result.push_str(&literal.to_string());
        }

        result.push_str("))");
        result
    }
}

impl RuleLiterals for MultiConflictRule {
    fn get_literals(&self) -> &Vec<i64> {
        &self.literals
    }

    fn is_multi_conflict_rule(&self) -> bool {
        true
    }
}

impl Rule for MultiConflictRule {
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
