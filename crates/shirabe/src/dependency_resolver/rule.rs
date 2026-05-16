//! ref: composer/src/Composer/DependencyResolver/Rule.php

use std::any::Any;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    abs, array_filter, array_keys, array_shift, array_values, implode, is_object, LogicException,
    PhpMixed,
};
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::problem::Problem;
use crate::dependency_resolver::request::Request;
use crate::dependency_resolver::rule_set::RuleSet;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::BasePackage;
use crate::package::link::Link;
use crate::package::version::version_parser::VersionParser;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_set::RepositorySet;

/// PHP: @phpstan-type ReasonData = Link|BasePackage|string|int|array{...}|array{...}
/// We model this as an enum.
#[derive(Debug)]
pub enum ReasonData {
    Link(Link),
    BasePackage(Box<BasePackage>),
    String(String),
    Int(i64),
    RootRequire {
        package_name: String,
        constraint: Box<dyn ConstraintInterface>,
    },
    Fixed {
        package: Box<BasePackage>,
    },
}

/// @phpstan-type ReasonData Link|BasePackage|string|int|array{packageName: string, constraint: ConstraintInterface}|array{package: BasePackage}
#[derive(Debug)]
pub struct Rule {
    /// @var int
    pub(crate) bitfield: i64,
    /// @var Request
    pub(crate) request: Option<Request>,
    /// @var Link|BasePackage|ConstraintInterface|string
    /// @phpstan-var ReasonData
    pub(crate) reason_data: ReasonData,
}

impl Rule {
    // reason constants and // their reason data contents
    pub const RULE_ROOT_REQUIRE: i64 = 2;
    pub const RULE_FIXED: i64 = 3;
    pub const RULE_PACKAGE_CONFLICT: i64 = 6;
    pub const RULE_PACKAGE_REQUIRES: i64 = 7;
    pub const RULE_PACKAGE_SAME_NAME: i64 = 10;
    pub const RULE_LEARNED: i64 = 12;
    pub const RULE_PACKAGE_ALIAS: i64 = 13;
    pub const RULE_PACKAGE_INVERSE_ALIAS: i64 = 14;

    // bitfield defs
    const BITFIELD_TYPE: i64 = 0;
    const BITFIELD_REASON: i64 = 8;
    const BITFIELD_DISABLED: i64 = 16;

    /// @param self::RULE_* $reason     A RULE_* constant describing the reason for generating this rule
    /// @param mixed        $reasonData
    ///
    /// @phpstan-param ReasonData $reasonData
    pub fn new(reason: i64, reason_data: ReasonData) -> Self {
        let bitfield = (0i64 << Self::BITFIELD_DISABLED)
            | (reason << Self::BITFIELD_REASON)
            | (255i64 << Self::BITFIELD_TYPE);
        Self {
            bitfield,
            request: None,
            reason_data,
        }
    }

    /// @return self::RULE_*
    pub fn get_reason(&self) -> i64 {
        (self.bitfield & (255 << Self::BITFIELD_REASON)) >> Self::BITFIELD_REASON
    }

    /// @phpstan-return ReasonData
    pub fn get_reason_data(&self) -> &ReasonData {
        &self.reason_data
    }

