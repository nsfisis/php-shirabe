//! ref: composer/vendor/composer/semver/src/VersionParser.php

use crate::constraint::constraint::Constraint;
use crate::constraint::constraint_interface::ConstraintInterface;
use crate::constraint::match_all_constraint::MatchAllConstraint;
use crate::constraint::multi_constraint::MultiConstraint;
use shirabe_php_shim as php;

// Regex to match pre-release data (sort of).
//
// Due to backwards compatibility:
//   - Instead of enforcing hyphen, an underscore, dot or nothing at all are also accepted.
//   - Only stabilities as recognized by Composer are allowed to precede a numerical identifier.
//   - Numerical-only pre-release identifiers are not supported, see tests.
//
//                        |--------------|
// [major].[minor].[patch] -[pre-release] +[build-metadata]
const MODIFIER_REGEX: &str =
    "[._-]?(?:(stable|beta|b|RC|alpha|a|patch|pl|p)((?:[.-]?\\d+)*+)?)?([.-]?dev)?";

const STABILITIES_REGEX: &str = "stable|RC|beta|alpha|dev";

#[derive(Debug)]
pub struct VersionParser;

impl VersionParser {
    pub fn parse_stability(version: &str) -> String {
        let version = php::preg_replace("{#.+$}", "", version).unwrap_or_default();

        if version.starts_with("dev-") || version.ends_with("-dev") {
            return "dev".to_string();
        }

        let pattern = format!("{{{}(?:\\+.*)?$}}i", MODIFIER_REGEX);
        let lower = php::strtolower(&version);
        let mut match_: Vec<Option<String>> = Vec::new();
        php::preg_match(&pattern, &lower, &mut match_);

        // match_[3] = the ([.-]?dev)? capture
        if match_
            .get(3)
            .and_then(|o| o.as_deref())
            .is_some_and(|s| !s.is_empty())
        {
            return "dev".to_string();
        }

        // match_[1] = the (stable|beta|b|RC|alpha|a|patch|pl|p) capture
        let m1 = match_.get(1).and_then(|o| o.as_deref()).unwrap_or("");
        if !m1.is_empty() {
            if m1 == "beta" || m1 == "b" {
                return "beta".to_string();
            }
            if m1 == "alpha" || m1 == "a" {
                return "alpha".to_string();
            }
            if m1 == "rc" {
                return "RC".to_string();
            }
        }

        "stable".to_string()
    }

    pub fn normalize_stability(stability: &str) -> anyhow::Result<String> {
        let stability = php::strtolower(stability);

        if !["stable", "rc", "beta", "alpha", "dev"].contains(&stability.as_str()) {
            anyhow::bail!(
                "Invalid stability string \"{}\", expected one of stable, RC, beta, alpha or dev",
                stability
            );
        }

        Ok(if stability == "rc" {
            "RC".to_string()
        } else {
            stability
        })
    }

