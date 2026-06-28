//! ref: composer/src/Composer/DependencyResolver/Rule.php

use crate::dependency_resolver::GenericRule;
use crate::dependency_resolver::MultiConflictRule;
use crate::dependency_resolver::Pool;
use crate::dependency_resolver::Problem;
use crate::dependency_resolver::Request;
use crate::dependency_resolver::Rule2Literals;
use crate::package::BasePackageHandle;
use crate::package::Link;
use crate::package::version::VersionParser;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositorySet;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    LogicException, PhpMixed, RuntimeException, array_keys, array_shift, implode,
};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub enum ReasonData {
    Link(Link),
    BasePackage(BasePackageHandle),
    String(String),
    Int(i64),
    RootRequire {
        package_name: String,
        constraint: AnyConstraint,
    },
    Fixed {
        package: BasePackageHandle,
    },
}

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
pub const BITFIELD_TYPE: i64 = 0;
pub const BITFIELD_REASON: i64 = 8;
pub const BITFIELD_DISABLED: i64 = 16;

#[derive(Debug)]
pub enum Rule {
    Generic(GenericRule),
    MultiConflict(MultiConflictRule),
    TwoLiterals(Rule2Literals),
}

impl Rule {
    fn base(&self) -> &RuleBase {
        match self {
            Rule::Generic(r) => r.base(),
            Rule::MultiConflict(r) => r.base(),
            Rule::TwoLiterals(r) => r.base(),
        }
    }

    fn base_mut(&mut self) -> &mut RuleBase {
        match self {
            Rule::Generic(r) => r.base_mut(),
            Rule::MultiConflict(r) => r.base_mut(),
            Rule::TwoLiterals(r) => r.base_mut(),
        }
    }

    fn bitfield(&self) -> i64 {
        self.base().bitfield
    }

    fn bitfield_mut(&mut self) -> &mut i64 {
        &mut self.base_mut().bitfield
    }

    fn reason_data(&self) -> &ReasonData {
        &self.base().reason_data
    }

    pub fn get_literals(&self) -> Vec<i64> {
        match self {
            Rule::Generic(r) => r.get_literals().clone(),
            Rule::MultiConflict(r) => r.get_literals().clone(),
            // PHP Rule2Literals::getLiterals returns [$literal1, $literal2].
            Rule::TwoLiterals(r) => vec![r.literal1, r.literal2],
        }
    }

    pub fn get_hash(&self) -> Result<PhpMixed> {
        match self {
            Rule::Generic(r) => Ok(PhpMixed::Int(r.get_hash()?)),
            Rule::MultiConflict(r) => Ok(PhpMixed::Int(r.get_hash()?)),
            Rule::TwoLiterals(r) => Ok(PhpMixed::String(r.get_hash())),
        }
    }

    pub fn equals(&self, rule: &Rule) -> bool {
        match self {
            Rule::Generic(r) => r.equals(rule),
            Rule::MultiConflict(r) => r.equals(rule),
            Rule::TwoLiterals(r) => r.equals(rule),
        }
    }

    pub fn is_assertion(&self) -> bool {
        match self {
            Rule::Generic(r) => r.is_assertion(),
            Rule::MultiConflict(r) => r.is_assertion(),
            Rule::TwoLiterals(r) => r.is_assertion(),
        }
    }

    pub fn is_multi_conflict_rule(&self) -> bool {
        matches!(self, Rule::MultiConflict(_))
    }

    pub fn as_multi_conflict(&self) -> Option<&MultiConflictRule> {
        match self {
            Rule::MultiConflict(r) => Some(r),
            _ => None,
        }
    }

    /// @return self::RULE_*
    pub fn get_reason(&self) -> i64 {
        (self.bitfield() & (255 << BITFIELD_REASON)) >> BITFIELD_REASON
    }

    /// @phpstan-return ReasonData
    pub fn get_reason_data(&self) -> &ReasonData {
        self.reason_data()
    }

