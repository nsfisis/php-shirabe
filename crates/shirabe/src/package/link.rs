//! ref: composer/src/Composer/Package/Link.php sha1:85708b20ed9db407c25ea041133020dd22222b01

use crate::package::PackageInterfaceHandle;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    PhpDisplay, ToPhpString, UnexpectedValueException, bformat, bwrite, php_str, strtolower,
};
use shirabe_semver::constraint::AnyConstraint;

#[derive(Clone, Debug)]
pub struct Link {
    source: BString,
    target: BString,
    constraint: AnyConstraint,
    description: BString,
    pretty_constraint: Option<BString>,
}

impl Link {
    pub const TYPE_REQUIRE: &'static BStr = php_str(b"requires");
    pub const TYPE_DEV_REQUIRE: &'static BStr = php_str(b"devRequires");
    pub const TYPE_PROVIDE: &'static BStr = php_str(b"provides");
    pub const TYPE_CONFLICT: &'static BStr = php_str(b"conflicts");
    pub const TYPE_REPLACE: &'static BStr = php_str(b"replaces");

    /// Special type
    pub const TYPE_DOES_NOT_REQUIRE: &'static BStr = php_str(b"does not require");

    const TYPE_UNKNOWN: &'static BStr = php_str(b"relates to");

    pub fn types() -> Vec<&'static BStr> {
        vec![
            Self::TYPE_REQUIRE,
            Self::TYPE_DEV_REQUIRE,
            Self::TYPE_PROVIDE,
            Self::TYPE_CONFLICT,
            Self::TYPE_REPLACE,
        ]
    }

    pub fn new(
        source: BString,
        target: BString,
        constraint: AnyConstraint,
        description: Option<BString>,
        pretty_constraint: Option<BString>,
    ) -> Self {
        let description = description.unwrap_or_else(|| Self::TYPE_UNKNOWN.into());
        let description = if description.as_bstr() == Self::TYPE_DEV_REQUIRE {
            "requires (for development)".into()
        } else {
            description
        };
        Self {
            source: strtolower(&source),
            target: strtolower(&target),
            constraint,
            description,
            pretty_constraint,
        }
    }

    pub fn get_description(&self) -> &BStr {
        self.description.as_bstr()
    }

    pub fn get_source(&self) -> &BStr {
        self.source.as_bstr()
    }

    pub fn get_target(&self) -> &BStr {
        self.target.as_bstr()
    }

    pub fn get_constraint(&self) -> &AnyConstraint {
        &self.constraint
    }

    pub fn get_pretty_constraint(&self) -> anyhow::Result<&BStr> {
        match &self.pretty_constraint {
            Some(pretty_constraint) => Ok(pretty_constraint.as_bstr()),
            None => Err(UnexpectedValueException::new(bformat!(
                "Link {} has been misconfigured and had no prettyConstraint given.",
                self.to_php_string()
            ))
            .into()),
        }
    }

    pub fn get_pretty_constraint_for_serialization(&self) -> Option<&BStr> {
        self.pretty_constraint.as_ref().map(|s| s.as_bstr())
    }

    pub fn get_pretty_string(&self, source_package: PackageInterfaceHandle) -> BString {
        bformat!(
            "{} {} {} {}",
            source_package.get_pretty_string(),
            self.description,
            self.target,
            self.constraint.get_pretty_string()
        )
    }
}

impl PhpDisplay for Link {
    fn fmt(&self, out: &mut BString) {
        bwrite!(
            out,
            "{} {} {} ({})",
            self.source,
            self.description,
            self.target,
            self.constraint
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_php_shim::Catch;
    use shirabe_semver::constraint::MatchAllConstraint;

    #[test]
    fn get_pretty_constraint_returns_the_given_pretty_constraint() {
        let link = Link::new(
            BString::from("a/a"),
            BString::from("b/b"),
            MatchAllConstraint::new(None).into(),
            None,
            Some(BString::from("^1.0")),
        );
        assert_eq!("^1.0", link.get_pretty_constraint().unwrap());
    }

    #[test]
    fn get_pretty_constraint_throws_without_a_pretty_constraint() {
        let link = Link::new(
            BString::from("a/a"),
            BString::from("b/b"),
            MatchAllConstraint::new(None).into(),
            None,
            None,
        );
        let error = link.get_pretty_constraint().unwrap_err();
        let exception = error
            .catch::<UnexpectedValueException>()
            .expect("an UnexpectedValueException");
        assert_eq!(
            "Link a/a relates to b/b (*) has been misconfigured and had no prettyConstraint given.",
            exception.get_message()
        );
    }
}
