//! ref: composer/src/Composer/Package/Version/VersionParser.php

use crate::repository::PlatformRepository;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::php_regex;
use shirabe_semver::Semver;
use shirabe_semver::VersionParser as SemverVersionParser;
use shirabe_semver::constraint::AnyConstraint;
use std::sync::{LazyLock, Mutex};

static CONSTRAINTS: LazyLock<Mutex<IndexMap<String, AnyConstraint>>> =
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

    pub fn parse_constraints(&self, constraints: &str) -> anyhow::Result<AnyConstraint> {
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
            .insert(constraints.to_string(), parsed.clone());
        Ok(parsed)
    }

    pub fn parse_name_version_pairs(
        &self,
        pairs: Vec<String>,
    ) -> anyhow::Result<Vec<IndexMap<String, String>>> {
        let pairs: Vec<String> = pairs;
        let mut result: Vec<IndexMap<String, String>> = Vec::new();
        let count = pairs.len();
        let mut i = 0_usize;
        while i < count {
            let mut pair = Preg::replace(
                php_regex!(r"{^([^=: ]+)[=: ](.*)$}"),
                "$1 $2",
                pairs[i].trim(),
            );
            if !pair.contains(' ')
                && i + 1 < count
                && !pairs[i + 1].contains('/')
                && !Preg::is_match(
                    php_regex!(r"{(?<=[a-z0-9_/-])\*|\*(?=[a-z0-9_/-])}i"),
                    &pairs[i + 1],
                )
                && !PlatformRepository::is_platform_package(&pairs[i + 1])
            {
                pair += &format!(" {}", pairs[i + 1]);
                i += 1;
            }
            if pair.contains(' ') {
                let parts: Vec<&str> = pair.splitn(2, ' ').collect();
                let name = parts[0].to_string();
                let version = parts[1].to_string();
                let mut map = IndexMap::new();
                map.insert("name".to_string(), name);
                map.insert("version".to_string(), version);
                result.push(map);
            } else {
                let mut map = IndexMap::new();
                map.insert("name".to_string(), pair);
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

    pub fn normalize(&self, version: &str, full_version: Option<&str>) -> anyhow::Result<String> {
        self.inner.normalize(version, full_version)
    }

    pub fn normalize_stability(stability: &str) -> anyhow::Result<String> {
        SemverVersionParser::normalize_stability(stability)
    }

    pub fn normalize_branch(&self, name: &str) -> anyhow::Result<String> {
        self.inner.normalize_branch(name)
    }

    pub fn parse_stability(version: &str) -> String {
        SemverVersionParser::parse_stability(version)
    }

    pub fn parse_numeric_alias_prefix(&self, branch: &str) -> Option<String> {
        self.inner.parse_numeric_alias_prefix(branch)
    }

    pub fn is_upgrade(normalized_from: &str, normalized_to: &str) -> anyhow::Result<bool> {
        if normalized_from == normalized_to {
            return Ok(true);
        }

        let mut normalized_from = normalized_from.to_string();
        let mut normalized_to = normalized_to.to_string();

        if ["dev-master", "dev-trunk", "dev-default"].contains(&normalized_from.as_str()) {
            normalized_from = VersionParser::DEFAULT_BRANCH_ALIAS.to_string();
        }
        if ["dev-master", "dev-trunk", "dev-default"].contains(&normalized_to.as_str()) {
            normalized_to = VersionParser::DEFAULT_BRANCH_ALIAS.to_string();
        }

        if normalized_from.starts_with("dev-") || normalized_to.starts_with("dev-") {
            return Ok(true);
        }

        let sorted = Semver::sort(vec![normalized_to.clone(), normalized_from.clone()])?;

        Ok(sorted[0] == normalized_from)
    }
}