    pub fn get_required_package(&self) -> Option<String> {
        match self.get_reason() {
            r if r == RULE_ROOT_REQUIRE => match self.get_reason_data() {
                ReasonData::RootRequire { package_name, .. } => Some(package_name.clone()),
                _ => None,
            },
            r if r == RULE_FIXED => match self.get_reason_data() {
                ReasonData::Fixed { package } => Some(package.get_name().to_string()),
                _ => None,
            },
            r if r == RULE_PACKAGE_REQUIRES => match self.get_reason_data() {
                ReasonData::Link(link) => Some(link.get_target().to_string()),
                _ => None,
            },
            _ => None,
        }
    }

    /// @param RuleSet::TYPE_* $type
    pub fn set_type(&mut self, r#type: i64) {
        *self.bitfield_mut() =
            (self.bitfield() & !(255i64 << BITFIELD_TYPE)) | ((255 & r#type) << BITFIELD_TYPE);
    }

    pub fn get_type(&self) -> i64 {
        (self.bitfield() & (255 << BITFIELD_TYPE)) >> BITFIELD_TYPE
    }

    pub fn disable(&mut self) -> Result<()> {
        if let Rule::MultiConflict(_) = self {
            return Err(RuntimeException {
                message: "Disabling multi conflict rules is not possible. Please contact composer at https://github.com/composer/composer to let us debug what lead to this situation.".to_string(),
                code: 0,
            }
            .into());
        }
        *self.bitfield_mut() =
            (self.bitfield() & !(255i64 << BITFIELD_DISABLED)) | (1i64 << BITFIELD_DISABLED);
        Ok(())
    }

    pub fn enable(&mut self) {
        *self.bitfield_mut() &= !(255i64 << BITFIELD_DISABLED);
    }

    pub fn is_disabled(&self) -> bool {
        0 != ((self.bitfield() & (255 << BITFIELD_DISABLED)) >> BITFIELD_DISABLED)
    }

    pub fn is_enabled(&self) -> bool {
        0 == ((self.bitfield() & (255 << BITFIELD_DISABLED)) >> BITFIELD_DISABLED)
    }

    pub fn is_caused_by_lock(
        &self,
        _repository_set: &RepositorySet,
        request: &Request,
        pool: &Pool,
    ) -> bool {
        if self.get_reason() == RULE_PACKAGE_REQUIRES
            && let ReasonData::Link(link) = self.get_reason_data()
        {
            if PlatformRepository::is_platform_package(link.get_target()) {
                return false;
            }
            if let Some(locked_repo) = request.get_locked_repository() {
                let packages = locked_repo
                    .borrow_mut()
                    .get_packages()
                    .expect("LockArrayRepository::get_packages() never fails");
                for package in packages {
                    let p = package.clone();
                    if p.get_name() == link.get_target() {
                        if pool.is_unacceptable_fixed_or_locked_package(p.clone()) {
                            return true;
                        }
                        if !link.get_constraint().matches(
                            &SimpleConstraint::new(
                                "=".to_string(),
                                p.get_version().to_string(),
                                None,
                            )
                            .into(),
                        ) {
                            return true;
                        }
                        // required package was locked but has been unlocked and still matches
                        if !request.is_locked_package(p) {
                            return true;
                        }
                        break;
                    }
                }
            }
        }

        if self.get_reason() == RULE_ROOT_REQUIRE
            && let ReasonData::RootRequire {
                package_name,
                constraint,
            } = self.get_reason_data()
        {
            if PlatformRepository::is_platform_package(package_name) {
                return false;
            }
            if let Some(locked_repo) = request.get_locked_repository() {
                let packages = locked_repo
                    .borrow_mut()
                    .get_packages()
                    .expect("LockArrayRepository::get_packages() never fails");
                for package in packages {
                    let p = package.clone();
                    if p.get_name() == *package_name {
                        if pool.is_unacceptable_fixed_or_locked_package(p.clone()) {
                            return true;
                        }
                        if !constraint.matches(
                            &SimpleConstraint::new(
                                "=".to_string(),
                                p.get_version().to_string(),
                                None,
                            )
                            .into(),
                        ) {
                            return true;
                        }
                        break;
                    }
                }
            }
        }

        false
    }

    /// @internal
    pub fn get_source_package(&self, pool: &Pool) -> Result<BasePackageHandle> {
        let literals = self.get_literals();

        match self.get_reason() {
            r if r == RULE_PACKAGE_CONFLICT => {
                let mut package1 =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[0]));
                let mut package2 =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[1]));

                let reason_data = self.get_reason_data();
                // swap literals if they are not in the right order with package2 being the conflicter
                if let ReasonData::Link(link) = reason_data
                    && link.get_source() == package1.get_name()
                {
                    std::mem::swap(&mut package1, &mut package2);
                }

                Ok(package2)
            }

