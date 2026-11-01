//! ref: composer/src/Composer/Package/Version/VersionBumper.php

use crate::package::PackageInterfaceHandle;
use crate::package::dumper::ArrayDumper;
use crate::package::loader::ArrayLoader;
use crate::package::version::VersionParser;
use crate::util::Platform;
use bstr::BString;
use shirabe_php_shim::{
    bconcat, explode, implode, php_regex, preg_is_match, preg_match_all, preg_replace,
    substr_count, substr_replace,
};
use shirabe_semver::Intervals;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct VersionBumper;

impl VersionBumper {
    pub fn bump_requirement(
        &self,
        constraint: &AnyConstraint,
        package: PackageInterfaceHandle,
    ) -> anyhow::Result<BString> {
        let parser = VersionParser::new();
        let pretty_constraint = constraint.get_pretty_string();
        if pretty_constraint.starts_with(b"dev-") {
            return Ok(pretty_constraint);
        }

        let mut version = package.get_version();
        if version.starts_with(b"dev-") {
            let loader = ArrayLoader::new(Some(parser.clone()), false);
            let dumper = ArrayDumper::new();
            let dumped = dumper.dump(package);
            let extra = loader.get_branch_alias(&dumped)?;

            if extra.is_none()
                || extra
                    .as_ref()
                    .is_some_and(|e| e == VersionParser::DEFAULT_BRANCH_ALIAS)
            {
                return Ok(pretty_constraint);
            }

            version = extra.unwrap();
        }

        let intervals = Intervals::get(constraint)?;

        if !intervals.branches.names.is_empty() {
            return Ok(pretty_constraint);
        }

        let major = preg_replace(php_regex!(r"{^([1-9][0-9]*|0\.\d+).*}"), "$1", &version);
        let version_without_suffix =
            preg_replace(php_regex!(r"{(?:\.(?:0|9999999))+(-dev)?$}"), "", &version);
        let new_pretty_constraint = bconcat!("^", &version_without_suffix);

        if !preg_is_match(php_regex!(r"{^\^\d+(\.\d+)*$}"), &new_pretty_constraint) {
            return Ok(pretty_constraint);
        }

        // Regex pattern compatibility:
        // PHP uses zero-width look-behind `(?<=,|\ |\||^)` and look-ahead `(?=,|$|\ |\||@)` to require
        // a separator around the constraint. The `regex` crate supports neither, so the separators are
        // matched as *consuming* groups instead. Only the `constraint` named group's own offset/length
        // is used for the replacement below (never group 0), so consuming the surrounding separators
        // does not shift the rewrite. The single-character `|`/space/comma separators are not consumed
        // by both an adjacent trailing and leading match, but Composer never packs two same-major
        // constraints around a single such separator (alternatives use `||`, ranges use spaces).
        let pattern = bconcat!(
            r#"{
            (?:^|[,\ |]) # leading separator
            (?P<constraint>
                \^v?"#,
            &major,
            r#"(?:\.\d+)* # e.g. ^2.anything
                | ~v?"#,
            &major,
            r#"(?:\.\d+){1,3} # e.g. ~2.2 or ~2.2.2 or ~2.2.2.2
                | v?"#,
            &major,
            r#"(?:\.[*x])+ # e.g. 2.* or 2.*.* or 2.x.x.x etc
                | >=v?\d(?:\.\d+)* # e.g. >=2 or >=1.2 etc
                | \* # full wildcard
            )
            (?:$|[,\ |@]) # trailing separator
        }x"#
        );

        let constraint_matches: Vec<(BString, i64)> = preg_match_all(&pattern, &pretty_constraint)
            .map(|match_| {
                let constraint = match_
                    .name("constraint")
                    .expect("the `constraint` group participates whenever the pattern matches");
                let offset = match_
                    .name_offset("constraint")
                    .expect("the `constraint` group participates whenever the pattern matches");
                (BString::from(constraint), offset as i64)
            })
            .collect();
        if !constraint_matches.is_empty() {
            let mut modified = pretty_constraint.clone();
            for (match_str, match_offset) in constraint_matches.into_iter().rev() {
                let dots_in_match = substr_count(&match_str, ".");
                let suffix =
                    if dots_in_match == 2 && substr_count(&version_without_suffix, ".") == 1 {
                        ".0"
                    } else {
                        ""
                    };
                let replacement = if match_str.starts_with(b"~") && dots_in_match != 1 {
                    let mut version_bits = explode(".", &version_without_suffix);
                    let needed_len = dots_in_match as usize + 1;
                    while version_bits.len() < needed_len {
                        version_bits.push("0".into());
                    }
                    bconcat!("~", implode(".", &version_bits[..needed_len]))
                } else if match_str == "*" || match_str.starts_with(b">=") {
                    bconcat!(">=", &version_without_suffix, suffix)
                } else {
                    bconcat!(&new_pretty_constraint, suffix)
                };
                modified = substr_replace(
                    &modified,
                    &replacement,
                    match_offset,
                    Some(Platform::strlen(&match_str)),
                );
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
