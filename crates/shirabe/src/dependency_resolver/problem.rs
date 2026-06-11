//! ref: composer/src/Composer/DependencyResolver/Problem.php

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::symfony::console::formatter::OutputFormatter;
use shirabe_php_shim::{
    LogicException, PhpMixed, defined, extension_loaded, implode, in_array, php_to_string,
    phpversion, spl_object_hash, sprintf, str_replace, str_starts_with, stripos, strpos,
    strtolower, substr, substr_count, version_compare,
};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::MultiConstraint;
use shirabe_semver::constraint::SimpleConstraint;

use crate::advisory::SecurityAdvisory;
use crate::dependency_resolver::Pool;
use crate::dependency_resolver::Request;
use crate::dependency_resolver::rule::{self, Rule};
use crate::package::AliasPackage;
use crate::package::BasePackage;
use crate::package::BasePackageHandle;
use crate::package::CompletePackageInterface;
use crate::package::Link;
use crate::package::PackageInterface;
use crate::package::RootPackageInterface;
use crate::package::version::VersionParser;
use crate::repository::LockArrayRepository;
use crate::repository::PlatformRepository;
use crate::repository::RepositorySet;

/// Represents a problem detected while solving dependencies
#[derive(Debug)]
pub struct Problem {
    /// A map containing the id of each rule part of this problem as a key
    pub(crate) reason_seen: IndexMap<String, bool>,

    /// A set of reasons for the problem, each is a rule or a root require and a rule
    pub(crate) reasons: IndexMap<i64, Vec<std::rc::Rc<std::cell::RefCell<Rule>>>>,

    pub(crate) section: i64,
}

impl Problem {
    pub fn new() -> Self {
        Self {
            reason_seen: IndexMap::new(),
            reasons: IndexMap::new(),
            section: 0,
        }
    }

    /// Add a rule as a reason
    pub fn add_rule(&mut self, rule: std::rc::Rc<std::cell::RefCell<Rule>>) {
        let id = spl_object_hash(&*rule.borrow());
        self.add_reason(id, rule);
    }

    /// Retrieve all reasons for this problem
    pub fn get_reasons(&self) -> &IndexMap<i64, Vec<std::rc::Rc<std::cell::RefCell<Rule>>>> {
        &self.reasons
    }

    /// A human readable textual representation of the problem's reasons
    pub fn get_pretty_string(
        &self,
        repository_set: &RepositorySet,
        request: &Request,
        pool: &mut Pool,
        is_verbose: bool,
        installed_map: &IndexMap<String, BasePackageHandle>,
        learned_pool: &Vec<Vec<std::rc::Rc<std::cell::RefCell<Rule>>>>,
    ) -> anyhow::Result<String> {
        // TODO doesn't this entirely defeat the purpose of the problem sections? what's the point of sections?
        let mut reasons: Vec<std::rc::Rc<std::cell::RefCell<Rule>>> = Vec::new();
        for section_rules in self.reasons.values().rev() {
            for rule in section_rules {
                reasons.push(rule.clone());
            }
        }

        if reasons.len() == 1 {
            let rule = reasons[0].clone();
            let rule_ref = rule.borrow();

            if rule_ref.get_reason() != rule::RULE_ROOT_REQUIRE {
                return Err(LogicException {
                    message: "Single reason problems must contain a root require rule.".to_string(),
                    code: 0,
                }
                .into());
            }

            let reason_data = rule_ref.get_reason_data();
            let (package_name, constraint): (String, Option<&AnyConstraint>) = match reason_data {
                rule::ReasonData::RootRequire {
                    package_name,
                    constraint,
                } => (package_name.clone(), Some(constraint)),
                _ => (String::new(), None),
            };

            let packages = pool.compute_what_provides(&package_name, constraint);
            if packages.len() == 0 {
                let missing = Self::get_missing_package_reason(
                    repository_set,
                    request,
                    pool,
                    is_verbose,
                    &package_name,
                    constraint,
                )?;
                return Ok(format!("\n    {}", implode("", &[missing.0, missing.1])));
            }
        }

        reasons.sort_by(|rule1, rule2| {
            let rule1_prio = self.get_rule_priority(&rule1.borrow());
            let rule2_prio = self.get_rule_priority(&rule2.borrow());
            if rule1_prio != rule2_prio {
                return rule2_prio.cmp(&rule1_prio);
            }

            self.get_sortable_string(pool, &rule1.borrow())
                .cmp(&self.get_sortable_string(pool, &rule2.borrow()))
        });

        Self::format_deduplicated_rules(
            &reasons,
            "    ",
            repository_set,
            request,
            pool,
            is_verbose,
            installed_map,
            learned_pool,
        )
    }

    fn get_sortable_string(&self, pool: &Pool, rule: &Rule) -> String {
        match rule.get_reason() {
            rule::RULE_ROOT_REQUIRE => match rule.get_reason_data() {
                rule::ReasonData::RootRequire { package_name, .. } => package_name.clone(),
                _ => String::new(),
            },
            rule::RULE_FIXED => match rule.get_reason_data() {
                rule::ReasonData::Fixed { package } => package.get_unique_name(),
                _ => String::new(),
            },
            rule::RULE_PACKAGE_CONFLICT | rule::RULE_PACKAGE_REQUIRES => {
                let source = rule.get_source_package(pool).unwrap();
                let link_pretty = match rule.get_reason_data() {
                    rule::ReasonData::Link(link) => link.get_pretty_string(source.clone()),
                    _ => String::new(),
                };
                format!("{}//{}", source.get_unique_name(), link_pretty)
            }
            rule::RULE_PACKAGE_SAME_NAME
            | rule::RULE_PACKAGE_ALIAS
            | rule::RULE_PACKAGE_INVERSE_ALIAS => {
                // TODO(phase-c): PHP returns (string) $rule->getReasonData(), but the alias rules'
                // reason_data is still a placeholder pending the RuleSetGenerator reason_data wiring.
                format!("{:?}", rule.get_reason_data())
            }
            rule::RULE_LEARNED => implode(
                "-",
                &rule
                    .get_literals()
                    .iter()
                    .map(|l| l.to_string())
                    .collect::<Vec<_>>(),
            ),
            _ => {
                // @phpstan-ignore deadCode.unreachable
                panic!("Unknown rule type: {}", rule.get_reason());
            }
        }
    }

