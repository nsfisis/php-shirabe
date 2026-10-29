//! ref: composer/src/Composer/Package/Version/VersionParser.php

use crate::repository::PlatformRepository;
use bstr::{BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    bconcat, explode_with_limit, php_regex, preg_is_match, preg_replace, strpos, trim,
};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::{Semver, VersionParser as SemverVersionParser};
use std::sync::{LazyLock, Mutex};

static CONSTRAINTS: LazyLock<Mutex<IndexMap<BString, AnyConstraint>>> =
    LazyLock::new(|| Mutex::new(IndexMap::new()));

#[derive(Debug, Clone)]
pub struct VersionParser {
    inner: SemverVersionParser,
}

impl Default for VersionParser {
    fn default() -> Self {
        Self::new()
    }
}

impl VersionParser {
    pub const DEFAULT_BRANCH_ALIAS: &'static str = "9999999-dev";

    pub fn parse_constraints(
        &self,
        constraints: impl AsRef<[u8]>,
    ) -> anyhow::Result<AnyConstraint> {
        let constraints = constraints.as_ref().as_bstr();
        {
            let cache = CONSTRAINTS.lock().unwrap();
            if let Some(cached) = cache.get(constraints) {
                return Ok(cached.clone());
            }
        }
        let parsed = self.inner.parse_constraints(constraints)?;
        CONSTRAINTS
            .lock()
            .unwrap()
            .insert(BString::from(constraints), parsed.clone());
        Ok(parsed)
    }

    pub fn parse_name_version_pairs(
        &self,
        pairs: Vec<BString>,
    ) -> anyhow::Result<Vec<IndexMap<BString, BString>>> {
        let mut result: Vec<IndexMap<BString, BString>> = Vec::new();
        let count = pairs.len();
        let mut i = 0_usize;
        while i < count {
            let mut pair = preg_replace(
                php_regex!(r"{^([^=: ]+)[=: ](.*)$}"),
                "$1 $2",
                trim(&pairs[i]),
            );
            if !pair.contains(&b' ')
                && i + 1 < count
                && !pairs[i + 1].contains(&b'/')
                && !preg_is_match(
                    php_regex!(r"{(?<=[a-z0-9_/-])\*|\*(?=[a-z0-9_/-])}i"),
                    &pairs[i + 1],
                )
                && !PlatformRepository::is_platform_package(&pairs[i + 1])
            {
                pair = bconcat!(pair, " ", pairs[i + 1]);
                i += 1;
            }
            if strpos(&pair, " ").is_some_and(|pos| pos > 0) {
                let parts = explode_with_limit(" ", &pair, 2);
                let mut map = IndexMap::new();
                map.insert(BString::from("name"), parts[0].clone());
                map.insert(BString::from("version"), parts[1].clone());
                result.push(map);
            } else {
                let mut map = IndexMap::new();
                map.insert(BString::from("name"), pair);
                result.push(map);
            }
            i += 1;
        }
        Ok(result)
    }

    pub fn new() -> Self {
        Self {
            inner: SemverVersionParser,
        }
    }

    pub fn normalize(
        &self,
        version: impl AsRef<[u8]>,
        full_version: Option<&[u8]>,
    ) -> anyhow::Result<BString> {
        self.inner.normalize(version, full_version)
    }

    pub fn normalize_stability(stability: impl AsRef<[u8]>) -> anyhow::Result<BString> {
        SemverVersionParser::normalize_stability(stability)
    }

    pub fn normalize_branch(&self, name: impl AsRef<[u8]>) -> anyhow::Result<BString> {
        self.inner.normalize_branch(name)
    }

    pub fn parse_stability(version: impl AsRef<[u8]>) -> BString {
        SemverVersionParser::parse_stability(version)
    }

    pub fn parse_numeric_alias_prefix(&self, branch: impl AsRef<[u8]>) -> Option<BString> {
        self.inner.parse_numeric_alias_prefix(branch)
    }

    pub fn is_upgrade(
        normalized_from: impl AsRef<[u8]>,
        normalized_to: impl AsRef<[u8]>,
    ) -> anyhow::Result<bool> {
        let mut normalized_from = BString::from(normalized_from.as_ref());
        let mut normalized_to = BString::from(normalized_to.as_ref());
        if normalized_from == normalized_to {
            return Ok(true);
        }

        if [b"dev-master".as_slice(), b"dev-trunk", b"dev-default"]
            .contains(&normalized_from.as_slice())
        {
            normalized_from = VersionParser::DEFAULT_BRANCH_ALIAS.into();
        }
        if [b"dev-master".as_slice(), b"dev-trunk", b"dev-default"]
            .contains(&normalized_to.as_slice())
        {
            normalized_to = VersionParser::DEFAULT_BRANCH_ALIAS.into();
        }

        if normalized_from.starts_with(b"dev-") || normalized_to.starts_with(b"dev-") {
            return Ok(true);
        }

        let sorted = Semver::sort(vec![normalized_to, normalized_from.clone()])?;

        Ok(sorted[0] == normalized_from)
    }
}
