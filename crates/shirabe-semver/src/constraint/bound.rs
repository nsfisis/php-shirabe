//! ref: composer/vendor/composer/semver/src/Constraint/Bound.php sha1:7106e51a8b7f25dc36ed4550d0b3c0e337e3c94b

use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    InvalidArgumentException, PhpDisplay, bformat, bwrite, version_compare_ordering,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    version: BString,
    is_inclusive: bool,
}

impl Bound {
    pub fn new(version: impl Into<BString>, is_inclusive: bool) -> Self {
        Self {
            version: version.into(),
            is_inclusive,
        }
    }

    pub fn get_version(&self) -> &BStr {
        self.version.as_bstr()
    }

    pub fn is_inclusive(&self) -> bool {
        self.is_inclusive
    }

    pub fn is_zero(&self) -> bool {
        self.get_version() == "0.0.0.0-dev" && self.is_inclusive()
    }

    pub fn is_positive_infinity(&self) -> bool {
        self.get_version() == bformat!("{}.0.0.0", i64::MAX) && !self.is_inclusive()
    }

    pub fn compare_to(&self, other: &Bound, operator: impl AsRef<[u8]>) -> anyhow::Result<bool> {
        let operator = operator.as_ref();
        if operator != b"<" && operator != b">" {
            return Err(InvalidArgumentException::new(
                "Does not support any other operator other than > or <.",
            )
            .into());
        }

        if self == other {
            return Ok(false);
        }

        let compare_result =
            version_compare_ordering(self.get_version(), other.get_version()) as i8;

        if compare_result != 0 {
            return Ok((if operator == b">" { 1 } else { -1 }) == compare_result);
        }

        Ok(if operator == b">" {
            other.is_inclusive()
        } else {
            !other.is_inclusive()
        })
    }

    pub fn zero() -> Self {
        Bound::new("0.0.0.0-dev", true)
    }

    pub fn positive_infinity() -> Self {
        Bound::new(bformat!("{}.0.0.0", i64::MAX), false)
    }
}

impl PhpDisplay for Bound {
    fn fmt(&self, out: &mut BString) {
        bwrite!(
            out,
            "{}{}",
            self.get_version(),
            if self.is_inclusive() {
                " [inclusive]"
            } else {
                " [exclusive]"
            }
        );
    }
}
