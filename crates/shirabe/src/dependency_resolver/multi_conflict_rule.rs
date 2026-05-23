//! ref: composer/src/Composer/DependencyResolver/MultiConflictRule.php

use crate::dependency_resolver::{ReasonData, Rule, RuleBase};
use anyhow::Result;
use shirabe_php_shim::{PHP_VERSION_ID, PhpMixed, RuntimeException, hash_raw};

#[derive(Debug)]
pub struct MultiConflictRule {
    inner: RuleBase,
    pub(crate) literals: Vec<i64>,
}

impl MultiConflictRule {
    pub fn new(mut literals: Vec<i64>, reason: PhpMixed, reason_data: PhpMixed) -> Result<Self> {
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
            inner: RuleBase::new(reason.as_int().unwrap_or(0), ReasonData::from(reason_data)),
            literals,
        })
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

    pub fn equals(&self, rule: &Rule) -> bool {
        if let Rule::MultiConflict(other) = rule {
            self.literals == other.literals
        } else {
            false
        }
    }

    pub fn is_assertion(&self) -> bool {
        false
    }
}

impl std::fmt::Display for MultiConflictRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO multi conflict?
        write!(
            f,
            "{}",
            if self.inner.is_disabled() {
                "disabled(multi("
            } else {
                "(multi("
            }
        )?;

        for (i, literal) in self.literals.iter().enumerate() {
            if i != 0 {
                write!(f, "|")?;
            }
            write!(f, "{}", literal)?;
        }
        write!(f, "))")
    }
}