    pub fn get_required_package(&self) -> Option<String> {
        match self.get_reason() {
            r if r == Self::RULE_ROOT_REQUIRE => match self.get_reason_data() {
                ReasonData::RootRequire { package_name, .. } => Some(package_name.clone()),
                _ => None,
            },
            r if r == Self::RULE_FIXED => match self.get_reason_data() {
                ReasonData::Fixed { package } => Some(package.get_name().to_string()),
                _ => None,
            },
            r if r == Self::RULE_PACKAGE_REQUIRES => match self.get_reason_data() {
                ReasonData::Link(link) => Some(link.get_target().to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    /// @param RuleSet::TYPE_* $type
    pub fn set_type(&mut self, r#type: i64) {
        self.bitfield = (self.bitfield & !(255i64 << Self::BITFIELD_TYPE))
            | ((255 & r#type) << Self::BITFIELD_TYPE);
    }

    pub fn get_type(&self) -> i64 {
        (self.bitfield & (255 << Self::BITFIELD_TYPE)) >> Self::BITFIELD_TYPE
    }

    pub fn disable(&mut self) {
        self.bitfield = (self.bitfield & !(255i64 << Self::BITFIELD_DISABLED))
            | (1i64 << Self::BITFIELD_DISABLED);
    }

    pub fn enable(&mut self) {
        self.bitfield &= !(255i64 << Self::BITFIELD_DISABLED);
    }

    pub fn is_disabled(&self) -> bool {
        0 != ((self.bitfield & (255 << Self::BITFIELD_DISABLED)) >> Self::BITFIELD_DISABLED)
    }

    pub fn is_enabled(&self) -> bool {
        0 == ((self.bitfield & (255 << Self::BITFIELD_DISABLED)) >> Self::BITFIELD_DISABLED)
    }

    pub fn is_caused_by_lock(
        &self,
        _repository_set: &RepositorySet,
        request: &Request,
        pool: &Pool,
    ) -> bool {
        if self.get_reason() == Self::RULE_PACKAGE_REQUIRES {
            if let ReasonData::Link(link) = self.get_reason_data() {
                if PlatformRepository::is_platform_package(link.get_target()) {
                    return false;
                }
                // TODO(phase-b): Request::get_locked_repository() signature
                if let Some(locked_repo) = todo!("request.get_locked_repository()") {
                    for package in todo!("locked_repo.get_packages()") {
                        let p: &BasePackage = todo!("package as BasePackage reference");
                        if p.get_name() == link.get_target() {
                            if pool.is_unacceptable_fixed_or_locked_package(p) {
                                return true;
                            }
                            if !link.get_constraint().matches(&Constraint::new("=", p.get_version()))
                            {
                                return true;
                            }
                            // required package was locked but has been unlocked and still matches
                            if !request.is_locked_package(todo!("package as &dyn PackageInterface"))
                            {
                                return true;
                            }
                            break;
                        }
                    }
                }
            }
        }

        if self.get_reason() == Self::RULE_ROOT_REQUIRE {
            if let ReasonData::RootRequire { package_name, constraint } = self.get_reason_data() {
                if PlatformRepository::is_platform_package(package_name) {
                    return false;
                }
                // TODO(phase-b): Request::get_locked_repository() signature
                if let Some(locked_repo) = todo!("request.get_locked_repository()") {
                    for package in todo!("locked_repo.get_packages()") {
                        let p: &BasePackage = todo!("package as BasePackage reference");
                        if p.get_name() == package_name {
                            if pool.is_unacceptable_fixed_or_locked_package(p) {
                                return true;
                            }
                            if !constraint.matches(&Constraint::new("=", p.get_version())) {
                                return true;
                            }
                            break;
                        }
                    }
                }
            }
        }

        false
    }

    /// @internal
    pub fn get_source_package(&self, pool: &Pool) -> Result<Box<BasePackage>> {
        let literals = self.get_literals();

        match self.get_reason() {
            r if r == Self::RULE_PACKAGE_CONFLICT => {
                let mut package1 =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[0]));
                let mut package2 =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[1]));

                let reason_data = self.get_reason_data();
                // swap literals if they are not in the right order with package2 being the conflicter
                if let ReasonData::Link(link) = reason_data {
                    if link.get_source() == package1.get_name() {
                        std::mem::swap(&mut package1, &mut package2);
                    }
                }

                Ok(package2)
            }

            r if r == Self::RULE_PACKAGE_REQUIRES => {
                let source_literal = literals[0];
                let source_package =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(source_literal));

                Ok(source_package)
            }

            _ => Err(LogicException {
                message: "Not implemented".to_string(),
                code: 0,
            }
            .into()),
        }
    }

    /// @param BasePackage[] $installedMap
    /// @param array<Rule[]> $learnedPool
    pub fn get_pretty_string(
        &self,
        repository_set: &RepositorySet,
        request: &Request,
        pool: &mut Pool,
        is_verbose: bool,
        installed_map: IndexMap<i64, Box<BasePackage>>,
        _learned_pool: IndexMap<i64, Vec<Box<dyn RuleTrait>>>,
    ) -> String {
        let mut literals = self.get_literals();

        match self.get_reason() {
            r if r == Self::RULE_ROOT_REQUIRE => {
                let reason_data = self.get_reason_data();
                let (package_name, constraint): (&str, &dyn ConstraintInterface) = match reason_data
                {
                    ReasonData::RootRequire { package_name, constraint } => {
                        (package_name.as_str(), constraint.as_ref())
                    }
                    _ => return String::new(),
                };

                let packages = pool.what_provides(package_name, Some(constraint));
                if 0 == packages.len() {
                    return format!(
                        "No package found to satisfy root composer.json require {} {}",
                        package_name,
                        constraint.get_pretty_string(),
                    );
                }

                // PHP: array_values(array_filter($packages, fn ($p) => !($p instanceof AliasPackage)))
                let packages_non_alias: Vec<Box<BasePackage>> = packages
                    .iter()
                    .filter(|p| {
                        (p.as_any() as &dyn Any).downcast_ref::<AliasPackage>().is_none()
                    })
                    .map(|p| p.clone_box())
                    .collect();
                if packages_non_alias.len() == 1 {
                    let package = &packages_non_alias[0];
                    // TODO(phase-b): request.is_locked_package signature
                    if request.is_locked_package(todo!("package as &dyn PackageInterface")) {
                        return format!(
                            "{} is locked to version {} and an update of this package was not requested.",
                            package.get_pretty_name(),
                            package.get_pretty_version(),
                        );
                    }
                }

                format!(
                    "Root composer.json requires {} {} -> satisfiable by {}.",
                    package_name,
                    constraint.get_pretty_string(),
                    self.format_packages_unique(pool, packages, is_verbose, Some(constraint), false),
                )
            }

            r if r == Self::RULE_FIXED => {
                let package_in = match self.get_reason_data() {
                    ReasonData::Fixed { package } => package.clone_box(),
                    _ => return String::new(),
                };
                let package = self.deduplicate_default_branch_alias(package_in);

                if request.is_locked_package(todo!("package as &dyn PackageInterface")) {
                    return format!(
                        "{} is locked to version {} and an update of this package was not requested.",
                        package.get_pretty_name(),
                        package.get_pretty_version(),
                    );
                }

                format!(
                    "{} is present at version {} and cannot be modified by Composer",
                    package.get_pretty_name(),
                    package.get_pretty_version(),
                )
            }

            r if r == Self::RULE_PACKAGE_CONFLICT => {
                let mut package1 =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[0]));
                let mut package2 =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[1]));

