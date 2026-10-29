//! ref: composer/src/Composer/Package/Link.php

use crate::package::PackageInterfaceHandle;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{bconcat, strtolower};
use shirabe_semver::constraint::AnyConstraint;

#[derive(Clone, Debug)]
pub struct Link {
    source: BString,
    target: BString,
    constraint: AnyConstraint,
    description: BString,
    pretty_constraint: BString,
}

impl Link {
    pub const TYPE_REQUIRE: &'static str = "requires";
    pub const TYPE_DEV_REQUIRE: &'static str = "devRequires";
    pub const TYPE_PROVIDE: &'static str = "provides";
    pub const TYPE_CONFLICT: &'static str = "conflicts";
    pub const TYPE_REPLACE: &'static str = "replaces";

    /// Special type
    pub const TYPE_DOES_NOT_REQUIRE: &'static str = "does not require";

    const TYPE_UNKNOWN: &'static str = "relates to";

    pub fn types() -> Vec<&'static str> {
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
        pretty_constraint: BString,
    ) -> Self {
        let description = description.unwrap_or_else(|| Self::TYPE_UNKNOWN.into());
        let description = if description == Self::TYPE_DEV_REQUIRE {
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

    pub fn get_pretty_constraint(&self) -> &BStr {
        self.pretty_constraint.as_bstr()
    }

    pub fn get_pretty_string(&self, source_package: PackageInterfaceHandle) -> BString {
        bconcat!(
            source_package.get_pretty_string(),
            " ",
            self.description,
            " ",
            self.target,
            " ",
            self.constraint.get_pretty_string()
        )
    }

    /// PHP's `__toString()`.
    pub fn to_php_string(&self) -> BString {
        bconcat!(
            self.source,
            " ",
            self.description,
            " ",
            self.target,
            " (",
            self.constraint.to_php_string(),
            ")"
        )
    }
}
