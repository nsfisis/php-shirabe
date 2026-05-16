//! ref: composer/vendor/composer/semver/src/Semver.php

use std::sync::OnceLock;

use crate::comparator::Comparator;
use crate::constraint::constraint::Constraint;
use crate::version_parser::VersionParser;

pub struct Semver;

impl Semver {
    pub const SORT_ASC: i64 = 1;
    pub const SORT_DESC: i64 = -1;

    fn version_parser() -> &'static VersionParser {
        static VERSION_PARSER: OnceLock<VersionParser> = OnceLock::new();
        VERSION_PARSER.get_or_init(|| VersionParser)
    }

    pub fn satisfies(version: String, constraints: String) -> anyhow::Result<bool> {
        let version_parser = Self::version_parser();
        let provider =
            Constraint::new("==".to_string(), version_parser.normalize(&version, None)?)?;
        let parsed_constraints = version_parser.parse_constraints(&constraints)?;
        Ok(parsed_constraints.matches(&provider))
    }

    pub fn satisfied_by(versions: Vec<String>, constraints: String) -> anyhow::Result<Vec<String>> {
        let mut result = Vec::new();
        for version in versions.iter() {
            if Self::satisfies(version.clone(), constraints.clone())? {
                result.push(version.clone());
            }
        }
        Ok(result)
    }

    pub fn sort(versions: Vec<String>) -> anyhow::Result<Vec<String>> {
        Self::usort(versions, Self::SORT_ASC)
    }

    pub fn rsort(versions: Vec<String>) -> anyhow::Result<Vec<String>> {
        Self::usort(versions, Self::SORT_DESC)
    }

    fn usort(versions: Vec<String>, direction: i64) -> anyhow::Result<Vec<String>> {
        let version_parser = Self::version_parser();

        let mut normalized: Vec<(String, usize)> = versions
            .iter()
            .enumerate()
            .map(|(key, version)| -> anyhow::Result<(String, usize)> {
                let normalized_version = version_parser.normalize(version, None)?;
                let normalized_version =
                    version_parser.normalize_default_branch(&normalized_version);
                Ok((normalized_version, key))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        normalized.sort_by(|left, right| {
            if left.0 == right.0 {
                return std::cmp::Ordering::Equal;
            }
            let is_less = Comparator::less_than(left.0.clone(), right.0.clone());
            let cmp_value = if is_less { -direction } else { direction };
            cmp_value.cmp(&0)
        });

        let sorted: Vec<String> = normalized
            .into_iter()
            .map(|(_, key)| versions[key].clone())
            .collect();

        Ok(sorted)
    }
}
