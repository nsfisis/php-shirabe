//! ref: composer/src/Composer/DependencyResolver/RuleSetGenerator.php

use std::any::Any;
use std::collections::VecDeque;

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::dependency_resolver::generic_rule::GenericRule;
use crate::dependency_resolver::multi_conflict_rule::MultiConflictRule;
use crate::dependency_resolver::policy_interface::PolicyInterface;
use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::request::Request;
use crate::dependency_resolver::rule::Rule;
use crate::dependency_resolver::rule_set::RuleSet;
use crate::dependency_resolver::rule2_literals::Rule2Literals;
use crate::filter::platform_requirement_filter::ignore_list_platform_requirement_filter::IgnoreListPlatformRequirementFilter;
use crate::filter::platform_requirement_filter::platform_requirement_filter_factory::PlatformRequirementFilterFactory;
use crate::filter::platform_requirement_filter::platform_requirement_filter_interface::PlatformRequirementFilterInterface;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::BasePackage;
use crate::package::package_interface::PackageInterface;

#[derive(Debug)]
pub struct RuleSetGenerator {
    pub(crate) policy: Box<dyn PolicyInterface>,
    pub(crate) pool: Pool,
    pub(crate) rules: RuleSet,
    pub(crate) added_map: IndexMap<i64, Box<dyn PackageInterface>>,
    pub(crate) added_packages_by_names: IndexMap<String, Vec<Box<dyn PackageInterface>>>,
}

