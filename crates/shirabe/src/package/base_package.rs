//! ref: composer/src/Composer/Package/BasePackage.php

use std::sync::LazyLock;

use indexmap::IndexMap;
use shirabe_php_shim::{LogicException, UnexpectedValueException, preg_quote};

use crate::package::Link;
use crate::package::PackageInterface;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;

pub struct SupportedLinkType {
    pub description: &'static str,
    pub method: &'static str,
}

pub static SUPPORTED_LINK_TYPES: LazyLock<IndexMap<&'static str, SupportedLinkType>> =
    LazyLock::new(|| {
        let mut m = IndexMap::new();
        m.insert(
            "require",
            SupportedLinkType {
                description: "requires",
                method: Link::TYPE_REQUIRE,
            },
        );
        m.insert(
            "conflict",
            SupportedLinkType {
                description: "conflicts",
                method: Link::TYPE_CONFLICT,
            },
        );
        m.insert(
            "provide",
            SupportedLinkType {
                description: "provides",
                method: Link::TYPE_PROVIDE,
            },
        );
        m.insert(
            "replace",
            SupportedLinkType {
                description: "replaces",
                method: Link::TYPE_REPLACE,
            },
        );
        m.insert(
            "require-dev",
            SupportedLinkType {
                description: "requires (for development)",
                method: Link::TYPE_DEV_REQUIRE,
            },
        );
        m
    });

pub static STABILITIES: LazyLock<IndexMap<&'static str, i64>> = LazyLock::new(|| {
    let mut m = IndexMap::new();
    m.insert("stable", 0i64);
    m.insert("RC", 5i64);
    m.insert("beta", 10i64);
    m.insert("alpha", 15i64);
    m.insert("dev", 20i64);
    m
});

pub const STABILITY_STABLE: i64 = 0;
pub const STABILITY_RC: i64 = 5;
pub const STABILITY_BETA: i64 = 10;
pub const STABILITY_ALPHA: i64 = 15;
pub const STABILITY_DEV: i64 = 20;

pub trait BasePackage: PackageInterface + std::fmt::Display {
    fn id(&self) -> i64;
    fn id_mut(&mut self) -> &mut i64;
    fn name(&self) -> &str;
    fn name_mut(&mut self) -> &mut String;
    fn pretty_name(&self) -> &str;
    fn pretty_name_mut(&mut self) -> &mut String;
    fn repository_opt(&self) -> Option<&dyn RepositoryInterface>;
    fn set_repository_box(&mut self, repository: Box<dyn RepositoryInterface>);
    fn take_repository(&mut self) -> Option<Box<dyn RepositoryInterface>>;

    /// PHP `setRepository($this)` from the containing repository — Rust port marker until
    /// the borrow story for repository-package back-references is finalized in phase B.
    fn set_repository_self(&mut self) {
        // TODO(phase-b): wire up a back-reference to the containing repository when needed.
    }

    // as_alias_package / as_complete_package_interface inherited from PackageInterface.

    fn as_alias_package_mut(&mut self) -> Option<&mut crate::package::AliasPackage> {
        None
    }

    // get_name / get_pretty_name / get_names live on PackageInterface; the BasePackage
    // duplicates were causing ambiguity at every call site (`pkg.get_name()` with
    // pkg: &dyn BasePackage). Concrete impls already forward to name()/pretty_name().

    // set_id, get_id, get_repository, get_unique_name, set_repository are inherited
    // from PackageInterface; do not redeclare here to avoid trait-method ambiguity.

    fn is_platform(&self) -> bool {
        self.repository_opt()
            .and_then(|r| r.as_any().downcast_ref::<PlatformRepository>())
            .is_some()
    }

    fn equals(&self, _package: &dyn PackageInterface) -> bool {
        // TODO(phase-b): implement via reference identity (requires Rc/Arc)
        // PHP uses === which is reference equality; unwraps AliasPackage on both sides
        todo!("equals requires reference identity which needs Rc/Arc")
    }

    // get_pretty_string is inherited from PackageInterface.

    fn get_full_pretty_version(&self, truncate: bool, display_mode: i64) -> anyhow::Result<String> {
        const DISPLAY_SOURCE_REF_IF_DEV: i64 = <dyn PackageInterface>::DISPLAY_SOURCE_REF_IF_DEV;
        const DISPLAY_SOURCE_REF: i64 = <dyn PackageInterface>::DISPLAY_SOURCE_REF;
        const DISPLAY_DIST_REF: i64 = <dyn PackageInterface>::DISPLAY_DIST_REF;

        if display_mode == DISPLAY_SOURCE_REF_IF_DEV
            && (!self.is_dev()
                || (!["hg", "git"].contains(&self.get_source_type().unwrap_or_default())
                    && (self.get_source_type().unwrap_or_default() != ""
                        || self.get_dist_reference().unwrap_or_default() == "")))
        {
            return Ok(self.get_pretty_version().to_string());
        }

        let reference: Option<&str> = match display_mode {
            DISPLAY_SOURCE_REF_IF_DEV => {
                if self.get_source_reference().unwrap_or_default() != "" {
                    self.get_source_reference()
                } else {
                    self.get_dist_reference()
                }
            }
            DISPLAY_SOURCE_REF => self.get_source_reference(),
            DISPLAY_DIST_REF => self.get_dist_reference(),
            _ => {
                return Err(anyhow::anyhow!(UnexpectedValueException {
                    message: format!("Display mode {} is not supported", display_mode),
                    code: 0,
                }));
            }
        };

        let reference = match reference {
            None => return Ok(self.get_pretty_version().to_string()),
            Some(r) => r,
        };

        if truncate && reference.len() == 40 && self.get_source_type() != Some("svn") {
            return Ok(format!("{} {}", self.get_pretty_version(), &reference[..7]));
        }

        Ok(format!("{} {}", self.get_pretty_version(), reference))
    }

    fn get_stability_priority(&self) -> i64 {
        *STABILITIES
            .get(self.get_stability())
            .unwrap_or(&STABILITY_STABLE)
    }

    fn php_clone(&mut self) {
        self.take_repository();
        *self.id_mut() = -1;
    }
}

pub fn package_name_to_regexp(allow_pattern: &str) -> String {
    package_name_to_regexp2(allow_pattern, "{^%s$}i")
}

pub fn package_name_to_regexp2(allow_pattern: &str, wrap: &str) -> String {
    let cleaned = preg_quote(allow_pattern, None).replace("\\*", ".*");
    wrap.replace("%s", &cleaned)
}

pub fn package_names_to_regexp(package_names: &[String], wrap: &str) -> String {
    let patterns: Vec<String> = package_names
        .iter()
        .map(|name| package_name_to_regexp2(name, "%s"))
        .collect();
    wrap.replace("%s", &patterns.join("|"))
}