    pub fn normalize(&self, version: &str, full_version: Option<&str>) -> anyhow::Result<String> {
        let version = php::trim(version, None);
        let orig_version = version.clone();
        let full_version = full_version.unwrap_or(&version).to_string();
        let mut version = version;

        // strip off aliasing
        let mut match_: Vec<Option<String>> = Vec::new();
        if php::preg_match("{^([^,\\s]++) ++as ++([^,\\s]++)$}", &version, &mut match_) > 0 {
            version = match_[1].clone().unwrap_or_default();
        }

        // strip off stability flag
        let stab_pattern = format!("{{@(?:{})$}}i", STABILITIES_REGEX);
        let mut match_: Vec<Option<String>> = Vec::new();
        if php::preg_match(&stab_pattern, &version, &mut match_) > 0 {
            let match0_len = match_[0].as_deref().unwrap_or("").len();
            version = version[..version.len() - match0_len].to_string();
        }

        // normalize master/trunk/default branches to dev-name for BC with 1.x as these used to
        // be valid constraints
        if ["master", "trunk", "default"].contains(&version.as_str()) {
            version = format!("dev-{}", version);
        }

        // if requirement is branch-like, use full name
        if version.to_ascii_lowercase().starts_with("dev-") {
            return Ok(format!("dev-{}", &version[4..]));
        }

        // strip off build metadata
        let mut match_: Vec<Option<String>> = Vec::new();
        if php::preg_match("{^([^,\\s+]++)\\+[^\\s]++$}", &version, &mut match_) > 0 {
            version = match_[1].clone().unwrap_or_default();
        }

        let mut index: Option<usize> = None;
        let mut matches: Vec<Option<String>> = Vec::new();

        // match classical versioning
        let classical_pattern = format!(
            "{{^v?(\\d{{1,5}}+)(\\.\\d++)?(\\.\\d++)?(\\.\\d++)?{}$}}i",
            MODIFIER_REGEX
        );
        if php::preg_match(&classical_pattern, &version, &mut matches) > 0 {
            let m2 = matches[2].as_deref().unwrap_or("");
            let m3 = matches[3].as_deref().unwrap_or("");
            let m4 = matches[4].as_deref().unwrap_or("");
            version = format!(
                "{}{}{}{}",
                matches[1].as_deref().unwrap_or(""),
                if m2.is_empty() { ".0" } else { m2 },
                if m3.is_empty() { ".0" } else { m3 },
                if m4.is_empty() { ".0" } else { m4 },
            );
            index = Some(5);
        } else {
            // match date(time) based versioning
            let datetime_pattern = format!(
                "{{^v?(\\d{{4}}(?:[.:-]?\\d{{2}}){{1,6}}(?:[.:-]?\\d{{1,3}}){{0,2}}){}$}}i",
                MODIFIER_REGEX
            );
            if php::preg_match(&datetime_pattern, &version, &mut matches) > 0 {
                version = php::preg_replace("{\\D}", ".", matches[1].as_deref().unwrap_or(""))
                    .unwrap_or_default();
                index = Some(2);
            }
        }

        // add version modifiers if a version was matched
        if let Some(idx) = index {
            let mi = matches.get(idx).and_then(|o| o.as_deref()).unwrap_or("");
            if !mi.is_empty() {
                if mi == "stable" {
                    return Ok(version);
                }
                let mi1 = matches
                    .get(idx + 1)
                    .and_then(|o| o.as_deref())
                    .unwrap_or("");
                version = format!(
                    "{}-{}{}",
                    version,
                    self.expand_stability(mi),
                    if !mi1.is_empty() {
                        mi1.trim_start_matches(['.', '-'])
                    } else {
                        ""
                    }
                );
            }

            if !matches
                .get(idx + 2)
                .and_then(|o| o.as_deref())
                .unwrap_or("")
                .is_empty()
            {
                version = format!("{}-dev", version);
            }

            return Ok(version);
        }

        // match dev branches
        let mut match_: Vec<Option<String>> = Vec::new();
        if php::preg_match("{(.*?)[.-]?dev$}i", &version, &mut match_) > 0 {
            let branch_name = match_[1].clone().unwrap_or_default();
            // a branch ending with -dev is only valid if it is numeric
            // if it gets prefixed with dev- it means the branch name should
            // have had a dev- prefix already when passed to normalize
            if let Ok(normalized) = self.normalize_branch(&branch_name)
                && !normalized.starts_with("dev-")
            {
                return Ok(normalized);
            }
        }

        let extra_message = if php::preg_match(
            &format!(
                "{{ +as +{}(?:@(?:{}))?$}}",
                php::preg_quote(&version, None),
                STABILITIES_REGEX
            ),
            &full_version,
            &mut Vec::new(),
        ) > 0
        {
            format!(
                " in \"{}\", the alias must be an exact version",
                full_version
            )
        } else if php::preg_match(
            &format!(
                "{{^{}(?:@(?:{}))?  +as +}}",
                php::preg_quote(&version, None),
                STABILITIES_REGEX
            ),
            &full_version,
            &mut Vec::new(),
        ) > 0
        {
            format!(
                " in \"{}\", the alias source must be an exact version, if it is a branch name \
                you should prefix it with dev-",
                full_version
            )
        } else {
            String::new()
        };

        anyhow::bail!(
            "Invalid version string \"{}\"{}",
            orig_version,
            extra_message
        )
    }

    pub fn parse_numeric_alias_prefix(&self, branch: &str) -> Option<String> {
        let mut matches: Vec<Option<String>> = Vec::new();
        // matches['version'] == matches[1] ((?P<version>...) is group 1)
        if php::preg_match(
            "{^(?P<version>(\\d++\\.)*\\d++)(?:\\.x)?-dev$}i",
            branch,
            &mut matches,
        ) > 0
        {
            let version = matches[1].clone().unwrap_or_default();
            return Some(format!("{}.", version));
        }

        None
    }

