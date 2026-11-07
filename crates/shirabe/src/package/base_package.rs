//! ref: composer/src/Composer/Package/BasePackage.php sha1:a9d85c796bc6f6bddfda2d83848bee03e9a97f1c

use crate::package::{AliasPackage, DisplayMode, Link, PackageInterface};
use crate::repository::{PlatformRepository, RepositoryInterfaceHandle};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{bconcat, implode, preg_quote, str_replace};
use std::sync::LazyLock;

pub struct SupportedLinkType {
    pub description: &'static BStr,
    pub method: &'static BStr,
}

pub static SUPPORTED_LINK_TYPES: LazyLock<IndexMap<&'static BStr, SupportedLinkType>> =
    LazyLock::new(|| {
        let mut m = IndexMap::new();
        m.insert(
            BStr::new("require"),
            SupportedLinkType {
                description: BStr::new("requires"),
                method: Link::TYPE_REQUIRE,
            },
        );
        m.insert(
            BStr::new("conflict"),
            SupportedLinkType {
                description: BStr::new("conflicts"),
                method: Link::TYPE_CONFLICT,
            },
        );
        m.insert(
            BStr::new("provide"),
            SupportedLinkType {
                description: BStr::new("provides"),
                method: Link::TYPE_PROVIDE,
            },
        );
        m.insert(
            BStr::new("replace"),
            SupportedLinkType {
                description: BStr::new("replaces"),
                method: Link::TYPE_REPLACE,
            },
        );
        m.insert(
            BStr::new("require-dev"),
            SupportedLinkType {
                description: BStr::new("requires (for development)"),
                method: Link::TYPE_DEV_REQUIRE,
            },
        );
        m
    });

pub static STABILITIES: LazyLock<IndexMap<&'static BStr, i64>> = LazyLock::new(|| {
    let mut m = IndexMap::new();
    m.insert(BStr::new("stable"), 0i64);
    m.insert(BStr::new("RC"), 5i64);
    m.insert(BStr::new("beta"), 10i64);
    m.insert(BStr::new("alpha"), 15i64);
    m.insert(BStr::new("dev"), 20i64);
    m
});

/// `self::STABILITIES[$stability]` for a stability name held as bytes.
pub fn stability_priority(stability: impl AsRef<[u8]>) -> Option<i64> {
    let stability = stability.as_ref();
    STABILITIES.get(BStr::new(stability)).copied()
}

pub const STABILITY_STABLE: i64 = 0;
pub const STABILITY_RC: i64 = 5;
pub const STABILITY_BETA: i64 = 10;
pub const STABILITY_ALPHA: i64 = 15;
pub const STABILITY_DEV: i64 = 20;

pub trait BasePackage: PackageInterface {
    fn id(&self) -> i64;
    fn id_mut(&mut self) -> &mut i64;
    fn name(&self) -> &BStr;
    fn name_mut(&mut self) -> &mut BString;
    fn pretty_name(&self) -> &BStr;
    fn pretty_name_mut(&mut self) -> &mut BString;
    fn repository_opt(&self) -> Option<RepositoryInterfaceHandle>;
    fn set_repository_box(&mut self, repository: RepositoryInterfaceHandle);
    fn take_repository(&mut self) -> Option<RepositoryInterfaceHandle>;

    fn as_alias_package_mut(&mut self) -> Option<&mut AliasPackage> {
        None
    }

    fn is_platform(&self) -> bool {
        self.repository_opt()
            .is_some_and(|r| r.is::<PlatformRepository>())
    }

    fn get_full_pretty_version(&self, truncate: bool, display_mode: DisplayMode) -> BString {
        if display_mode == DisplayMode::SourceRefIfDev
            && (!self.is_dev()
                || (!["hg", "git"]
                    .iter()
                    .any(|t| self.get_source_type().unwrap_or_default() == *t)
                    && (self.get_source_type().unwrap_or_default() != ""
                        || self.get_dist_reference().unwrap_or_default() == "")))
        {
            return BString::from(self.get_pretty_version());
        }

        let reference: Option<BString> = match display_mode {
            DisplayMode::SourceRefIfDev => {
                if self.get_source_reference().unwrap_or_default() != "" {
                    self.get_source_reference()
                } else {
                    self.get_dist_reference()
                }
            }
            DisplayMode::SourceRef => self.get_source_reference(),
            DisplayMode::DistRef => self.get_dist_reference(),
        };

        let reference = match reference {
            None => return BString::from(self.get_pretty_version()),
            Some(r) => r,
        };

        if truncate && reference.len() == 40 && self.get_source_type().is_none_or(|t| t != "svn") {
            return bconcat!(self.get_pretty_version(), " ", &reference[..7]);
        }

        bconcat!(self.get_pretty_version(), " ", reference)
    }

    fn get_stability_priority(&self) -> i64 {
        stability_priority(self.get_stability()).unwrap_or(STABILITY_STABLE)
    }
}

pub fn package_name_to_regexp(allow_pattern: impl AsRef<[u8]>) -> BString {
    package_name_to_regexp2(allow_pattern, "{^%s$}i")
}

pub fn package_name_to_regexp2(allow_pattern: impl AsRef<[u8]>, wrap: impl AsRef<[u8]>) -> BString {
    let cleaned = str_replace("\\*", ".*", preg_quote(allow_pattern, None));
    str_replace("%s", cleaned, wrap)
}

pub fn package_names_to_regexp(
    package_names: &[impl AsRef<[u8]>],
    wrap: impl AsRef<[u8]>,
) -> BString {
    let patterns: Vec<BString> = package_names
        .iter()
        .map(|name| package_name_to_regexp2(name, "%s"))
        .collect();
    str_replace("%s", implode("|", &patterns), wrap)
}
