//! ref: composer/src/Composer/Package/BasePackage.php

use std::sync::LazyLock;

use indexmap::IndexMap;
use shirabe_php_shim::{LogicException, UnexpectedValueException, preg_quote};

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
    m.insert("stable", 0i64);
    m.insert("RC", 5i64);
    m.insert("beta", 10i64);
    m.insert("alpha", 15i64);
    m.insert("dev", 20i64);
    m
});

pub trait BasePackage: PackageInterface + std::fmt::Display {
    const STABILITY_STABLE: i64 = 0;
    const STABILITY_RC: i64 = 5;
    const STABILITY_BETA: i64 = 10;
    const STABILITY_ALPHA: i64 = 15;
    const STABILITY_DEV: i64 = 20;

    fn id(&self) -> i64;
    fn id_mut(&mut self) -> &mut i64;
    fn name(&self) -> &str;
    fn name_mut(&mut self) -> &mut String;
    fn pretty_name(&self) -> &str;
    fn pretty_name_mut(&mut self) -> &mut String;
    fn repository_opt(&self) -> Option<&dyn RepositoryInterface>;
    fn set_repository_box(&mut self, repository: Box<dyn RepositoryInterface>);
    fn take_repository(&mut self) -> Option<Box<dyn RepositoryInterface>>;

    fn as_any(&self) -> &dyn std::any::Any;
    fn clone_box(&self) -> Box<dyn BasePackage>;

    fn get_name(&self) -> &str {
        self.name()
    }

    fn get_pretty_name(&self) -> &str {
        self.pretty_name()
    }

    fn get_names(&self, provides: bool) -> Vec<String> {
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

    fn set_id(&mut self, id: i64) {
        *self.id_mut() = id;
    }

    fn get_id(&self) -> i64 {
        self.id()
    }

    fn set_repository(&mut self, repository: Box<dyn RepositoryInterface>) -> anyhow::Result<()> {
        if let Some(existing) = self.repository_opt() {
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
        self.set_repository_box(repository);
        Ok(())
    }

    fn get_repository(&self) -> Option<&dyn RepositoryInterface> {
        self.repository_opt()
    }

    fn is_platform(&self) -> bool {
        self.repository_opt()
            .and_then(|r| r.as_any().downcast_ref::<PlatformRepository>())
            .is_some()
    }

    fn get_unique_name(&self) -> String {
        format!("{}-{}", self.get_name(), self.get_version())
    }

    fn equals(&self, _package: &dyn PackageInterface) -> bool {
        // TODO(phase-b): implement via reference identity (requires Rc/Arc)
        // PHP uses === which is reference equality; unwraps AliasPackage on both sides
        todo!("equals requires reference identity which needs Rc/Arc")
    }

    fn get_pretty_string(&self) -> String {
        format!("{} {}", self.get_pretty_name(), self.get_pretty_version())
    }

    fn get_full_pretty_version(&self, truncate: bool, display_mode: i64) -> anyhow::Result<String> {
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

    fn get_stability_priority(&self) -> i64 {
        *STABILITIES
            .get(self.get_stability())
            .unwrap_or(&Self::STABILITY_STABLE)
    }

    fn php_clone(&mut self) {
        self.take_repository();
        *self.id_mut() = -1;
    }

    fn package_name_to_regexp(allow_pattern: &str, wrap: &str) -> String
    where
        Self: Sized,
    {
        let cleaned = preg_quote(allow_pattern, None).replace("\\*", ".*");
        wrap.replace("%s", &cleaned)
    }

    fn package_names_to_regexp(package_names: &[String], wrap: &str) -> String
    where
        Self: Sized,
    {
        let patterns: Vec<String> = package_names
            .iter()
            .map(|name| Self::package_name_to_regexp(name, "%s"))
            .collect();
        wrap.replace("%s", &patterns.join("|"))
    }
}