    pub fn normalize_branch(&self, name: &str) -> anyhow::Result<String> {
        let name = php::trim(name, None);

        let mut matches: Vec<Option<String>> = Vec::new();
        // Groups: 1=major, 2=".minor"(outer), 3=minor(inner), 4=".patch"(outer),
        // 5=patch(inner), 6=".fourth"(outer), 7=fourth(inner).
        // We use the outer groups [1,2,4,6] to replicate PHP's groups [1,2,3,4].
        if php::preg_match(
            "{^v?(\\d++)(\\.(\\d++|[xX*]))?(\\.(\\d++|[xX*]))?(\\.(\\d++|[xX*]))?$}i",
            &name,
            &mut matches,
        ) > 0
        {
            let mut version = String::new();
            for i in [1usize, 2, 4, 6] {
                if let Some(Some(m)) = matches.get(i) {
                    version.push_str(&m.replace(['*', 'X'], "x"));
                } else {
                    version.push_str(".x");
                }
            }

            return Ok(format!("{}-dev", version.replace('x', "9999999")));
        }

        Ok(format!("dev-{}", name))
    }

    #[deprecated(
        note = "No need to use this anymore in theory, Composer 2 does not normalize any \
            branch names to 9999999-dev anymore"
    )]
    pub fn normalize_default_branch(&self, name: &str) -> String {
        if name == "dev-master" || name == "dev-default" || name == "dev-trunk" {
            return "9999999-dev".to_string();
        }

        name.to_string()
    }

    pub fn parse_constraints(
        &self,
        constraints: &str,
    ) -> anyhow::Result<Box<dyn ConstraintInterface>> {
        let pretty_constraint = constraints.to_string();

        let or_constraints = php::preg_split("{\\s*\\|\\|?\\s*}", &php::trim(constraints, None))
            .ok_or_else(|| anyhow::anyhow!("Failed to preg_split string: {}", constraints))?;

        let mut or_groups: Vec<Box<dyn ConstraintInterface>> = Vec::new();

        for or_constraint in &or_constraints {
            let and_constraints = php::preg_split(
                "{(?<!^|as|[=>< ,]) *(?<!-)[, ](?!-) *(?!,|as|$)}",
                or_constraint,
            )
            .ok_or_else(|| anyhow::anyhow!("Failed to preg_split string: {}", or_constraint))?;

            let constraint_objects: Vec<Box<dyn ConstraintInterface>> = if and_constraints.len() > 1
            {
                let mut objs: Vec<Box<dyn ConstraintInterface>> = Vec::new();
                for and_constraint in &and_constraints {
                    for parsed in self.parse_constraint(and_constraint)? {
                        objs.push(parsed);
                    }
                }
                objs
            } else {
                self.parse_constraint(&and_constraints[0])?
            };

            let constraint: Box<dyn ConstraintInterface> = if constraint_objects.len() == 1 {
                constraint_objects.into_iter().next().unwrap()
            } else {
                Box::new(MultiConstraint::new(constraint_objects, true)?)
            };

            or_groups.push(constraint);
        }

        let mut parsed_constraint = MultiConstraint::create(or_groups, false)?;

        parsed_constraint.set_pretty_string(Some(pretty_constraint));

        Ok(parsed_constraint)
    }

    fn parse_constraint(
        &self,
        constraint: &str,
    ) -> anyhow::Result<Vec<Box<dyn ConstraintInterface>>> {
        let mut constraint = constraint.to_string();

        // strip off aliasing
        let mut match_: Vec<Option<String>> = Vec::new();
        if php::preg_match(
            "{^([^,\\s]++) ++as ++([^,\\s]++)$}",
            &constraint,
            &mut match_,
        ) > 0
        {
            constraint = match_[1].clone().unwrap_or_default();
        }

        // strip @stability flags, and keep it for later use
        let mut stability_modifier: Option<String> = None;
        let mut match_: Vec<Option<String>> = Vec::new();
        let stab_pattern = format!("{{^([^,\\s]*?)@({})$}}i", STABILITIES_REGEX);
        if php::preg_match(&stab_pattern, &constraint, &mut match_) > 0 {
            let m1 = match_[1].as_deref().unwrap_or("");
            constraint = if !m1.is_empty() {
                m1.to_string()
            } else {
                "*".to_string()
            };
            let m2 = match_[2].as_deref().unwrap_or("");
            if m2 != "stable" {
                stability_modifier = Some(m2.to_string());
            }
        }

        // get rid of #refs as those are used by composer only
        let mut match_: Vec<Option<String>> = Vec::new();
        if php::preg_match(
            "{^(dev-[^,\\s@]+?|[^,\\s@]+?\\.x-dev)#.+$}i",
            &constraint,
            &mut match_,
        ) > 0
        {
            constraint = match_[1].clone().unwrap_or_default();
        }

        let mut match_: Vec<Option<String>> = Vec::new();
        if php::preg_match("{^(v)?[xX*](\\.[xX*])*$}i", &constraint, &mut match_) > 0 {
            let m1_nonempty = !match_
                .get(1)
                .and_then(|o| o.as_deref())
                .unwrap_or("")
                .is_empty();
            let m2_nonempty = !match_
                .get(2)
                .and_then(|o| o.as_deref())
                .unwrap_or("")
                .is_empty();
            if m1_nonempty || m2_nonempty {
                return Ok(vec![Box::new(Constraint::new(
                    ">=".to_string(),
                    "0.0.0.0-dev".to_string(),
                )?)]);
            }

            return Ok(vec![Box::new(MatchAllConstraint {
                pretty_string: None,
            })]);
        }

        let version_regex = format!(
            "v?(\\d++)(?:\\.(\\d++))?(?:\\.(\\d++))?(?:\\.(\\d++))?(?:{}|\\.([xX*][.-]?dev))(?:\\+[^\\s]+)?",
            MODIFIER_REGEX
        );

        // Tilde Range
        //
        // Like wildcard constraints, unsuffixed tilde constraints say that they must be greater
        // than the previous version, to ensure that unstable instances of the current version are
        // allowed. However, if a stability suffix is added to the constraint, then a >= match on
        // the current version is used instead.
        let mut matches: Vec<Option<String>> = Vec::new();
        let tilde_pattern = format!("{{^~>?{}$}}i", version_regex);
        if php::preg_match(&tilde_pattern, &constraint, &mut matches) > 0 {
            if constraint.starts_with("~>") {
                anyhow::bail!(
                    "Could not parse version constraint {}: Invalid operator \"~>\", you probably \
                    meant to use the \"~\" operator",
                    constraint
                );
            }

            // Work out which position in the version we are operating at
            let mut position = if !matches[4].as_deref().unwrap_or("").is_empty() {
                4
            } else if !matches[3].as_deref().unwrap_or("").is_empty() {
                3
            } else if !matches[2].as_deref().unwrap_or("").is_empty() {
                2
            } else {
                1
            };

            // when matching 2.x-dev or 3.0.x-dev we have to shift the second or third number,
            // despite no second/third number matching above
            if !matches[8].as_deref().unwrap_or("").is_empty() {
                position += 1;
            }

            // Calculate the stability suffix
            let stability_suffix = if matches[5].as_deref().unwrap_or("").is_empty()
                && matches[7].as_deref().unwrap_or("").is_empty()
                && matches[8].as_deref().unwrap_or("").is_empty()
            {
                "-dev"
            } else {
                ""
            };

            let low_version =
                self.normalize(&format!("{}{}", &constraint[1..], stability_suffix), None)?;
            let lower_bound = Constraint::new(">=".to_string(), low_version)?;

            // For upper bound, we increment the position of one more significance,
            // but highPosition = 0 would be illegal
            let high_position = std::cmp::max(1, position - 1);
            let high_version = format!(
                "{}-dev",
                self.manipulate_version_string(&matches, high_position, 1, "0")
                    .unwrap_or_default()
            );
            let upper_bound = Constraint::new("<".to_string(), high_version)?;

            return Ok(vec![Box::new(lower_bound), Box::new(upper_bound)]);
        }

        // Caret Range
        //
        // Allows changes that do not modify the left-most non-zero digit in the [major, minor,
        // patch] tuple. In other words, this allows patch and minor updates for versions 1.0.0
        // and above, patch updates for versions 0.X >=0.1.0, and no updates for versions 0.0.X
        let mut matches: Vec<Option<String>> = Vec::new();
        let caret_pattern = format!("{{^\\^{}($)}}i", version_regex);
        if php::preg_match(&caret_pattern, &constraint, &mut matches) > 0 {
            // Work out which position in the version we are operating at
            let m1 = matches[1].as_deref().unwrap_or("");
            let m2 = matches[2].as_deref().unwrap_or("");
            let m3 = matches[3].as_deref().unwrap_or("");
            let position = if m1 != "0" || m2.is_empty() {
                1
            } else if m2 != "0" || m3.is_empty() {
                2
            } else {
                3
            };

            // Calculate the stability suffix
            let stability_suffix = if matches[5].as_deref().unwrap_or("").is_empty()
                && matches[7].as_deref().unwrap_or("").is_empty()
                && matches[8].as_deref().unwrap_or("").is_empty()
            {
                "-dev"
            } else {
                ""
            };

            let low_version =
                self.normalize(&format!("{}{}", &constraint[1..], stability_suffix), None)?;
            let lower_bound = Constraint::new(">=".to_string(), low_version)?;

            // For upper bound, we increment the position of one more significance,
            // but highPosition = 0 would be illegal
            let high_version = format!(
                "{}-dev",
                self.manipulate_version_string(&matches, position, 1, "0")
                    .unwrap_or_default()
            );
            let upper_bound = Constraint::new("<".to_string(), high_version)?;

            return Ok(vec![Box::new(lower_bound), Box::new(upper_bound)]);
        }

        // X Range
        //
        // Any of X, x, or * may be used to "stand in" for one of the numeric values in the
        // [major, minor, patch] tuple. A partial version range is treated as an X-Range, so the
        // special character is in fact optional.
        let mut matches: Vec<Option<String>> = Vec::new();
        if php::preg_match(
            "{^v?(\\d++)(?:\\.(\\d++))?(?:\\.(\\d++))?(?:\\.[xX*])++$}",
            &constraint,
            &mut matches,
        ) > 0
        {
            let position = if !matches[3].as_deref().unwrap_or("").is_empty() {
                3
            } else if !matches[2].as_deref().unwrap_or("").is_empty() {
                2
            } else {
                1
            };

            let low_version = format!(
                "{}-dev",
                self.manipulate_version_string(&matches, position, 0, "0")
                    .unwrap_or_default()
            );
            let high_version = format!(
                "{}-dev",
                self.manipulate_version_string(&matches, position, 1, "0")
                    .unwrap_or_default()
            );

            if low_version == "0.0.0.0-dev" {
                return Ok(vec![Box::new(Constraint::new(
                    "<".to_string(),
                    high_version,
                )?)]);
            }

            return Ok(vec![
                Box::new(Constraint::new(">=".to_string(), low_version)?),
                Box::new(Constraint::new("<".to_string(), high_version)?),
            ]);
        }

        // Hyphen Range
        //
        // Specifies an inclusive set. If a partial version is provided as the first version in
        // the inclusive range, then the missing pieces are replaced with zeroes. If a partial
        // version is provided as the second version in the inclusive range, then all versions
        // that start with the supplied parts of the tuple are accepted, but nothing that would
        // be greater than the provided tuple parts.
        let mut matches: Vec<Option<String>> = Vec::new();
        let hyphen_pattern = format!(
            "{{^(?P<from>{}) +- +(?P<to>{})($)}}i",
            version_regex, version_regex
        );
        if php::preg_match(&hyphen_pattern, &constraint, &mut matches) > 0 {
            // matches[1]='from' string, matches[2..9]=from captures, matches[10]='to' string,
            // matches[11..18]=to captures, matches[19]='($)'
            // matches[6]=from stability, matches[8]=from dev, matches[9]=from wildcard-dev
            let low_stability_suffix = if matches[6].as_deref().unwrap_or("").is_empty()
                && matches[8].as_deref().unwrap_or("").is_empty()
                && matches[9].as_deref().unwrap_or("").is_empty()
            {
                "-dev"
            } else {
                ""
            };

            let from_str = matches[1].clone().unwrap_or_default(); // matches['from']
            let low_version = self.normalize(&from_str, None)?;
            let lower_bound = Constraint::new(
                ">=".to_string(),
                format!("{}{}", low_version, low_stability_suffix),
            )?;

            // PHP's empty() on "0" returns true, but here we only check for truly empty/missing
            let empty = |x: &Option<String>| -> bool { x.as_deref().is_none_or(|s| s.is_empty()) };

            // matches[12]=to minor, matches[13]=to patch, matches[15]=to stability,
            // matches[17]=to dev, matches[18]=to wildcard-dev
            let upper_bound: Constraint = if (!empty(&matches[12]) && !empty(&matches[13]))
                || !matches[15].as_deref().unwrap_or("").is_empty()
                || !matches[17].as_deref().unwrap_or("").is_empty()
                || !matches[18].as_deref().unwrap_or("").is_empty()
            {
                let to_str = matches[10].clone().unwrap_or_default(); // matches['to']
                let hv = self.normalize(&to_str, None)?;
                Constraint::new("<=".to_string(), hv)?
            } else {
                // matches[11]=to major, matches[12]=to minor, matches[13]=to patch,
                // matches[14]=to fourth
                let high_match = vec![
                    Some(String::new()),
                    matches[11].clone(),
                    matches[12].clone(),
                    matches[13].clone(),
                    matches[14].clone(),
                ];

                // validate to version
                let to_str = matches[10].clone().unwrap_or_default(); // matches['to']
                self.normalize(&to_str, None)?;

                let position = if empty(&matches[12]) { 1 } else { 2 };
                let hv = format!(
                    "{}-dev",
                    self.manipulate_version_string(&high_match, position, 1, "0")
                        .unwrap_or_default()
                );
                Constraint::new("<".to_string(), hv)?
            };

            return Ok(vec![Box::new(lower_bound), Box::new(upper_bound)]);
        }

        // Basic Comparators
        let mut match_: Vec<Option<String>> = Vec::new();
        if php::preg_match("{^(<>|!=|>=?|<=?|==?)?\\s*(.*)}", &constraint, &mut match_) > 0 {
            let version_str = match_[2].clone().unwrap_or_default();
            let op_str = match_[1].clone().unwrap_or_default();

            let version_result: anyhow::Result<String> = (match self.normalize(&version_str, None) {
                Ok(v) => Ok(v),
                Err(e) => {
                    // recover from an invalid constraint like foobar-dev which should be
                    // dev-foobar except if the constraint uses a known operator, in which
                    // case it must be a parse error
                    if version_str.ends_with("-dev")
                        && php::preg_match("{^[0-9a-zA-Z-./]+$}", &version_str, &mut Vec::new()) > 0
                    {
                        self.normalize(
                            &format!("dev-{}", &version_str[..version_str.len() - 4]),
                            None,
                        )
                    } else {
                        Err(e)
                    }
                }
            });

            if let Ok(mut version) = version_result {
                let op = if op_str.is_empty() { "=" } else { &op_str };

                if op != "=="
                    && op != "="
                    && let Some(ref stab_mod) = stability_modifier
                    && Self::parse_stability(&version) == "stable"
                {
                    version = format!("{}-{}", version, stab_mod);
                }
                if op == "<" || op == ">=" {
                    let modifier_pattern = format!("{{-{}$}}", MODIFIER_REGEX);
                    if php::preg_match(
                        &modifier_pattern,
                        &php::strtolower(&version_str),
                        &mut Vec::new(),
                    ) == 0
                        && !version_str.starts_with("dev-")
                    {
                        version = format!("{}-dev", version);
                    }
                }

                let final_op = if op_str.is_empty() {
                    "=".to_string()
                } else {
                    op_str
                };
                return Ok(vec![Box::new(Constraint::new(final_op, version)?)]);
            }
        }

        anyhow::bail!("Could not parse version constraint {}", constraint)
    }

    fn manipulate_version_string(
        &self,
        matches: &[Option<String>],
        position: usize,
        increment: i64,
        pad: &str,
    ) -> Option<String> {
        let mut parts: [i64; 5] = [
            0,
            matches
                .get(1)
                .and_then(|o| o.as_deref())
                .unwrap_or("0")
                .parse()
                .unwrap_or(0),
            matches
                .get(2)
                .and_then(|o| o.as_deref())
                .unwrap_or("0")
                .parse()
                .unwrap_or(0),
            matches
                .get(3)
                .and_then(|o| o.as_deref())
                .unwrap_or("0")
                .parse()
                .unwrap_or(0),
            matches
                .get(4)
                .and_then(|o| o.as_deref())
                .unwrap_or("0")
                .parse()
                .unwrap_or(0),
        ];
        let pad_val: i64 = pad.parse().unwrap_or(0);
        let mut position = position;

        for i in (1..=4).rev() {
            if i > position {
                parts[i] = pad_val;
            } else if i == position && increment != 0 {
                parts[i] += increment;
                // If $matches[$i] was 0, carry the decrement
                if parts[i] < 0 {
                    parts[i] = pad_val;
                    // Return null on a carry overflow
                    if i == 1 {
                        return None;
                    }
                    position -= 1;
                }
            }
        }

        Some(format!(
            "{}.{}.{}.{}",
            parts[1], parts[2], parts[3], parts[4]
        ))
    }

    fn expand_stability(&self, stability: &str) -> String {
        let stability = stability.to_ascii_lowercase();

        match stability.as_str() {
            "a" => "alpha".to_string(),
            "b" => "beta".to_string(),
            "p" | "pl" => "patch".to_string(),
            "rc" => "RC".to_string(),
            _ => stability,
        }
    }
}
