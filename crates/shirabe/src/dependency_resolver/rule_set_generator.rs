//! ref: composer/src/Composer/DependencyResolver/RuleSetGenerator.php

use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::dependency_resolver::GenericRule;
use crate::dependency_resolver::MultiConflictRule;
use crate::dependency_resolver::PolicyInterface;
use crate::dependency_resolver::Pool;
use crate::dependency_resolver::Request;
use crate::dependency_resolver::Rule2Literals;
use crate::dependency_resolver::RuleSet;
use crate::dependency_resolver::rule::{self, Rule};
use crate::filter::platform_requirement_filter::IgnoreListPlatformRequirementFilter;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::PlatformRequirementFilterInterface;
use crate::package::AliasPackage;
use crate::package::BasePackage;
use crate::package::PackageInterface;

#[derive(Debug)]
pub struct RuleSetGenerator {
    pub(crate) policy: Box<dyn PolicyInterface>,
    pub(crate) pool: std::rc::Rc<std::cell::RefCell<Pool>>,
    pub(crate) rules: RuleSet,
    pub(crate) added_map: IndexMap<i64, Box<dyn PackageInterface>>,
    pub(crate) added_packages_by_names: IndexMap<String, Vec<Box<dyn PackageInterface>>>,
}

impl RuleSetGenerator {
    pub fn new(
        policy: Box<dyn PolicyInterface>,
        pool: std::rc::Rc<std::cell::RefCell<Pool>>,
    ) -> Self {
        Self {
            policy,
            pool,
            rules: RuleSet::new(),
            added_map: IndexMap::new(),
            added_packages_by_names: IndexMap::new(),
        }
    }

    /// Creates a new rule for the requirements of a package.
    ///
    /// This rule is of the form (-A|B|C), where B and C are the providers of
    /// one requirement of the package A.
    fn create_require_rule(
        &self,
        package: &dyn PackageInterface,
        providers: &[Box<dyn PackageInterface>],
        reason: i64,
        reason_data: PhpMixed,
    ) -> Option<GenericRule> {
        let mut literals = vec![-package.get_id()];

        for provider in providers {
            // self fulfilling rule?
            if std::ptr::eq(
                provider.as_ref() as *const dyn PackageInterface,
                package as *const dyn PackageInterface,
            ) {
                return None;
            }
            literals.push(provider.get_id());
        }

        Some(GenericRule::new(
            literals,
            PhpMixed::Int(reason),
            reason_data,
        ))
    }

    /// Creates a rule to install at least one of a set of packages.
    ///
    /// The rule is (A|B|C) with A, B and C different packages. If the given
    /// set of packages is empty an impossible rule is generated.
    fn create_install_one_of_rule(
        &self,
        packages: &[Box<dyn PackageInterface>],
        reason: i64,
        reason_data: PhpMixed,
    ) -> GenericRule {
        let literals: Vec<i64> = packages.iter().map(|p| p.get_id()).collect();
        GenericRule::new(literals, PhpMixed::Int(reason), reason_data)
    }

    /// Creates a rule for two conflicting packages.
    ///
    /// The rule for conflicting packages A and B is (-A|-B). A is called the issuer
    /// and B the provider.
    fn create_rule2_literals(
        &self,
        issuer: &dyn PackageInterface,
        provider: &dyn PackageInterface,
        reason: i64,
        reason_data: PhpMixed,
    ) -> Option<Rule2Literals> {
        // ignore self conflict
        if std::ptr::eq(
            issuer as *const dyn PackageInterface,
            provider as *const dyn PackageInterface,
        ) {
            return None;
        }

        Some(Rule2Literals::new(
            -issuer.get_id(),
            -provider.get_id(),
            PhpMixed::Int(reason),
            reason_data,
        ))
    }

    fn create_multi_conflict_rule(
        &self,
        packages: &[Box<dyn PackageInterface>],
        reason: i64,
        reason_data: PhpMixed,
    ) -> Rule {
        let literals: Vec<i64> = packages.iter().map(|p| -p.get_id()).collect();

        if literals.len() == 2 {
            Rule::TwoLiterals(Rule2Literals::new(
                literals[0],
                literals[1],
                PhpMixed::Int(reason),
                reason_data,
            ))
        } else {
            Rule::MultiConflict(
                MultiConflictRule::new(literals, PhpMixed::Int(reason), reason_data).unwrap(),
            )
        }
    }

