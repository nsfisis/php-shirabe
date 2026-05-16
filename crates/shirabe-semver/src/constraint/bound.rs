//! ref: composer/vendor/composer/semver/src/Constraint/Bound.php

use anyhow::bail;

use shirabe_php_shim as php;

#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    version: String,
    is_inclusive: bool,
}

impl Bound {
    pub fn new(version: String, is_inclusive: bool) -> Self {
        Self {
            version,
            is_inclusive,
        }
    }

    pub fn get_version(&self) -> &str {
        &self.version
    }

    pub fn is_inclusive(&self) -> bool {
        self.is_inclusive
    }

    pub fn is_zero(&self) -> bool {
        self.get_version() == "0.0.0.0-dev" && self.is_inclusive()
    }

    pub fn is_positive_infinity(&self) -> bool {
        self.get_version() == format!("{}.0.0.0", i64::MAX) && !self.is_inclusive()
    }

    pub fn compare_to(&self, other: &Bound, operator: &str) -> anyhow::Result<bool> {
        if operator != "<" && operator != ">" {
            bail!("Does not support any other operator other than > or <.");
        }

        if self == other {
            return Ok(false);
        }

        let compare_result = php::version_compare_2(self.get_version(), other.get_version());

        if compare_result != 0 {
            return Ok((if operator == ">" { 1 } else { -1 }) == compare_result);
        }

        Ok(if operator == ">" {
            other.is_inclusive()
        } else {
            !other.is_inclusive()
        })
    }

    pub fn __to_string(&self) -> String {
        format!(
            "{} [{}]",
            self.get_version(),
            if self.is_inclusive() {
                "inclusive"
            } else {
                "exclusive"
            }
        )
    }

    pub fn zero() -> Self {
        Bound::new("0.0.0.0-dev".to_string(), true)
    }

    pub fn positive_infinity() -> Self {
        Bound::new(format!("{}.0.0.0", i64::MAX), false)
    }
}