            r if r == RULE_PACKAGE_REQUIRES => {
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
        installed_map: &IndexMap<String, BasePackageHandle>,
        _learned_pool: &Vec<Vec<Rc<RefCell<Rule>>>>,
    ) -> anyhow::Result<String> {
        let mut literals = self.get_literals();

        Ok(match self.get_reason() {
            r if r == RULE_ROOT_REQUIRE => {
                let reason_data = self.get_reason_data();
                let (package_name, constraint): (&str, &AnyConstraint) = match reason_data {
                    ReasonData::RootRequire {
                        package_name,
                        constraint,
                    } => (package_name.as_str(), constraint),
                    _ => return Ok(String::new()),
                };

                let packages = pool.what_provides(package_name, Some(constraint));
                if packages.is_empty() {
                    return Ok(format!(
                        "No package found to satisfy root composer.json require {} {}",
                        package_name,
                        constraint.get_pretty_string(),
                    ));
                }

                let packages_non_alias: Vec<BasePackageHandle> = packages
                    .iter()
                    .filter(|p| p.as_alias().is_none())
                    .cloned()
                    .collect();
                if packages_non_alias.len() == 1 {
                    let package = &packages_non_alias[0];
                    if request.is_locked_package(package.clone()) {
                        return Ok(format!(
                            "{} is locked to version {} and an update of this package was not requested.",
                            package.get_pretty_name(),
                            package.get_pretty_version(),
                        ));
                    }
                }

                format!(
                    "Root composer.json requires {} {} -> satisfiable by {}.",
                    package_name,
                    constraint.get_pretty_string(),
                    self.format_packages_unique_from_packages(
                        pool,
                        packages.to_vec(),
                        is_verbose,
                        Some(constraint),
                        false
                    ),
                )
            }

            r if r == RULE_FIXED => {
                let package_in = match self.get_reason_data() {
                    ReasonData::Fixed { package } => package.clone(),
                    _ => return Ok(String::new()),
                };
                let package = self.deduplicate_default_branch_alias(package_in);

                if request.is_locked_package(package.clone()) {
                    return Ok(format!(
                        "{} is locked to version {} and an update of this package was not requested.",
                        package.get_pretty_name(),
                        package.get_pretty_version(),
                    ));
                }

                format!(
                    "{} is present at version {} and cannot be modified by Composer",
                    package.get_pretty_name(),
                    package.get_pretty_version(),
                )
            }

            r if r == RULE_PACKAGE_CONFLICT => {
                let mut package1 =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[0]));
                let mut package2 =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[1]));

                let mut conflict_target = package1.get_pretty_string();
                let reason_data = self.get_reason_data();

                let link = match reason_data {
                    ReasonData::Link(l) => l,
                    _ => return Ok(String::new()),
                };
                // swap literals if they are not in the right order with package2 being the conflicter
                if link.get_source() == package1.get_name() {
                    std::mem::swap(&mut package1, &mut package2);
                    conflict_target = format!(
                        "{} {}",
                        package1.get_pretty_name(),
                        link.get_pretty_constraint(),
                    );
                }