    /// Adds a rule unless it duplicates an existing one of any type.
    ///
    /// To be able to directly pass in the result of one of the rule creation
    /// methods null is allowed which will not insert a rule.
    fn add_rule(&mut self, r#type: i64, new_rule: Option<Rule>) {
        if let Some(rule) = new_rule {
            self.rules.add(Rc::new(RefCell::new(rule)), r#type).ok();
        }
    }

    pub(crate) fn add_rules_for_package(
        &mut self,
        package: Box<dyn PackageInterface>,
        platform_requirement_filter: &dyn PlatformRequirementFilterInterface,
    ) {
        let mut work_queue: VecDeque<Box<dyn PackageInterface>> = VecDeque::new();
        work_queue.push_back(package);

        while let Some(package) = work_queue.pop_front() {
            if self.added_map.contains_key(&package.get_id()) {
                continue;
            }

            self.added_map
                .insert(package.get_id(), package.clone_package_box());

            let is_alias = package.as_any().downcast_ref::<AliasPackage>().is_some();

            if !is_alias {
                for name in package.get_names(false) {
                    self.added_packages_by_names
                        .entry(name)
                        .or_default()
                        .push(package.clone_package_box());
                }
            } else {
                let alias_pkg = package.as_any().downcast_ref::<AliasPackage>().unwrap();

                work_queue.push_back(alias_pkg.get_alias_of().clone_package_box());
                let alias_of = alias_pkg.get_alias_of();
                let rule = self.create_require_rule(
                    &*package,
                    &[alias_of.clone_package_box()],
                    rule::RULE_PACKAGE_ALIAS,
                    PhpMixed::Null, // reasonData: $package (BasePackage)
                );
                self.add_rule(RuleSet::TYPE_PACKAGE, rule.map(Rule::Generic));

                // aliases must be installed with their main package, so create a rule the other way around as well
                let inverse_rule = self.create_require_rule(
                    alias_of,
                    &[package.clone_package_box()],
                    rule::RULE_PACKAGE_INVERSE_ALIAS,
                    PhpMixed::Null, // reasonData: $package->getAliasOf() (BasePackage)
                );
                self.add_rule(RuleSet::TYPE_PACKAGE, inverse_rule.map(Rule::Generic));

                // if alias package has no self.version requires, its requirements do not
                // need to be added as the aliased package processing will take care of it
                if !alias_pkg.has_self_version_requires() {
                    continue;
                }
            }

            for link in package.get_requires().values() {
                let mut constraint = link.get_constraint().clone();
                if platform_requirement_filter.is_ignored(link.get_target()) {
                    continue;
                } else if let Some(ignore_list_filter) = platform_requirement_filter
                    .as_any()
                    .downcast_ref::<IgnoreListPlatformRequirementFilter>(
                ) {
                    let fallback = constraint.clone();
                    constraint = ignore_list_filter
                        .filter_constraint(link.get_target(), constraint, true)
                        .unwrap_or(fallback);
                }

                let possible_requires: Vec<Box<dyn PackageInterface>> = self
                    .pool
                    .borrow_mut()
                    .what_provides(link.get_target(), Some(&constraint))
                    .into_iter()
                    .map(|p| p.clone_package_box())
                    .collect();

                let rule = self.create_require_rule(
                    &*package,
                    &possible_requires,
                    rule::RULE_PACKAGE_REQUIRES,
                    PhpMixed::Null, // reasonData: $link (Link)
                );
                self.add_rule(RuleSet::TYPE_PACKAGE, rule.map(Rule::Generic));

                for require in possible_requires {
                    work_queue.push_back(require);
                }
            }
        }
    }

    pub(crate) fn add_conflict_rules(
        &mut self,
        platform_requirement_filter: &dyn PlatformRequirementFilterInterface,
    ) {
        let packages: Vec<Box<dyn PackageInterface>> = self
            .added_map
            .values()
            .map(|p| p.clone_package_box())
            .collect();

        for package in &packages {
            for link in package.get_conflicts().values() {
                // even if conflict ends up being with an alias, there would be at least one actual package by this name
                if !self.added_packages_by_names.contains_key(link.get_target()) {
                    continue;
                }

                let mut constraint = link.get_constraint().clone();
                if platform_requirement_filter.is_ignored(link.get_target()) {
                    continue;
                } else if let Some(ignore_list_filter) = platform_requirement_filter
                    .as_any()
                    .downcast_ref::<IgnoreListPlatformRequirementFilter>(
                ) {
                    let fallback = constraint.clone();
                    constraint = ignore_list_filter
                        .filter_constraint(link.get_target(), constraint, false)
                        .unwrap_or(fallback);
                }

                let conflicts = self
                    .pool
                    .borrow_mut()
                    .what_provides(link.get_target(), Some(&constraint));

                for conflict in &conflicts {
                    // define the conflict rule for regular packages, for alias packages it's only needed if the name
                    // matches the conflict exactly, otherwise the name match is by provide/replace which means the
                    // package which this is an alias of will conflict anyway, so no need to create additional rules
                    let conflict_is_alias =
                        conflict.as_any().downcast_ref::<AliasPackage>().is_some();
                    let conflict_name_matches = conflict.get_name() == link.get_target();
                    if !conflict_is_alias || conflict_name_matches {
                        let rule = self.create_rule2_literals(
                            &**package,
                            &**conflict,
                            rule::RULE_PACKAGE_CONFLICT,
                            PhpMixed::Null, // reasonData: $link (Link)
                        );
                        self.add_rule(RuleSet::TYPE_PACKAGE, rule.map(Rule::TwoLiterals));
                    }
                }
            }
        }

        let names_packages: Vec<(String, Vec<Box<dyn PackageInterface>>)> = self
            .added_packages_by_names
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().map(|p| p.clone_package_box()).collect()))
            .collect();

