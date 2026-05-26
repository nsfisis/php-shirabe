//! ref: composer/src/Composer/Package/Version/VersionBumper.php

use crate::package::PackageInterfaceHandle;
use crate::package::dumper::ArrayDumper;
use crate::package::loader::ArrayLoader;
use crate::package::version::VersionParser;
use crate::util::Platform;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::intervals::Intervals;

#[derive(Debug)]
pub struct VersionBumper;

impl VersionBumper {
    pub fn bump_requirement(
        &self,
        constraint: &AnyConstraint,
        package: PackageInterfaceHandle,
    ) -> Result<String> {
        let parser = VersionParser::new();
        let pretty_constraint = constraint.get_pretty_string();
        if pretty_constraint.starts_with("dev-") {
            return Ok(pretty_constraint);
        }

        let mut version = package.get_version().to_string();
        if version.starts_with("dev-") {
            // TODO(phase-b): ArrayLoader::new takes Option<VersionParser> by value; pass None until
            // VersionParser sharing is reconciled.
            let _ = &parser;
            let loader = ArrayLoader::new(None, false);
            let dumper = ArrayDumper::new();
            let dumped = dumper.dump(package.clone());
            let extra = loader.get_branch_alias(&dumped)?;

            if extra.is_none() || extra.as_deref() == Some(VersionParser::DEFAULT_BRANCH_ALIAS) {
                return Ok(pretty_constraint);
            }

            version = extra.unwrap();
        }

        let intervals = Intervals::get(constraint)?;

        if !intervals.branches.names.is_empty() {
            return Ok(pretty_constraint);
        }

        let major = Preg::replace(r"{^([1-9][0-9]*|0\.\d+).*}", "$1", &version)?;
        let version_without_suffix =
            Preg::replace(r"{(?:\.(?:0|9999999))+(-dev)?$}", "", &version)?;
        let new_pretty_constraint = format!("^{}", version_without_suffix);

        if !Preg::is_match(r"{^\^\d+(\.\d+)*$}", &new_pretty_constraint)? {
            return Ok(pretty_constraint);
        }

        let pattern = format!(
            r#"{{
            (?<=,|\ |\||^) # leading separator
            (?P<constraint>
                \^v?{major}(?:\.\d+)* # e.g. ^2.anything
                | ~v?{major}(?:\.\d+){{1,3}} # e.g. ~2.2 or ~2.2.2 or ~2.2.2.2
                | v?{major}(?:\.[*x])+ # e.g. 2.* or 2.*.* or 2.x.x.x etc
                | >=v?\d(?:\.\d+)* # e.g. >=2 or >=1.2 etc
                | \* # full wildcard
            )
            (?=,|$|\ |\||@) # trailing separator
        }}x"#,
            major = major
        );

        let mut matches: IndexMap<CaptureKey, Vec<(String, usize)>> = IndexMap::new();
        if Preg::is_match_all_with_offsets3(&pattern, &pretty_constraint, Some(&mut matches))? {
            let mut modified = pretty_constraint.clone();
            let constraint_matches = matches
                .get(&CaptureKey::ByName("constraint".to_string()))
                .cloned()
                .unwrap_or_default();
            for match_ in constraint_matches.iter().rev() {
                let match_str = &match_.0;
                let match_offset = match_.1 as i64;
                let suffix = if match_str.matches('.').count() == 2
                    && version_without_suffix.matches('.').count() == 1
                {
                    ".0"
                } else {
                    ""
                };
                let replacement =
                    if match_str.starts_with('~') && match_str.matches('.').count() != 1 {
                        let mut version_bits: Vec<String> = version_without_suffix
                            .split('.')
                            .map(String::from)
                            .collect();
                        let needed_len = match_str.matches('.').count() + 1;
                        while version_bits.len() < needed_len {
                            version_bits.push("0".to_string());
                        }
                        let dots_in_match = match_str.matches('.').count();
                        format!("~{}", version_bits[..dots_in_match + 1].join("."))
                    } else if match_str == "*" || match_str.starts_with(">=") {
                        format!(">={}{}", version_without_suffix, suffix)
                    } else {
                        format!("{}{}", new_pretty_constraint, suffix)
                    };
                let offset = match_offset as usize;
                let length = Platform::strlen(match_str) as usize;
                modified =
                    shirabe_php_shim::substr_replace(&modified, &replacement, offset, length);
            }

            let new_constraint = parser.parse_constraints(&modified)?;
            if Intervals::is_subset_of(&new_constraint, constraint)?
                && Intervals::is_subset_of(constraint, &new_constraint)?
            {
                return Ok(pretty_constraint);
            }

            return Ok(modified);
        }

        Ok(pretty_constraint)
    }
}
