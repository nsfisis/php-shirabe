//! ref: composer/src/Composer/Package/Link.php

use shirabe_semver::constraint::AnyConstraint;

use crate::package::PackageInterfaceHandle;

pub struct Link {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) constraint: AnyConstraint,
    pub(crate) description: String,
    pub(crate) pretty_constraint: String,
}

impl Clone for Link {
    fn clone(&self) -> Self {
        // TODO(phase-b): Link is a PHP class; this clone is a shallow placeholder until
        // Link is shared via Rc<Link>.
        Self {
            source: self.source.clone(),
            target: self.target.clone(),
            constraint: self.constraint.clone(),
            description: self.description.clone(),
            pretty_constraint: self.pretty_constraint.clone(),
        }
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Link")
            .field("source", &self.source)
            .field("target", &self.target)
            .field("description", &self.description)
            .field("pretty_constraint", &self.pretty_constraint)
            .finish()
    }
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
        source: String,
        target: String,
        constraint: AnyConstraint,
        description: Option<String>,
        pretty_constraint: String,
    ) -> Self {
        let description = description.unwrap_or_else(|| Self::TYPE_UNKNOWN.to_string());
        let description = if description == Self::TYPE_DEV_REQUIRE {
            "requires (for development)".to_string()
        } else {
            description
        };
        Self {
            source: source.to_lowercase(),
            target: target.to_lowercase(),
            constraint,
            description,
            pretty_constraint,
        }
    }

    pub fn get_description(&self) -> &str {
        &self.description
    }

    pub fn get_source(&self) -> &str {
        &self.source
    }

    pub fn get_target(&self) -> &str {
        &self.target
    }

    pub fn get_constraint(&self) -> &AnyConstraint {
        &self.constraint
    }

    pub fn get_pretty_constraint(&self) -> &str {
        &self.pretty_constraint
    }

    pub fn to_string(&self) -> String {
        format!(
            "{} {} {} ({})",
            self.source,
            self.description,
            self.target,
            self.constraint.to_string(),
        )
    }

    pub fn get_pretty_string(&self, source_package: PackageInterfaceHandle) -> String {
        format!(
            "{} {} {} {}",
            source_package.get_pretty_string(),
            self.description,
            self.target,
            self.constraint.get_pretty_string()
        )
    }
}