                let mut conflict_target = package1.get_pretty_string();
                let reason_data = self.get_reason_data();

                let link = match reason_data {
                    ReasonData::Link(l) => l,
                    _ => return String::new(),
                };
                // swap literals if they are not in the right order with package2 being the conflicter
                if link.get_source() == package1.get_name() {
                    std::mem::swap(&mut package1, &mut package2);
                    conflict_target =
                        format!("{} {}", package1.get_pretty_name(), link.get_pretty_constraint().unwrap_or(""));
                }

                // if the conflict is not directly against the package but something it provides/replaces,
                // we try to find that link to display a better message
                if link.get_target() != package1.get_name() {
                    let mut provide_type: Option<&str> = None;
                    let mut provided: Option<String> = None;
                    for provide in package1.get_provides().values() {
                        if provide.get_target() == link.get_target() {
                            provide_type = Some("provides");
                            provided = Some(provide.get_pretty_constraint().unwrap_or("").to_string());
                            break;
                        }
                    }
                    for replace in package1.get_replaces().values() {
                        if replace.get_target() == link.get_target() {
                            provide_type = Some("replaces");
                            provided = Some(replace.get_pretty_constraint().unwrap_or("").to_string());
                            break;
                        }
                    }
                    if let Some(pt) = provide_type {
                        conflict_target = format!(
                            "{} {} ({} {} {} {})",
                            link.get_target(),
                            link.get_pretty_constraint().unwrap_or(""),
                            package1.get_pretty_string(),
                            pt,
                            link.get_target(),
                            provided.unwrap_or_default(),
                        );
                    }
                }