    fn get_rule_priority(&self, rule: &Rule) -> i64 {
        match rule.get_reason() {
            rule::RULE_FIXED => 3,
            rule::RULE_ROOT_REQUIRE => 2,
            rule::RULE_PACKAGE_CONFLICT | rule::RULE_PACKAGE_REQUIRES => 1,
            rule::RULE_PACKAGE_SAME_NAME
            | rule::RULE_LEARNED
            | rule::RULE_PACKAGE_ALIAS
            | rule::RULE_PACKAGE_INVERSE_ALIAS => 0,
            _ => {
                // @phpstan-ignore deadCode.unreachable
                panic!("Unknown rule type: {}", rule.get_reason());
            }
        }
    }

    /// @internal
    pub fn format_deduplicated_rules(
        rules: &Vec<std::rc::Rc<std::cell::RefCell<Rule>>>,
        indent: &str,
        repository_set: &RepositorySet,
        request: &Request,
        pool: &mut Pool,
        is_verbose: bool,
        installed_map: &IndexMap<String, BasePackageHandle>,
        learned_pool: &Vec<Vec<std::rc::Rc<std::cell::RefCell<Rule>>>>,
    ) -> anyhow::Result<String> {
        let mut messages: Vec<String> = Vec::new();
        let mut templates: IndexMap<String, IndexMap<String, IndexMap<String, String>>> =
            IndexMap::new();
        let parser = VersionParser::new();
        let deduplicatable_rule_types =
            vec![rule::RULE_PACKAGE_REQUIRES, rule::RULE_PACKAGE_CONFLICT];
        for rule in rules {
            let rule_ref = rule.borrow();
            let mut message = rule_ref.get_pretty_string(
                repository_set,
                request,
                pool,
                is_verbose,
                installed_map,
                learned_pool,
            )?;
            let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
            let matched = if in_array(
                PhpMixed::Int(rule_ref.get_reason()),
                &PhpMixed::List(
                    deduplicatable_rule_types
                        .iter()
                        .map(|t| Box::new(PhpMixed::Int(*t)))
                        .collect(),
                ),
                true,
            ) {
                Preg::is_match_strict_groups3(
                    r"{^(?P<package>\S+) (?P<version>\S+) (?P<type>requires|conflicts)}",
                    &message,
                    Some(&mut m),
                )
                .unwrap_or(false)
            } else {
                false
            };
            if matched {
                message = str_replace("%", "%%", &message);
                let template =
                    Preg::replace(r"{^\S+ \S+ }", "%s%s ", &message).unwrap_or(message.clone());
                messages.push(template.clone());
                let pkg_key = m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default();
                let m2 = m.get(&CaptureKey::ByIndex(2)).cloned().unwrap_or_default();
                let version_key = parser.normalize(&m2, Some("")).unwrap_or_default();
                templates
                    .entry(template.clone())
                    .or_insert_with(IndexMap::new)
                    .entry(pkg_key.clone())
                    .or_insert_with(IndexMap::new)
                    .insert(version_key, m2.clone());
                let source_package = rule_ref.get_source_package(pool).unwrap();
                for (version, pretty_version) in
                    pool.get_removed_versions_by_package(&source_package.ptr_id().to_string())
                {
                    templates
                        .get_mut(&template)
                        .unwrap()
                        .get_mut(&pkg_key)
                        .unwrap()
                        .insert(version, pretty_version);
                }
            } else if message != "" {
                messages.push(message);
            }
        }

        let mut result: Vec<String> = Vec::new();
        // array_unique preserves first-seen order
        let mut unique_messages: Vec<String> = Vec::new();
        {
            let mut seen: IndexMap<String, bool> = IndexMap::new();
            for m in &messages {
                if !seen.contains_key(m) {
                    seen.insert(m.clone(), true);
                    unique_messages.push(m.clone());
                }
            }
        }
        for message in &unique_messages {
            if templates.contains_key(message) {
                let entry = templates.get(message).unwrap().clone();
                for (package, mut versions) in entry {
                    // uksort($versions, 'version_compare')
                    let mut keys: Vec<String> = versions.keys().cloned().collect();
                    keys.sort_by(|a, b| {
                        if version_compare(a, b, "<") {
                            std::cmp::Ordering::Less
                        } else if version_compare(a, b, ">") {
                            std::cmp::Ordering::Greater
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    });
                    let mut sorted: IndexMap<String, String> = IndexMap::new();
                    for k in &keys {
                        sorted.insert(k.clone(), versions.shift_remove(k).unwrap());
                    }
                    versions = sorted;

                    let versions_list: Vec<String> = if !is_verbose {
                        Self::condense_version_list(&versions, 1, 16)
                    } else {
                        versions.values().cloned().collect()
                    };
                    if versions_list.len() > 1 {
                        // remove the s from requires/conflicts to correct grammar
                        let message_var =
                            Preg::replace(r"{^(%s%s (?:require|conflict))s}", "$1", message)
                                .unwrap_or(message.clone());
                        result.push(sprintf(
                            &message_var,
                            &[
                                PhpMixed::String(package.clone()),
                                PhpMixed::String(format!("[{}]", implode(", ", &versions_list))),
                            ],
                        ));
                    } else {
                        let first = versions_list.first().cloned().unwrap_or_default();
                        result.push(sprintf(
                            message,
                            &[
                                PhpMixed::String(package.clone()),
                                PhpMixed::String(format!(" {}", first)),
                            ],
                        ));
                    }
                }
            } else {
                result.push(message.clone());
            }
        }

        Ok(format!(
            "\n{}- {}",
            indent,
            implode(&format!("\n{}- ", indent), &result)
        ))
    }

    pub fn is_caused_by_lock(
        &self,
        repository_set: &RepositorySet,
        request: &Request,
        pool: &Pool,
    ) -> bool {
        for section_rules in self.reasons.values() {
            for rule in section_rules {
                if rule
                    .borrow()
                    .is_caused_by_lock(repository_set, request, pool)
                {
                    return true;
                }
            }
        }

        false
    }

    /// Store a reason descriptor but ignore duplicates
    pub(crate) fn add_reason(&mut self, id: String, reason: std::rc::Rc<std::cell::RefCell<Rule>>) {
        // TODO: if a rule is part of a problem description in two sections, isn't this going to remove a message
        // that is important to understand the issue?

        if !self.reason_seen.contains_key(&id) {
            self.reason_seen.insert(id, true);
            self.reasons
                .entry(self.section)
                .or_insert_with(Vec::new)
                .push(reason);
        }
    }

    pub fn next_section(&mut self) {
        self.section += 1;
    }

    /// @internal
    pub fn get_missing_package_reason(
        repository_set: &RepositorySet,
        request: &Request,
        pool: &mut Pool,
        is_verbose: bool,
        package_name: &str,
        constraint: Option<&AnyConstraint>,
    ) -> anyhow::Result<(String, String)> {
        if PlatformRepository::is_platform_package(package_name) {
            // handle php/php-*/hhvm
            if stripos(package_name, "php") == Some(0) || package_name == "hhvm" {
                let version = Self::get_platform_package_version(
                    pool,
                    package_name,
                    &phpversion("").unwrap_or_default(),
                );

                let msg = format!(
                    "- Root composer.json requires {}{} but ",
                    package_name,
                    Self::constraint_to_text(constraint)
                );

                if defined("HHVM_VERSION")
                    || (package_name == "hhvm" && pool.what_provides(package_name, None).len() > 0)
                {
                    return Ok((
                        msg,
                        "your HHVM version does not satisfy that requirement.".to_string(),
                    ));
                }

                if package_name == "hhvm" {
                    return Ok((
                        msg,
                        "HHVM was not detected on this machine, make sure it is in your PATH."
                            .to_string(),
                    ));
                }

                if version.is_none() {
                    return Ok((
                        msg,
                        format!(
                            "the {} package is disabled by your platform config. Enable it again with \"composer config platform.{} --unset\".",
                            package_name, package_name
                        ),
                    ));
                }

                return Ok((
                    msg,
                    format!(
                        "your {} version ({}) does not satisfy that requirement.",
                        package_name,
                        version.unwrap()
                    ),
                ));
            }

            // handle php extensions
            if stripos(package_name, "ext-") == Some(0) {
                if strpos(package_name, " ").is_some() {
                    return Ok((
                        "- ".to_string(),
                        format!(
                            "PHP extension {} should be required as {}.",
                            package_name,
                            str_replace(" ", "-", package_name)
                        ),
                    ));
                }

                let ext = substr(package_name, 4, None);
                let msg = format!(
                    "- Root composer.json requires PHP extension {}{} but ",
                    package_name,
                    Self::constraint_to_text(constraint)
                );

                let runtime_version = phpversion(&ext);
                let effective_version = match runtime_version {
                    None => "0".to_string(),
                    Some(ref v) => v.clone(),
                };
                let version =
                    Self::get_platform_package_version(pool, package_name, &effective_version);
                if version.is_none() {
                    let providers_str_opt =
                        Self::get_providers_list(repository_set, package_name, 5)?;
                    let providers_str = match providers_str_opt {
                        Some(ps) => format!(
                            "\n\n      Alternatively you can require one of these packages that provide the extension (or parts of it):\n      <warning>Keep in mind that the suggestions are automated and may not be valid or safe to use</warning>\n{}",
                            ps
                        ),
                        None => String::new(),
                    };

                    if extension_loaded(&ext) {
                        return Ok((
                            msg,
                            format!(
                                "the {} package is disabled by your platform config. Enable it again with \"composer config platform.{} --unset\".{}",
                                package_name, package_name, providers_str
                            ),
                        ));
                    }

                    return Ok((
                        msg,
                        format!(
                            "it is missing from your system. Install or enable PHP's {} extension.{}",
                            ext, providers_str
                        ),
                    ));
                }

                return Ok((
                    msg,
                    format!("it has the wrong version installed ({}).", version.unwrap()),
                ));
            }

            // handle linked libs
            if stripos(package_name, "lib-") == Some(0) {
                if strtolower(package_name) == "lib-icu" {
                    let error = if extension_loaded("intl") {
                        "it has the wrong version installed, try upgrading the intl extension."
                    } else {
                        "it is missing from your system, make sure the intl extension is loaded."
                    };

                    return Ok((
                        format!(
                            "- Root composer.json requires linked library {}{} but ",
                            package_name,
                            Self::constraint_to_text(constraint)
                        ),
                        error.to_string(),
                    ));
                }

                let providers_str_opt = Self::get_providers_list(repository_set, package_name, 5)?;
                let providers_str = match providers_str_opt {
                    Some(ps) => format!(
                        "\n\n      Alternatively you can require one of these packages that provide the library (or parts of it):\n      <warning>Keep in mind that the suggestions are automated and may not be valid or safe to use</warning>\n{}",
                        ps
                    ),
                    None => String::new(),
                };

                return Ok((
                    format!(
                        "- Root composer.json requires linked library {}{} but ",
                        package_name,
                        Self::constraint_to_text(constraint)
                    ),
                    format!(
                        "it has the wrong version installed or is missing from your system, make sure to load the extension providing it.{}",
                        providers_str
                    ),
                ));
            }
        }

        let mut locked_package: Option<BasePackageHandle> = None;
        for (_key, package) in request.get_locked_packages() {
            if package.get_name().as_str() == package_name {
                locked_package = Some(package.clone());
                if pool.is_unacceptable_fixed_or_locked_package(package.clone()) {
                    return Ok((
                        "- ".to_string(),
                        format!(
                            "{} is fixed to {} (lock file version) by a partial update but that version is rejected by your minimum-stability. Make sure you list it as an argument for the update command.",
                            package.get_pretty_name(),
                            package.get_pretty_version()
                        ),
                    ));
                }
                break;
            }
        }

        if let Some(c) = constraint {
            if c.is_constraint()
                && c.get_operator() == SimpleConstraint::STR_OP_EQ
                && Preg::is_match3(r"{^dev-.*#.*}", &c.get_pretty_string(), None).unwrap_or(false)
            {
                let new_constraint =
                    Preg::replace(r"{ +as +([^,\s|]+)$}", "", &c.get_pretty_string())
                        .unwrap_or_else(|_| c.get_pretty_string());
                let packages = repository_set.find_packages(
                    package_name,
                    Some(
                        MultiConstraint::new(
                            vec![
                                AnyConstraint::Simple(SimpleConstraint::new(
                                    SimpleConstraint::STR_OP_EQ.to_string(),
                                    new_constraint.clone(),
                                    None,
                                )),
                                AnyConstraint::Simple(SimpleConstraint::new(
                                    SimpleConstraint::STR_OP_EQ.to_string(),
                                    str_replace("#", "+", &new_constraint),
                                    None,
                                )),
                            ],
                            false,
                            None,
                        )
                        .into(),
                    ),
                    0,
                )?;
                if packages.len() > 0 {
                    return Ok((
                        format!(
                            "- Root composer.json requires {}{}, ",
                            package_name,
                            Self::constraint_to_text(constraint)
                        ),
                        format!(
                            "found {}. The # character in branch names is replaced by a + character. Make sure to require it as \"{}\".",
                            Self::get_package_list(
                                &packages,
                                is_verbose,
                                Some(pool),
                                constraint,
                                false
                            ),
                            str_replace("#", "+", &c.get_pretty_string())
                        ),
                    ));
                }
            }
        }

        // first check if the actual requested package is found in normal conditions
        // if so it must mean it is rejected by another constraint than the one given here
        let packages =
            repository_set.find_packages(package_name, constraint.map(|c| c.clone()), 0)?;
        if packages.len() > 0 {
            let root_reqs = repository_set.get_root_requires();
            if root_reqs.contains_key(package_name) {
                let filtered: Vec<&BasePackageHandle> = packages
                    .iter()
                    .filter(|p| {
                        root_reqs[package_name].matches(
                            &SimpleConstraint::new(
                                "==".to_string(),
                                p.get_version().to_string(),
                                None,
                            )
                            .into(),
                        )
                    })
                    .collect();
                if filtered.len() == 0 {
                    return Ok((
                        format!(
                            "- Root composer.json requires {}{}, ",
                            package_name,
                            Self::constraint_to_text(constraint)
                        ),
                        format!(
                            "found {} but {} with your root composer.json require ({}).",
                            Self::get_package_list(
                                &packages,
                                is_verbose,
                                Some(pool),
                                constraint,
                                false
                            ),
                            if Self::has_multiple_names(&packages) {
                                "these conflict"
                            } else {
                                "it conflicts"
                            },
                            root_reqs[package_name].get_pretty_string()
                        ),
                    ));
                }
            }

            let temp_reqs = repository_set.get_temporary_constraints();
            let first_pkg = packages.first().unwrap();
            for name in first_pkg.get_names(true) {
                if temp_reqs.contains_key(&name) {
                    let filtered: Vec<&BasePackageHandle> = packages
                        .iter()
                        .filter(|p| {
                            temp_reqs[&name].matches(
                                &SimpleConstraint::new(
                                    "==".to_string(),
                                    p.get_version().to_string(),
                                    None,
                                )
                                .into(),
                            )
                        })
                        .collect();
                    if filtered.len() == 0 {
                        return Ok((
                            format!(
                                "- Root composer.json requires {}{}, ",
                                name,
                                Self::constraint_to_text(constraint)
                            ),
                            format!(
                                "found {} but {} with your temporary update constraint ({}:{}).",
                                Self::get_package_list(
                                    &packages,
                                    is_verbose,
                                    Some(pool),
                                    constraint,
                                    false
                                ),
                                if Self::has_multiple_names(&packages) {
                                    "these conflict"
                                } else {
                                    "it conflicts"
                                },
                                name,
                                temp_reqs[&name].get_pretty_string()
                            ),
                        ));
                    }
                }
            }

            if let Some(ref lp) = locked_package {
                let fixed_constraint = AnyConstraint::from(SimpleConstraint::new(
                    "==".to_string(),
                    lp.get_version().to_string(),
                    None,
                ));
                let filtered: Vec<&BasePackageHandle> = packages
                    .iter()
                    .filter(|p| {
                        fixed_constraint.matches(
                            &SimpleConstraint::new(
                                "==".to_string(),
                                p.get_version().to_string(),
                                None,
                            )
                            .into(),
                        )
                    })
                    .collect();
                if filtered.len() == 0 {
                    return Ok((
                        format!(
                            "- Root composer.json requires {}{}, ",
                            package_name,
                            Self::constraint_to_text(constraint)
                        ),
                        format!(
                            "found {} but the package is fixed to {} (lock file version) by a partial update and that version does not match. Make sure you list it as an argument for the update command.",
                            Self::get_package_list(
                                &packages,
                                is_verbose,
                                Some(pool),
                                constraint,
                                false
                            ),
                            lp.get_pretty_version()
                        ),
                    ));
                }
            }

            let non_locked_packages: Vec<&BasePackageHandle> = packages
                .iter()
                .filter(|p| {
                    !p.get_repository()
                        .map_or(false, |r| r.is::<LockArrayRepository>())
                })
                .collect();

            if non_locked_packages.len() == 0 {
                return Ok((
                    format!(
                        "- Root composer.json requires {}{}, ",
                        package_name,
                        Self::constraint_to_text(constraint)
                    ),
                    format!(
                        "found {} in the lock file but not in remote repositories, make sure you avoid updating this package to keep the one from the lock file.",
                        Self::get_package_list(
                            &packages,
                            is_verbose,
                            Some(pool),
                            constraint,
                            false
                        )
                    ),
                ));
            }

            if pool.is_abandoned_removed_package_version(package_name, constraint) {
                return Ok((
                    format!(
                        "- Root composer.json requires {}{}, ",
                        package_name,
                        Self::constraint_to_text(constraint)
                    ),
                    format!(
                        "found {} but these were not loaded, because they are abandoned and you configured \"block-abandoned\" to true in your \"audit\" config.",
                        Self::get_package_list(
                            &packages,
                            is_verbose,
                            Some(pool),
                            constraint,
                            false
                        )
                    ),
                ));
            }

            if pool.is_security_removed_package_version(package_name, constraint) {
                // TODO(phase-c): get_matching_security_advisories needs Vec<PackageInterfaceHandle>
                // and SecurityAdvisory.inner.advisory_id is on the private inner field.
                // Convert packages to PackageInterfaceHandle and adjust SecurityAdvisory accessor first.
                let _ = repository_set;
                let advisories_list: Vec<String> = pool
                    .get_security_advisory_identifiers_for_package_version(package_name, constraint)
                    .into_iter()
                    .map(|advisory_id: String| {
                        if str_starts_with(&advisory_id, "PKSA-") {
                            return format!(
                                "<href={}>{}</>",
                                OutputFormatter::escape(&format!(
                                    "https://packagist.org/security-advisories/{}",
                                    advisory_id
                                ))
                                .expect("OutputFormatter::escape does not fail"),
                                advisory_id
                            );
                        }

                        advisory_id
                    })
                    .collect();

                return Ok((
                    format!(
                        "- Root composer.json requires {}{}, ",
                        package_name,
                        Self::constraint_to_text(constraint)
                    ),
                    format!(
                        "found {} but these were not loaded, because they are affected by security advisories (\"{}\"). Go to https://packagist.org/security-advisories/ to find advisory details. To ignore the advisories, add them to the audit \"ignore\" config. To turn the feature off entirely, you can set \"block-insecure\" to false in your \"audit\" config.",
                        Self::get_package_list(
                            &packages,
                            is_verbose,
                            Some(pool),
                            constraint,
                            false
                        ),
                        implode("\", \"", &advisories_list)
                    ),
                ));
            }

            return Ok((
                format!(
                    "- Root composer.json requires {}{}, ",
                    package_name,
                    Self::constraint_to_text(constraint)
                ),
                format!(
                    "found {} but these were not loaded, likely because {} with another require.",
                    Self::get_package_list(&packages, is_verbose, Some(pool), constraint, false),
                    if Self::has_multiple_names(&packages) {
                        "they conflict"
                    } else {
                        "it conflicts"
                    }
                ),
            ));
        }

        // check if the package is found when bypassing stability checks
        let packages = repository_set.find_packages(
            package_name,
            constraint.map(|c| c.clone()),
            RepositorySet::ALLOW_UNACCEPTABLE_STABILITIES,
        )?;
        if packages.len() > 0 {
            // we must first verify if a valid package would be found in a lower priority repository
            let all_repos_packages = repository_set.find_packages(
                package_name,
                constraint.map(|c| c.clone()),
                RepositorySet::ALLOW_SHADOWED_REPOSITORIES,
            )?;
            if all_repos_packages.len() > 0 {
                return Ok(Self::compute_check_for_lower_prio_repo(
                    pool,
                    is_verbose,
                    package_name,
                    &packages,
                    &all_repos_packages,
                    "minimum-stability",
                    constraint,
                ));
            }

            return Ok((
                format!(
                    "- Root composer.json requires {}{}, ",
                    package_name,
                    Self::constraint_to_text(constraint)
                ),
                format!(
                    "found {} but {} not match your minimum-stability.",
                    Self::get_package_list(&packages, is_verbose, Some(pool), constraint, false),
                    if Self::has_multiple_names(&packages) {
                        "these do"
                    } else {
                        "it does"
                    }
                ),
            ));
        }

        // check if the package is found when bypassing the constraint and stability checks
        let packages = repository_set.find_packages(
            package_name,
            None,
            RepositorySet::ALLOW_UNACCEPTABLE_STABILITIES,
        )?;
        if packages.len() > 0 {
            // we must first verify if a valid package would be found in a lower priority repository
            let all_repos_packages = repository_set.find_packages(
                package_name,
                constraint.map(|c| c.clone()),
                RepositorySet::ALLOW_SHADOWED_REPOSITORIES,
            )?;
            if all_repos_packages.len() > 0 {
                return Ok(Self::compute_check_for_lower_prio_repo(
                    pool,
                    is_verbose,
                    package_name,
                    &packages,
                    &all_repos_packages,
                    "constraint",
                    constraint,
                ));
            }

            let mut suffix = String::new();
            if let Some(c) = constraint {
                if c.is_constraint() && c.get_version() == "dev-master" {
                    for candidate in &packages {
                        if in_array(
                            PhpMixed::String(candidate.get_version().to_string()),
                            &PhpMixed::List(vec![
                                Box::new(PhpMixed::String("dev-default".to_string())),
                                Box::new(PhpMixed::String("dev-main".to_string())),
                            ]),
                            true,
                        ) {
                            suffix = format!(
                                " Perhaps dev-master was renamed to {}?",
                                candidate.get_pretty_version()
                            );
                            break;
                        }
                    }
                }
            }

            // check if the root package is a name match and hint the dependencies on root troubleshooting article
            let all_repos_packages = &packages;
            let top_package = all_repos_packages.first();
            if let Some(tp) = top_package {
                if tp.as_root().is_some() {
                    suffix = " See https://getcomposer.org/dep-on-root for details and assistance."
                        .to_string();
                }
            }

            return Ok((
                format!(
                    "- Root composer.json requires {}{}, ",
                    package_name,
                    Self::constraint_to_text(constraint)
                ),
                format!(
                    "found {} but {} not match the constraint.{}",
                    Self::get_package_list(&packages, is_verbose, Some(pool), constraint, false),
                    if Self::has_multiple_names(&packages) {
                        "these do"
                    } else {
                        "it does"
                    },
                    suffix
                ),
            ));
        }

        if !Preg::is_match3(r"{^[A-Za-z0-9_./-]+$}", package_name, None).unwrap_or(false) {
            let illegal_chars =
                Preg::replace(r"{[A-Za-z0-9_./-]+}", "", package_name).unwrap_or_default();

            return Ok((
                format!("- Root composer.json requires {}, it ", package_name),
                format!(
                    "could not be found, it looks like its name is invalid, \"{}\" is not allowed in package names.",
                    illegal_chars
                ),
            ));
        }

        let providers_str = Self::get_providers_list(repository_set, package_name, 15)?;
        if let Some(ps) = providers_str {
            return Ok((
                format!(
                    "- Root composer.json requires {}{}, it ",
                    package_name,
                    Self::constraint_to_text(constraint)
                ),
                format!(
                    "could not be found in any version, but the following packages provide it:\n{}      Consider requiring one of these to satisfy the {} requirement.",
                    ps, package_name
                ),
            ));
        }

        Ok((
            format!("- Root composer.json requires {}, it ", package_name),
            "could not be found in any version, there may be a typo in the package name."
                .to_string(),
        ))
    }

    /// @internal
    pub fn get_package_list(
        packages: &Vec<BasePackageHandle>,
        is_verbose: bool,
        pool: Option<&Pool>,
        constraint: Option<&AnyConstraint>,
        use_removed_version_group: bool,
    ) -> String {
        struct PreparedEntry {
            name: String,
            versions: IndexMap<String, String>,
        }
        let mut prepared: IndexMap<String, PreparedEntry> = IndexMap::new();
        let mut has_default_branch: IndexMap<String, bool> = IndexMap::new();
        for package in packages {
            let pkg_name = package.get_name().to_string();
            let entry = prepared
                .entry(pkg_name.clone())
                .or_insert_with(|| PreparedEntry {
                    name: package.get_pretty_name().to_string(),
                    versions: IndexMap::new(),
                });
            entry.name = package.get_pretty_name().to_string();
            let alias_suffix = if let Some(alias) = package.as_alias() {
                format!(" (alias of {})", alias.get_alias_of().get_pretty_version())
            } else {
                String::new()
            };
            entry.versions.insert(
                package.get_version().to_string(),
                format!("{}{}", package.get_pretty_version(), alias_suffix),
            );
            if pool.is_some() && constraint.is_some() {
                for (version, pretty_version) in pool
                    .unwrap()
                    .get_removed_versions(&pkg_name, constraint.unwrap())
                {
                    entry.versions.insert(version, pretty_version);
                }
            }
            if pool.is_some() && use_removed_version_group {
                for (version, pretty_version) in pool
                    .unwrap()
                    .get_removed_versions_by_package(&package.ptr_id().to_string())
                {
                    entry.versions.insert(version, pretty_version);
                }
            }
            if package.is_default_branch() {
                has_default_branch.insert(pkg_name, true);
            }
        }

        let mut prepared_strings: Vec<String> = Vec::new();
        for (name, mut package) in prepared {
            // remove the implicit default branch alias to avoid cruft in the display
            if package
                .versions
                .contains_key(VersionParser::DEFAULT_BRANCH_ALIAS)
                && has_default_branch.contains_key(&name)
            {
                package
                    .versions
                    .shift_remove(VersionParser::DEFAULT_BRANCH_ALIAS);
            }

            // uksort($package['versions'], 'version_compare')
            let mut keys: Vec<String> = package.versions.keys().cloned().collect();
            keys.sort_by(|a, b| {
                if version_compare(a, b, "<") {
                    std::cmp::Ordering::Less
                } else if version_compare(a, b, ">") {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            });
            let mut sorted: IndexMap<String, String> = IndexMap::new();
            for k in &keys {
                sorted.insert(k.clone(), package.versions.shift_remove(k).unwrap());
            }
            package.versions = sorted;

            let version_list: Vec<String> = if !is_verbose {
                Self::condense_version_list(&package.versions, 4, 16)
            } else {
                package.versions.values().cloned().collect()
            };
            prepared_strings.push(format!(
                "{}[{}]",
                package.name,
                implode(", ", &version_list)
            ));
        }

        implode(", ", &prepared_strings)
    }

    /// @param string $version the effective runtime version of the platform package
    /// @return ?string a version string or null if it appears the package was artificially disabled
    fn get_platform_package_version(
        pool: &mut Pool,
        package_name: &str,
        version: &str,
    ) -> Option<String> {
        let available = pool.what_provides(package_name, None);

        if available.len() > 0 {
            let mut selected: Option<&BasePackageHandle> = None;
            for pkg in &available {
                if pkg
                    .get_repository()
                    .map_or(false, |r| r.is::<PlatformRepository>())
                {
                    selected = Some(pkg);
                    break;
                }
            }
            if selected.is_none() {
                selected = available.first();
            }
            let selected = selected.unwrap();

            // must be a package providing/replacing and not a real platform package
            if selected.get_name() != package_name {
                let provides: Vec<Link> = selected.get_provides().values().cloned().collect();
                let replaces: Vec<Link> = selected.get_replaces().values().cloned().collect();
                for link in provides.iter().chain(replaces.iter()) {
                    if link.get_target() == package_name {
                        return Some(format!(
                            "{} {}d by {}",
                            link.get_pretty_constraint(),
                            substr(link.get_description(), 0, Some(-1)),
                            selected.get_pretty_string()
                        ));
                    }
                }
            }

            let mut version: String = selected.get_pretty_version().to_string();
            let extra = selected.get_extra();
            if selected.as_complete().is_some()
                && extra.contains_key("config.platform")
                && extra["config.platform"].as_bool() == Some(true)
            {
                let description: String = selected
                    .as_complete()
                    .and_then(|c| c.get_description())
                    .unwrap_or_default();
                version = format!("{}; {}", version, str_replace("Package ", "", &description));
            }
            return Some(version);
        }

        let _ = version;
        None
    }

    /// @param array<string|int, string> $versions an array of pretty versions, with normalized versions as keys
    /// @return list<string> a list of pretty versions and '...' where versions were removed
    fn condense_version_list(
        versions: &IndexMap<String, String>,
        max: i64,
        max_dev: i64,
    ) -> Vec<String> {
        if (versions.len() as i64) <= max {
            return versions.values().cloned().collect();
        }

        let mut filtered: Vec<String> = Vec::new();
        let mut by_major: IndexMap<String, Vec<String>> = IndexMap::new();
        for (version, pretty) in versions {
            if stripos(version, "dev-") == Some(0) {
                by_major
                    .entry("dev".to_string())
                    .or_insert_with(Vec::new)
                    .push(pretty.clone());
            } else {
                let key = Preg::replace(r"{^(\d+)\..*}", "$1", version)
                    .unwrap_or_else(|_| version.clone());
                by_major
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(pretty.clone());
            }
        }
        for (major_version, versions_for_major) in by_major {
            let max_versions = if major_version == "dev" { max_dev } else { max };
            if (versions_for_major.len() as i64) > max_versions {
                // output only 1st and last versions
                filtered.push(versions_for_major[0].clone());
                filtered.push("...".to_string());
                filtered.push(versions_for_major[versions_for_major.len() - 1].clone());
            } else {
                for v in versions_for_major {
                    filtered.push(v);
                }
            }
        }

        filtered
    }

    fn has_multiple_names(packages: &Vec<BasePackageHandle>) -> bool {
        let mut name: Option<String> = None;
        for package in packages {
            if name.is_none() || name.as_deref() == Some(package.get_name().as_str()) {
                name = Some(package.get_name().to_string());
            } else {
                return true;
            }
        }

        false
    }

    fn compute_check_for_lower_prio_repo(
        pool: &Pool,
        is_verbose: bool,
        package_name: &str,
        higher_repo_packages: &Vec<BasePackageHandle>,
        all_repos_packages: &Vec<BasePackageHandle>,
        reason: &str,
        constraint: Option<&AnyConstraint>,
    ) -> (String, String) {
        let mut next_repo_packages: Vec<BasePackageHandle> = Vec::new();
        let mut next_repo: Option<crate::repository::RepositoryInterfaceHandle> = None;
        for package in all_repos_packages {
            let pkg_repo = package.get_repository();
            let same_repo = match (&next_repo, &pkg_repo) {
                (None, _) => true,
                (Some(nr), Some(pr)) => nr.ptr_eq(pr),
                _ => false,
            };
            if same_repo {
                next_repo_packages.push(package.clone());
                next_repo = pkg_repo;
            } else {
                break;
            }
        }
        let next_repo = next_repo.expect("next_repo must be set");

        if higher_repo_packages.len() > 0 {
            let top_package = higher_repo_packages.first().unwrap();
            if top_package.as_root().is_some() {
                return (
                    format!(
                        "- Root composer.json requires {}{}, it is ",
                        package_name,
                        Self::constraint_to_text(constraint)
                    ),
                    format!(
                        "satisfiable by {} from {} but {} {} is the root package and cannot be modified. See https://getcomposer.org/dep-on-root for details and assistance.",
                        Self::get_package_list(
                            &next_repo_packages,
                            is_verbose,
                            Some(pool),
                            constraint,
                            false
                        ),
                        next_repo.get_repo_name(),
                        top_package.get_pretty_name(),
                        top_package.get_pretty_version()
                    ),
                );
            }
        }

        if next_repo.is::<LockArrayRepository>() {
            let singular = higher_repo_packages.len() == 1;

            let mut suggestion = format!(
                "Make sure you either fix the {} or avoid updating this package to keep the one present in the lock file ({}).",
                reason,
                Self::get_package_list(
                    &next_repo_packages,
                    is_verbose,
                    Some(pool),
                    constraint,
                    false
                )
            );
            // symlinked path repos cannot be locked so do not suggest keeping it locked
            if next_repo_packages[0].get_dist_type() == Some("path".to_string()) {
                let transport_options = next_repo_packages[0].get_transport_options();
                if !transport_options.contains_key("symlink")
                    || transport_options["symlink"].as_bool() != Some(false)
                {
                    suggestion = format!(
                        "Make sure you fix the {} as packages installed from symlinked path repos are updated even in partial updates and the one from the lock file can thus not be used.",
                        reason
                    );
                }
            }

            return (
                format!(
                    "- Root composer.json requires {}{}, ",
                    package_name,
                    Self::constraint_to_text(constraint)
                ),
                format!(
                    "found {} but {} not match your {} and {} therefore not installable. {}",
                    Self::get_package_list(
                        higher_repo_packages,
                        is_verbose,
                        Some(pool),
                        constraint,
                        false
                    ),
                    if singular { "it does" } else { "these do" },
                    reason,
                    if singular { "is" } else { "are" },
                    suggestion
                ),
            );
        }

        (
            format!(
                "- Root composer.json requires {}{}, it is ",
                package_name,
                Self::constraint_to_text(constraint)
            ),
            format!(
                "satisfiable by {} from {} but {} from {} has higher repository priority. The packages from the higher priority repository do not match your {} and are therefore not installable. That repository is canonical so the lower priority repo's packages are not installable. See https://getcomposer.org/repoprio for details and assistance.",
                Self::get_package_list(
                    &next_repo_packages,
                    is_verbose,
                    Some(pool),
                    constraint,
                    false
                ),
                next_repo.get_repo_name(),
                Self::get_package_list(
                    higher_repo_packages,
                    is_verbose,
                    Some(pool),
                    constraint,
                    false
                ),
                higher_repo_packages
                    .first()
                    .and_then(|p| p.get_repository())
                    .map(|r| r.get_repo_name())
                    .unwrap_or_default(),
                reason
            ),
        )
    }

    /// Turns a constraint into text usable in a sentence describing a request
    pub(crate) fn constraint_to_text(constraint: Option<&AnyConstraint>) -> String {
        if let Some(c) = constraint {
            if c.is_constraint()
                && c.get_operator() == SimpleConstraint::STR_OP_EQ
                && !str_starts_with(&c.get_version(), "dev-")
            {
                if !Preg::is_match3(r"{^\d+(?:\.\d+)*$}", &c.get_pretty_string(), None)
                    .unwrap_or(false)
                {
                    return format!(" {} (exact version match)", c.get_pretty_string());
                }

                let mut versions = vec![c.get_pretty_string()];
                let mut i = 3 - substr_count(&versions[0], ".");
                while i > 0 {
                    let last = versions.last().unwrap().clone();
                    versions.push(format!("{}.0", last));
                    i -= 1;
                }

                let last = versions.last().unwrap().clone();
                let detail = if versions.len() > 1 {
                    format!(
                        "{} or {}",
                        implode(
                            ", ",
                            &versions[..versions.len() - 1]
                                .iter()
                                .cloned()
                                .collect::<Vec<_>>()
                        ),
                        last
                    )
                } else {
                    versions[0].clone()
                };
                return format!(
                    " {} (exact version match: {})",
                    c.get_pretty_string(),
                    detail
                );
            }
        }

        match constraint {
            Some(c) => format!(" {}", c.get_pretty_string()),
            None => String::new(),
        }
    }

    fn get_providers_list(
        repository_set: &RepositorySet,
        package_name: &str,
        max_providers: i64,
    ) -> anyhow::Result<Option<String>> {
        let providers = repository_set.get_providers(package_name)?;
        if providers.len() > 0 {
            let provider_count = providers.len() as i64;
            let slice: Vec<crate::repository::ProviderInfo> = if provider_count > max_providers + 1
            {
                providers
                    .values()
                    .take(max_providers as usize)
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                providers.values().cloned().collect::<Vec<_>>()
            };
            let mut providers_str = implode(
                "",
                &slice
                    .iter()
                    .map(|p| {
                        let description = match &p.description {
                            Some(d) if !d.is_empty() => format!(" {}", substr(d, 0, Some(100))),
                            _ => String::new(),
                        };

                        format!("      - {}{}\n", p.name, description)
                    })
                    .collect::<Vec<_>>(),
            );
            if provider_count > max_providers + 1 {
                providers_str.push_str(&format!(
                    "      ... and {} more.\n",
                    provider_count - max_providers
                ));
            }

            return Ok(Some(providers_str));
        }

        Ok(None)
    }
}
