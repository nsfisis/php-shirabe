//! ref: composer/src/Composer/Package/BasePackage.php

use std::sync::LazyLock;

use indexmap::IndexMap;
use shirabe_php_shim::{preg_quote, LogicException, UnexpectedValueException};

use crate::package::link::Link;
use crate::package::package_interface::PackageInterface;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_interface::RepositoryInterface;

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
    m.insert("stable", BasePackage::STABILITY_STABLE);
    m.insert("RC", BasePackage::STABILITY_RC);
    m.insert("beta", BasePackage::STABILITY_BETA);
    m.insert("alpha", BasePackage::STABILITY_ALPHA);
    m.insert("dev", BasePackage::STABILITY_DEV);
    m
});

#[derive(Debug)]
pub struct BasePackage {
    pub id: i64,
    pub(crate) name: String,
    pub(crate) pretty_name: String,
    pub(crate) repository: Option<Box<dyn RepositoryInterface>>,
}

impl std::fmt::Display for BasePackage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_unique_name())
    }
}

impl BasePackage {
    pub const STABILITY_STABLE: i64 = 0;
    pub const STABILITY_RC: i64 = 5;
    pub const STABILITY_BETA: i64 = 10;
    pub const STABILITY_ALPHA: i64 = 15;
    pub const STABILITY_DEV: i64 = 20;

    pub fn new(name: String) -> Self {
        let pretty_name = name.clone();
        let name = name.to_lowercase();
        Self {
            id: -1,
            name,
            pretty_name,
            repository: None,
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_pretty_name(&self) -> &str {
        &self.pretty_name
    }

    pub fn get_names(&self, provides: bool) -> Vec<String> {
        let mut names: IndexMap<String, bool> = IndexMap::new();
        names.insert(self.get_name().to_string(), true);

        if provides {
            for link in self.get_provides().values() {
                names.insert(link.get_target().to_string(), true);
            }
        }

        for link in self.get_replaces().values() {
            names.insert(link.get_target().to_string(), true);
        }

        names.into_keys().collect()
    }

    pub fn set_id(&mut self, id: i64) {
        self.id = id;
    }

    pub fn get_id(&self) -> i64 {
        self.id
    }

    pub fn set_repository(
        &mut self,
        repository: Box<dyn RepositoryInterface>,
    ) -> anyhow::Result<()> {
        if let Some(ref existing) = self.repository {
            // TODO(phase-b): proper reference identity check before raising error
            return Err(anyhow::anyhow!(LogicException {
                message: format!(
                    "Package \"{}\" cannot be added to repository \"{}\" as it is already in repository \"{}\".",
                    self.get_pretty_name(),
                    repository.get_repo_name(),
                    existing.get_repo_name(),
                ),
                code: 0,
            }));
        }
        self.repository = Some(repository);
        Ok(())
    }

    pub fn get_repository(&self) -> Option<&dyn RepositoryInterface> {
        self.repository.as_deref()
    }

    pub fn is_platform(&self) -> bool {
        self.repository
            .as_ref()
            .and_then(|r| r.as_any().downcast_ref::<PlatformRepository>())
            .is_some()
    }

    pub fn get_unique_name(&self) -> String {
        format!("{}-{}", self.get_name(), self.get_version())
    }

    pub fn equals(&self, _package: &dyn PackageInterface) -> bool {
        // TODO(phase-b): implement via reference identity (requires Rc/Arc)
        // PHP uses === which is reference equality; unwraps AliasPackage on both sides
        todo!("equals requires reference identity which needs Rc/Arc")
    }

    pub fn get_pretty_string(&self) -> String {
        format!("{} {}", self.get_pretty_name(), self.get_pretty_version())
    }

    pub fn get_full_pretty_version(
        &self,
        truncate: bool,
        display_mode: i64,
    ) -> anyhow::Result<String> {
        const DISPLAY_SOURCE_REF_IF_DEV: i64 = PackageInterface::DISPLAY_SOURCE_REF_IF_DEV;
        const DISPLAY_SOURCE_REF: i64 = PackageInterface::DISPLAY_SOURCE_REF;
        const DISPLAY_DIST_REF: i64 = PackageInterface::DISPLAY_DIST_REF;

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

    pub fn get_stability_priority(&self) -> i64 {
        *STABILITIES
            .get(self.get_stability())
            .unwrap_or(&Self::STABILITY_STABLE)
    }

    pub fn php_clone(&mut self) {
        self.repository = None;
        self.id = -1;
    }

    pub fn package_name_to_regexp(allow_pattern: &str, wrap: &str) -> String {
        let cleaned = preg_quote(allow_pattern, None).replace("\\*", ".*");
        wrap.replace("%s", &cleaned)
    }

    pub fn package_names_to_regexp(package_names: &[String], wrap: &str) -> String {
        let patterns: Vec<String> = package_names
            .iter()
            .map(|name| Self::package_name_to_regexp(name, "%s"))
            .collect();
        wrap.replace("%s", &patterns.join("|"))
    }

    // Methods below are defined in Package/CompletePackage subclasses in PHP.
    // Called via $this polymorphism from BasePackage methods.
    // TODO(phase-b): resolve via trait dispatch or field access in concrete types.

    pub fn get_provides(&self) -> IndexMap<String, Link> {
        todo!("defined in Package subclass")
    }

    pub fn get_replaces(&self) -> IndexMap<String, Link> {
        todo!("defined in Package subclass")
    }

    pub fn get_version(&self) -> &str {
        todo!("defined in Package subclass")
    }

    pub fn get_pretty_version(&self) -> &str {
        todo!("defined in Package subclass")
    }

    pub fn is_dev(&self) -> bool {
        todo!("defined in Package subclass")
    }

    pub fn get_source_type(&self) -> Option<&str> {
        todo!("defined in Package subclass")
    }

    pub fn get_source_reference(&self) -> Option<&str> {
        todo!("defined in Package subclass")
    }

    pub fn get_dist_reference(&self) -> Option<&str> {
        todo!("defined in Package subclass")
    }

    pub fn get_stability(&self) -> &str {
        todo!("defined in Package subclass")
    }

    pub fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    pub fn clone_box(&self) -> Box<BasePackage> {
        todo!("clone_box needs resolution in Phase B")
    }
}