impl RuleSetGenerator {
    pub fn new(policy: Box<dyn PolicyInterface>, pool: Pool) -> Self {
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
            // Rule2Literals and MultiConflictRule both implement Rule (Phase B: define Rule type)
            Rule::from(Rule2Literals::new(
                literals[0],
                literals[1],
                PhpMixed::Int(reason),
                reason_data,
            ))
        } else {
            Rule::from(
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
            self.rules.add(rule, r#type).ok();
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

            self.added_map.insert(package.get_id(), package.clone_box());

            let is_alias = (package.as_any() as &dyn Any)
                .downcast_ref::<AliasPackage>()
                .is_some();

            if !is_alias {
                for name in package.get_names(false) {
                    self.added_packages_by_names
                        .entry(name)
                        .or_default()
                        .push(package.clone_box());
                }
            } else {
                let alias_pkg = (package.as_any() as &dyn Any)
                    .downcast_ref::<AliasPackage>()
                    .unwrap();

                work_queue.push_back(alias_pkg.get_alias_of().clone_box());
                let alias_of = alias_pkg.get_alias_of();
                let rule = self.create_require_rule(
                    &*package,
                    &[alias_of.clone_box()],
                    Rule::RULE_PACKAGE_ALIAS,
                    PhpMixed::Null, // reasonData: $package (BasePackage)
                );
                self.add_rule(RuleSet::TYPE_PACKAGE, rule.map(Rule::from));

                // aliases must be installed with their main package, so create a rule the other way around as well
                let inverse_rule = self.create_require_rule(
                    alias_of,
                    &[package.clone_box()],
                    Rule::RULE_PACKAGE_INVERSE_ALIAS,
                    PhpMixed::Null, // reasonData: $package->getAliasOf() (BasePackage)
                );
                self.add_rule(RuleSet::TYPE_PACKAGE, inverse_rule.map(Rule::from));

                // if alias package has no self.version requires, its requirements do not
                // need to be added as the aliased package processing will take care of it
                if !alias_pkg.has_self_version_requires() {
                    continue;
                }
            }

            for link in package.get_requires().values() {
                let mut constraint = link.get_constraint().clone_box();
                if platform_requirement_filter.is_ignored(link.get_target()) {
                    continue;
                } else if let Some(ignore_list_filter) = (platform_requirement_filter as &dyn Any)
                    .downcast_ref::<IgnoreListPlatformRequirementFilter>(
                ) {
                    constraint = ignore_list_filter
                        .filter_constraint(link.get_target(), constraint, true)
                        .unwrap_or(constraint);
                }

                let possible_requires = self.pool.what_provides(link.get_target(), &*constraint);

                let rule = self.create_require_rule(
                    &*package,
                    &possible_requires,
                    Rule::RULE_PACKAGE_REQUIRES,
                    PhpMixed::Null, // reasonData: $link (Link)
                );
                self.add_rule(RuleSet::TYPE_PACKAGE, rule.map(Rule::from));

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
        let packages: Vec<Box<dyn PackageInterface>> =
            self.added_map.values().map(|p| p.clone_box()).collect();

        for package in &packages {
            for link in package.get_conflicts().values() {
                // even if conflict ends up being with an alias, there would be at least one actual package by this name
                if !self.added_packages_by_names.contains_key(link.get_target()) {
                    continue;
                }

                let mut constraint = link.get_constraint().clone_box();
                if platform_requirement_filter.is_ignored(link.get_target()) {
                    continue;
                } else if let Some(ignore_list_filter) = (platform_requirement_filter as &dyn Any)
                    .downcast_ref::<IgnoreListPlatformRequirementFilter>(
                ) {
                    constraint = ignore_list_filter
                        .filter_constraint(link.get_target(), constraint, false)
                        .unwrap_or(constraint);
                }

                let conflicts = self.pool.what_provides(link.get_target(), &*constraint);

                for conflict in &conflicts {
                    // define the conflict rule for regular packages, for alias packages it's only needed if the name
                    // matches the conflict exactly, otherwise the name match is by provide/replace which means the
                    // package which this is an alias of will conflict anyway, so no need to create additional rules
                    let conflict_is_alias = (conflict.as_any() as &dyn Any)
                        .downcast_ref::<AliasPackage>()
                        .is_some();
                    let conflict_name_matches = conflict.get_name() == link.get_target();
                    if !conflict_is_alias || conflict_name_matches {
                        let rule = self.create_rule2_literals(
                            &**package,
                            &**conflict,
                            Rule::RULE_PACKAGE_CONFLICT,
                            PhpMixed::Null, // reasonData: $link (Link)
                        );
                        self.add_rule(RuleSet::TYPE_PACKAGE, rule.map(Rule::from));
                    }
                }
            }
        }

        let names_packages: Vec<(String, Vec<Box<dyn PackageInterface>>)> = self
            .added_packages_by_names
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().map(|p| p.clone_box()).collect()))
            .collect();

        for (name, packages) in names_packages {
            if packages.len() > 1 {
                let reason = Rule::RULE_PACKAGE_SAME_NAME;
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
                if self.pool.is_unacceptable_fixed_or_locked_package(package) {
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
                &[package.clone_box()],
                Rule::RULE_FIXED,
                PhpMixed::Array(reason_data),
            );
            self.add_rule(RuleSet::TYPE_REQUEST, Some(Rule::from(rule)));
        }

        for (package_name, constraint) in request.get_requires() {
            let mut constraint = constraint.clone_box();
            if platform_requirement_filter.is_ignored(package_name) {
                continue;
            } else if let Some(ignore_list_filter) = (platform_requirement_filter as &dyn Any)
                .downcast_ref::<IgnoreListPlatformRequirementFilter>(
            ) {
                constraint = ignore_list_filter
                    .filter_constraint(package_name, constraint, true)
                    .unwrap_or(constraint);
            }

            let packages = self.pool.what_provides(package_name, &*constraint);
            if !packages.is_empty() {
                for package in &packages {
                    self.add_rules_for_package(package.clone_box(), platform_requirement_filter);
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
                    Rule::RULE_ROOT_REQUIRE,
                    PhpMixed::Array(reason_data),
                );
                self.add_rule(RuleSet::TYPE_REQUEST, Some(Rule::from(rule)));
            }
        }

        Ok(())
    }

    pub(crate) fn add_rules_for_root_aliases(
        &mut self,
        platform_requirement_filter: &dyn PlatformRequirementFilterInterface,
    ) {
        for package in self.pool.get_packages() {
            // ensure that rules for root alias packages and aliases of packages which were loaded are also loaded
            // even if the alias itself isn't required, otherwise a package could be installed without its alias which
            // leads to unexpected behavior
            let is_not_added = !self.added_map.contains_key(&package.get_id());
            let as_alias = (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>();
            if is_not_added {
                if let Some(alias_pkg) = as_alias {
                    if alias_pkg.is_root_package_alias()
                        || self
                            .added_map
                            .contains_key(&alias_pkg.get_alias_of().get_id())
                    {
                        self.add_rules_for_package(
                            package.clone_box(),
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
