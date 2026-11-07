//! ref: composer/src/Composer/DependencyResolver/MultiConflictRule.php sha1:5d81b33b93543b78d3419ac8bb4247a556dbb4cb

use crate::dependency_resolver::{ReasonData, Rule, RuleBase};
use bstr::BString;
use shirabe_php_shim::{PhpDisplay, RuntimeException, ToPhpString, bformat, hash_raw, implode};

#[derive(Debug)]
pub struct MultiConflictRule {
    inner: RuleBase,
    literals: Vec<i64>,
}

impl MultiConflictRule {
    pub fn new(
        mut literals: Vec<i64>,
        reason: i64,
        reason_data: ReasonData,
    ) -> anyhow::Result<Self> {
        if literals.len() < 3 {
            return Err(RuntimeException::new(BString::from(
                "multi conflict rule requires at least 3 literals",
            ))
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
        let joined = implode(
            ",",
            &self
                .literals
                .iter()
                .map(|l| l.to_php_string())
                .collect::<Vec<_>>(),
        );
        let binary = hash_raw("xxh3", bformat!("c:{joined}"));
        match binary.get(..4) {
            Some(chunk) => Ok(i32::from_ne_bytes(chunk.try_into().unwrap()) as i64),
            None => Err(RuntimeException::new(bformat!("Failed unpacking: {joined}")).into()),
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

impl PhpDisplay for MultiConflictRule {
    fn fmt(&self, out: &mut BString) {
        // TODO multi conflict?
        out.extend_from_slice(if self.inner.is_disabled() {
            b"disabled(multi("
        } else {
            b"(multi("
        });

        for (i, literal) in self.literals.iter().enumerate() {
            if i != 0 {
                out.push(b'|');
            }
            literal.fmt(out);
        }

        out.extend_from_slice(b"))");
    }
}