        for (name, packages) in names_packages {
            if packages.len() > 1 {
                let reason = rule::RULE_PACKAGE_SAME_NAME;
                let rule =
                    self.create_multi_conflict_rule(&packages, reason, PhpMixed::String(name));
                self.add_rule(RuleSet::TYPE_PACKAGE, Some(rule));
            }
        }
    }

    pub(crate) fn add_rules_for_request(
        &mut self,
        request: &Request,
        platform_requirement_filter: &dyn PlatformRequirementFilterInterface,
    ) -> anyhow::Result<()> {
        for package in request.get_fixed_packages().values() {
            if package.get_id() == -1 {
                // fixed package was not added to the pool as it did not pass the stability requirements, this is fine
                if self
                    .pool
                    .borrow()
                    .is_unacceptable_fixed_or_locked_package(package.as_ref())
                {
                    continue;
                }

                // otherwise, looks like a bug
                return Err(anyhow::anyhow!(shirabe_php_shim::LogicException {
                    message: format!(
                        "Fixed package {} was not added to solver pool.",
                        package.get_pretty_string()
                    ),
                    code: 0,
                }));
            }

            self.add_rules_for_package(package.clone_box(), platform_requirement_filter);

            let mut reason_data: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            reason_data.insert(
                "package".to_string(),
                Box::new(PhpMixed::Null), // reasonData: $package (BasePackage)
            );
            let rule = self.create_install_one_of_rule(
                &[package.clone_package_box()],
                rule::RULE_FIXED,
                PhpMixed::Array(reason_data),
            );
            self.add_rule(RuleSet::TYPE_REQUEST, Some(Rule::Generic(rule)));
        }

        for (package_name, constraint) in request.get_requires() {
            let mut constraint = constraint.clone();
            if platform_requirement_filter.is_ignored(package_name) {
                continue;
            } else if let Some(ignore_list_filter) = platform_requirement_filter
                .as_any()
                .downcast_ref::<IgnoreListPlatformRequirementFilter>(
            ) {
                let fallback = constraint.clone();
                constraint = ignore_list_filter
                    .filter_constraint(package_name, constraint, true)
                    .unwrap_or(fallback);
            }

            let packages: Vec<Box<dyn PackageInterface>> = self
                .pool
                .borrow_mut()
                .what_provides(package_name, Some(&constraint))
                .into_iter()
                .map(|p| p.clone_package_box())
                .collect();
            if !packages.is_empty() {
                for package in &packages {
                    self.add_rules_for_package(
                        package.clone_package_box(),
                        platform_requirement_filter,
                    );
                }

                let mut reason_data: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                reason_data.insert(
                    "packageName".to_string(),
                    Box::new(PhpMixed::String(package_name.clone())),
                );
                reason_data.insert(
                    "constraint".to_string(),
                    Box::new(PhpMixed::Null), // reasonData: $constraint (ConstraintInterface)
                );
                let rule = self.create_install_one_of_rule(
                    &packages,
                    rule::RULE_ROOT_REQUIRE,
                    PhpMixed::Array(reason_data),
                );
                self.add_rule(RuleSet::TYPE_REQUEST, Some(Rule::Generic(rule)));
            }
        }

        Ok(())
    }

    pub(crate) fn add_rules_for_root_aliases(
        &mut self,
        platform_requirement_filter: &dyn PlatformRequirementFilterInterface,
    ) {
        let packages: Vec<Box<dyn BasePackage>> = self
            .pool
            .borrow()
            .get_packages()
            .iter()
            .map(|p| p.clone_box())
            .collect();
        for package in &packages {
            // ensure that rules for root alias packages and aliases of packages which were loaded are also loaded
            // even if the alias itself isn't required, otherwise a package could be installed without its alias which
            // leads to unexpected behavior
            let is_not_added = !self.added_map.contains_key(&package.get_id());
            let as_alias = package.as_any().downcast_ref::<AliasPackage>();
            if is_not_added {
                if let Some(alias_pkg) = as_alias {
                    if alias_pkg.is_root_package_alias()
                        || self
                            .added_map
                            .contains_key(&alias_pkg.get_alias_of().get_id())
                    {
                        self.add_rules_for_package(
                            package.clone_package_box(),
                            platform_requirement_filter,
                        );
                    }
                }
            }
        }
    }

    pub fn get_rules_for(
        &mut self,
        request: &Request,
        platform_requirement_filter: Option<Box<dyn PlatformRequirementFilterInterface>>,
    ) -> anyhow::Result<RuleSet> {
        let platform_requirement_filter = platform_requirement_filter
            .unwrap_or_else(PlatformRequirementFilterFactory::ignore_nothing);

        self.add_rules_for_request(request, &*platform_requirement_filter)?;

        self.add_rules_for_root_aliases(&*platform_requirement_filter);

        self.add_conflict_rules(&*platform_requirement_filter);

        // Remove references to packages
        self.added_map = IndexMap::new();
        self.added_packages_by_names = IndexMap::new();

        let rules = std::mem::replace(&mut self.rules, RuleSet::new());

        Ok(rules)
    }
}