                format!("{} conflicts with {}.", package2.get_pretty_string(), conflict_target)
            }

            r if r == Self::RULE_PACKAGE_REQUIRES => {
                assert!(literals.len() > 0);
                let source_literal = array_shift(&mut literals).unwrap();
                let source_package =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(source_literal));
                let reason_data = self.get_reason_data();
                let link = match reason_data {
                    ReasonData::Link(l) => l,
                    _ => return String::new(),
                };

                let mut requires: Vec<Box<BasePackage>> = vec![];
                for literal in &literals {
                    requires.push(pool.literal_to_package(*literal));
                }

                let text = link.get_pretty_string(&*source_package);
                if requires.len() > 0 {
                    format!(
                        "{} -> satisfiable by {}.",
                        text,
                        self.format_packages_unique(
                            pool,
                            requires,
                            is_verbose,
                            Some(link.get_constraint()),
                            false,
                        ),
                    )
                } else {
                    let target_name = link.get_target();

                    let reason = Problem::get_missing_package_reason(
                        repository_set,
                        request,
                        pool,
                        is_verbose,
                        target_name,
                        Some(link.get_constraint()),
                    );

                    return format!("{} -> {}", text, reason.1);
                }
            }

            r if r == Self::RULE_PACKAGE_SAME_NAME => {
                let mut package_names: IndexMap<String, bool> = IndexMap::new();
                for literal in &literals {
                    let package = pool.literal_to_package(*literal);
                    package_names.insert(package.get_name().to_string(), true);
                }
                // PHP: unset($literal);
                let replaced_name = match self.get_reason_data() {
                    ReasonData::String(s) => s.clone(),
                    _ => String::new(),
                };

                if package_names.len() > 1 {
                    let reason = if !package_names.contains_key(&replaced_name) {
                        format!(
                            "They {} replace {} and thus cannot coexist.",
                            if literals.len() == 2 { "both" } else { "all" },
                            replaced_name,
                        )
                    } else {
                        let mut replacer_names = package_names.clone();
                        replacer_names.shift_remove(&replaced_name);
                        let replacer_names = array_keys(&replacer_names);

                        let mut reason_str = if replacer_names.len() == 1 {
                            format!("{} replaces ", replacer_names[0])
                        } else {
                            format!("[{}] replace ", implode(", ", &replacer_names))
                        };
                        reason_str.push_str(&format!(
                            "{} and thus cannot coexist with it.",
                            replaced_name,
                        ));
                        reason_str
                    };

                    let mut installed_packages: Vec<Box<BasePackage>> = vec![];
                    let mut removable_packages: Vec<Box<BasePackage>> = vec![];
                    for literal in &literals {
                        if installed_map.contains_key(&abs(*literal)) {
                            installed_packages.push(pool.literal_to_package(*literal));
                        } else {
                            removable_packages.push(pool.literal_to_package(*literal));
                        }
                    }

                    if installed_packages.len() > 0 && removable_packages.len() > 0 {
                        return format!(
                            "{} cannot be installed as that would require removing {}. {}",
                            self.format_packages_unique(
                                pool,
                                removable_packages,
                                is_verbose,
                                None,
                                true,
                            ),
                            self.format_packages_unique(
                                pool,
                                installed_packages,
                                is_verbose,
                                None,
                                true,
                            ),
                            reason,
                        );
                    }

                    return format!(
                        "Only one of these can be installed: {}. {}",
                        self.format_packages_unique_from_literals(pool, &literals, is_verbose, None, true),
                        reason,
                    );
                }

                format!(
                    "You can only install one version of a package, so only one of these can be installed: {}.",
                    self.format_packages_unique_from_literals(pool, &literals, is_verbose, None, true),
                )
            }
            r if r == Self::RULE_LEARNED => {
                /// @TODO currently still generates way too much output to be helpful, and in some cases can even lead to endless recursion
                // (PHP commented-out alternative code preserved)
                let learned_string = " (conflict analysis result)";

                let rule_text = if literals.len() == 1 {
                    pool.literal_to_pretty_string(literals[0], &installed_map)
                } else {
                    let mut groups: IndexMap<String, Vec<Box<BasePackage>>> = IndexMap::new();
                    for literal in &literals {
                        let package = pool.literal_to_package(*literal);
                        let group = if installed_map.contains_key(&package.id) {
                            if *literal > 0 { "keep" } else { "remove" }
                        } else {
                            if *literal > 0 { "install" } else { "don't install" }
                        };

                        groups
                            .entry(group.to_string())
                            .or_insert_with(Vec::new)
                            .push(self.deduplicate_default_branch_alias(package));
                    }
                    let mut rule_texts: Vec<String> = vec![];
                    for (group, packages) in &groups {
                        rule_texts.push(format!(
                            "{}{} {}",
                            group,
                            if packages.len() > 1 { " one of" } else { "" },
                            self.format_packages_unique(
                                pool,
                                packages.iter().map(|p| p.clone_box()).collect(),
                                is_verbose,
                                None,
                                false,
                            ),
                        ));
                    }

                    implode(" | ", &rule_texts)
                };

                format!("Conclusion: {}{}", rule_text, learned_string)
            }
            r if r == Self::RULE_PACKAGE_ALIAS => {
                let alias_package = pool.literal_to_package(literals[0]);

                // avoid returning content like "9999999-dev is an alias of dev-master" as it is useless
                if alias_package.get_version() == VersionParser::DEFAULT_BRANCH_ALIAS {
                    return String::new();
                }
                let package =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[1]));

                format!(
                    "{} is an alias of {} and thus requires it to be installed too.",
                    alias_package.get_pretty_string(),
                    package.get_pretty_string(),
                )
            }
            r if r == Self::RULE_PACKAGE_INVERSE_ALIAS => {
                // inverse alias rules work the other way around than above
                let alias_package = pool.literal_to_package(literals[1]);

                // avoid returning content like "9999999-dev is an alias of dev-master" as it is useless
                if alias_package.get_version() == VersionParser::DEFAULT_BRANCH_ALIAS {
                    return String::new();
                }
                let package =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[0]));

                format!(
                    "{} is an alias of {} and must be installed with it.",
                    alias_package.get_pretty_string(),
                    package.get_pretty_string(),
                )
            }
            _ => {
                let mut rule_text = String::new();
                for (i, literal) in literals.iter().enumerate() {
                    if i != 0 {
                        rule_text.push('|');
                    }
                    rule_text.push_str(&pool.literal_to_pretty_string(*literal, &installed_map));
                }

                format!("({})", rule_text)
            }
        }
    }

    /// @param array<int|BasePackage> $literalsOrPackages An array containing packages or literals
    pub(crate) fn format_packages_unique(
        &self,
        pool: &Pool,
        literals_or_packages: Vec<Box<BasePackage>>,
        is_verbose: bool,
        constraint: Option<&dyn ConstraintInterface>,
        use_removed_version_group: bool,
    ) -> String {
        let mut packages: Vec<Box<BasePackage>> = vec![];
        for package in literals_or_packages {
            // PHP: \is_object($package) ? $package : $pool->literalToPackage($package);
            // In Rust we already have BasePackage, so no conversion needed.
            packages.push(package);
        }

        Problem::get_package_list(packages, is_verbose, pool, constraint, use_removed_version_group)
    }

    /// Helper for cases where literals come as int IDs (PHP supports both via union).
    fn format_packages_unique_from_literals(
        &self,
        pool: &Pool,
        literals: &[i64],
        is_verbose: bool,
        constraint: Option<&dyn ConstraintInterface>,
        use_removed_version_group: bool,
    ) -> String {
        let mut packages: Vec<Box<BasePackage>> = vec![];
        for literal in literals {
            packages.push(pool.literal_to_package(*literal).clone_box());
        }
        Problem::get_package_list(packages, is_verbose, pool, constraint, use_removed_version_group)
    }

    fn deduplicate_default_branch_alias(&self, package: Box<BasePackage>) -> Box<BasePackage> {
        if let Some(alias_pkg) =
            (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>()
        {
            if alias_pkg.get_pretty_version() == VersionParser::DEFAULT_BRANCH_ALIAS {
                return alias_pkg.get_alias_of().clone_box();
            }
        }

        package
    }
}

/// PHP abstract methods on Rule — concrete subclasses must implement.
pub trait RuleTrait: std::any::Any {
    /// @return list<int>
    fn get_literals(&self) -> Vec<i64>;

    /// @return int|string
    fn get_hash(&self) -> PhpMixed;

    fn to_string(&self) -> String;

    fn equals(&self, rule: &dyn RuleTrait) -> bool;

    fn is_assertion(&self) -> bool;
}

// TODO(phase-b): abstract method dispatch — currently Rule has stubs that subclasses override.
impl Rule {
    pub fn get_literals(&self) -> Vec<i64> {
        todo!("abstract: implemented by subclass")
    }

    pub fn get_hash(&self) -> PhpMixed {
        todo!("abstract: implemented by subclass")
    }

    pub fn equals(&self, _rule: &Rule) -> bool {
        todo!("abstract: implemented by subclass")
    }

    pub fn is_assertion(&self) -> bool {
        todo!("abstract: implemented by subclass")
    }
}

impl std::fmt::Display for Rule {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO(phase-b): abstract; subclasses provide __toString
        todo!("abstract: implemented by subclass")
    }
}
