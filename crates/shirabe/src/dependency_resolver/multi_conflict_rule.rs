//! ref: composer/src/Composer/DependencyResolver/MultiConflictRule.php

use crate::dependency_resolver::{ReasonData, Rule, RuleBase};
use shirabe_php_shim::{PHP_VERSION_ID, RuntimeException, hash_raw};

#[derive(Debug)]
pub struct MultiConflictRule {
    inner: RuleBase,
    pub(crate) literals: Vec<i64>,
}

impl MultiConflictRule {
    pub fn new(
        mut literals: Vec<i64>,
        reason: i64,
        reason_data: ReasonData,
    ) -> anyhow::Result<Self> {
        if literals.len() < 3 {
            return Err(RuntimeException::new(
                "multi conflict rule requires at least 3 literals".to_string(),
            )
            .into());
        }

        // sort all packages ascending by id
        literals.sort();

        Ok(Self {
            inner: RuleBase::new(reason, reason_data),
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

    pub fn get_hash(&self) -> anyhow::Result<i64> {
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
        match binary.get(..4) {
            Some(chunk) => Ok(i32::from_ne_bytes(chunk.try_into().unwrap()) as i64),
            None => Err(RuntimeException::new(format!("Failed unpacking: {}", joined)).into()),
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