                // if the conflict is not directly against the package but something it provides/replaces,
                // we try to find that link to display a better message
                if link.get_target() != package1.get_name() {
                    let mut provide_type: Option<&str> = None;
                    let mut provided: Option<String> = None;
                    for provide in package1.get_provides().values() {
                        if provide.get_target() == link.get_target() {
                            provide_type = Some("provides");
                            provided = Some(provide.get_pretty_constraint().to_string());
                            break;
                        }
                    }
                    for replace in package1.get_replaces().values() {
                        if replace.get_target() == link.get_target() {
                            provide_type = Some("replaces");
                            provided = Some(replace.get_pretty_constraint().to_string());
                            break;
                        }
                    }
                    if let Some(pt) = provide_type {
                        conflict_target = format!(
                            "{} {} ({} {} {} {})",
                            link.get_target(),
                            link.get_pretty_constraint(),
                            package1.get_pretty_string(),
                            pt,
                            link.get_target(),
                            provided.unwrap_or_default(),
                        );
                    }
                }

                format!(
                    "{} conflicts with {}.",
                    package2.get_pretty_string(),
                    conflict_target
                )
            }

            r if r == RULE_PACKAGE_REQUIRES => {
                assert!(!literals.is_empty());
                let source_literal = array_shift(&mut literals).unwrap();
                let source_package =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(source_literal));
                let reason_data = self.get_reason_data();
                let link = match reason_data {
                    ReasonData::Link(l) => l,
                    _ => return Ok(String::new()),
                };

                let mut requires: Vec<BasePackageHandle> = vec![];
                for literal in &literals {
                    requires.push(pool.literal_to_package(*literal));
                }

                let text = link.get_pretty_string(source_package.clone());
                if !requires.is_empty() {
                    format!(
                        "{} -> satisfiable by {}.",
                        text,
                        self.format_packages_unique_from_packages(
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
                    )?;

                    return Ok(format!("{} -> {}", text, reason.1));
                }
            }

            r if r == RULE_PACKAGE_SAME_NAME => {
                let mut package_names: IndexMap<String, bool> = IndexMap::new();
                for literal in &literals {
                    let package = pool.literal_to_package(*literal);
                    package_names.insert(package.get_name().to_string(), true);
                }
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

                    let mut installed_packages: Vec<BasePackageHandle> = vec![];
                    let mut removable_packages: Vec<BasePackageHandle> = vec![];
                    for literal in &literals {
                        if installed_map.contains_key(&literal.abs().to_string()) {
                            installed_packages.push(pool.literal_to_package(*literal));
                        } else {
                            removable_packages.push(pool.literal_to_package(*literal));
                        }
                    }

                    if !installed_packages.is_empty() && !removable_packages.is_empty() {
                        return Ok(format!(
                            "{} cannot be installed as that would require removing {}. {}",
                            self.format_packages_unique_from_packages(
                                pool,
                                removable_packages,
                                is_verbose,
                                None,
                                true,
                            ),
                            self.format_packages_unique_from_packages(
                                pool,
                                installed_packages,
                                is_verbose,
                                None,
                                true,
                            ),
                            reason,
                        ));
                    }

                    return Ok(format!(
                        "Only one of these can be installed: {}. {}",
                        self.format_packages_unique_from_literals(
                            pool, &literals, is_verbose, None, true
                        ),
                        reason,
                    ));
                }

                format!(
                    "You can only install one version of a package, so only one of these can be installed: {}.",
                    self.format_packages_unique_from_literals(
                        pool, &literals, is_verbose, None, true
                    ),
                )
            }
            r if r == RULE_LEARNED => {
                // @TODO currently still generates way too much output to be helpful, and in some cases can even lead to endless recursion
                // (PHP commented-out alternative code preserved)
                let learned_string = " (conflict analysis result)";

                let rule_text = if literals.len() == 1 {
                    pool.literal_to_pretty_string(literals[0], installed_map)
                } else {
                    let mut groups: IndexMap<String, Vec<BasePackageHandle>> = IndexMap::new();
                    for literal in &literals {
                        let package = pool.literal_to_package(*literal);
                        let group = if installed_map.contains_key(&package.id().to_string()) {
                            if *literal > 0 { "keep" } else { "remove" }
                        } else {
                            if *literal > 0 {
                                "install"
                            } else {
                                "don't install"
                            }
                        };

                        groups
                            .entry(group.to_string())
                            .or_default()
                            .push(self.deduplicate_default_branch_alias(package.clone()));
                    }
                    let mut rule_texts: Vec<String> = vec![];
                    for (group, packages) in &groups {
                        rule_texts.push(format!(
                            "{}{} {}",
                            group,
                            if packages.len() > 1 { " one of" } else { "" },
                            self.format_packages_unique_from_packages(
                                pool,
                                packages.to_vec(),
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
            r if r == RULE_PACKAGE_ALIAS => {
                let alias_package = pool.literal_to_package(literals[0]);

                // avoid returning content like "9999999-dev is an alias of dev-master" as it is useless
                if alias_package.get_version() == VersionParser::DEFAULT_BRANCH_ALIAS {
                    return Ok(String::new());
                }
                let package =
                    self.deduplicate_default_branch_alias(pool.literal_to_package(literals[1]));

                format!(
                    "{} is an alias of {} and thus requires it to be installed too.",
                    alias_package.get_pretty_string(),
                    package.get_pretty_string(),
                )
            }
            r if r == RULE_PACKAGE_INVERSE_ALIAS => {
                // inverse alias rules work the other way around than above
                let alias_package = pool.literal_to_package(literals[1]);

                // avoid returning content like "9999999-dev is an alias of dev-master" as it is useless
                if alias_package.get_version() == VersionParser::DEFAULT_BRANCH_ALIAS {
                    return Ok(String::new());
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
                    rule_text.push_str(&pool.literal_to_pretty_string(*literal, installed_map));
                }

                format!("({})", rule_text)
            }
        })
    }

    // Corresponds the variant formatPackagesUnique() that takes an array of BasePackages.
    fn format_packages_unique_from_packages(
        &self,
        pool: &Pool,
        packages: Vec<BasePackageHandle>,
        is_verbose: bool,
        constraint: Option<&AnyConstraint>,
        use_removed_version_group: bool,
    ) -> String {
        Problem::get_package_list(
            &packages,
            is_verbose,
            Some(pool),
            constraint,
            use_removed_version_group,
        )
    }

    // Corresponds the variant formatPackagesUnique() that takes an array of integers.
    fn format_packages_unique_from_literals(
        &self,
        pool: &Pool,
        literals: &[i64],
        is_verbose: bool,
        constraint: Option<&AnyConstraint>,
        use_removed_version_group: bool,
    ) -> String {
        let mut packages: Vec<BasePackageHandle> = vec![];
        for literal in literals {
            packages.push(pool.literal_to_package(*literal));
        }
        Problem::get_package_list(
            &packages,
            is_verbose,
            Some(pool),
            constraint,
            use_removed_version_group,
        )
    }

    fn deduplicate_default_branch_alias(&self, package: BasePackageHandle) -> BasePackageHandle {
        if let Some(alias_pkg) = package.as_alias()
            && alias_pkg.get_pretty_version() == VersionParser::DEFAULT_BRANCH_ALIAS
        {
            return alias_pkg.get_alias_of().into();
        }

        package
    }
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rule::Generic(r) => write!(f, "{}", r),
            Rule::MultiConflict(r) => write!(f, "{}", r),
            Rule::TwoLiterals(r) => write!(f, "{}", r),
        }
    }
}

#[derive(Debug)]
pub struct RuleBase {
    pub(crate) bitfield: i64,
    pub(crate) request: Option<Request>,
    pub(crate) reason_data: ReasonData,
}

impl RuleBase {
    pub const BITFIELD_DISABLED: i64 = BITFIELD_DISABLED;
    pub const BITFIELD_REASON: i64 = BITFIELD_REASON;
    pub const BITFIELD_TYPE: i64 = BITFIELD_TYPE;

    pub fn new(reason: i64, reason_data: ReasonData) -> Self {
        let bitfield =
            (0i64 << BITFIELD_DISABLED) | (reason << BITFIELD_REASON) | (255i64 << BITFIELD_TYPE);
        Self {
            bitfield,
            request: None,
            reason_data,
        }
    }

    pub fn is_disabled(&self) -> bool {
        0 != ((self.bitfield & (255 << BITFIELD_DISABLED)) >> BITFIELD_DISABLED)
    }
}
