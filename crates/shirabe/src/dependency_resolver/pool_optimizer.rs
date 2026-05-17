//! ref: composer/src/Composer/DependencyResolver/PoolOptimizer.php

use std::any::Any;

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{LogicException, PhpMixed, implode, ksort, spl_object_hash};
use shirabe_semver::compiling_matcher::CompilingMatcher;
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;
use shirabe_semver::constraint::multi_constraint::MultiConstraint;
use shirabe_semver::intervals::Intervals;

use crate::dependency_resolver::policy_interface::PolicyInterface;
use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::request::Request;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::BasePackage;
use crate::package::version::version_parser::VersionParser;

/// Optimizes a given pool
#[derive(Debug)]
pub struct PoolOptimizer {
    /// @var PolicyInterface
    policy: Box<dyn PolicyInterface>,

    /// @var array<int, true>
    irremovable_packages: IndexMap<i64, bool>,

    /// @var array<string, array<string, ConstraintInterface>>
    require_constraints_per_package:
        IndexMap<String, IndexMap<String, Box<dyn ConstraintInterface>>>,

    /// @var array<string, array<string, ConstraintInterface>>
    conflict_constraints_per_package:
        IndexMap<String, IndexMap<String, Box<dyn ConstraintInterface>>>,

    /// @var array<int, true>
    packages_to_remove: IndexMap<i64, bool>,

    /// @var array<int, BasePackage[]>
    aliases_per_package: IndexMap<i64, Vec<Box<dyn BasePackage>>>,

    /// @var array<string, array<string, string>>
    removed_versions_by_package: IndexMap<String, IndexMap<String, String>>,
}

#[derive(Debug, Clone)]
struct IdenticalDefinitionPointers {
    group_hash: String,
    dependency_hash: String,
}

impl PoolOptimizer {
    pub fn new(policy: Box<dyn PolicyInterface>) -> Self {
        Self {
            policy,
            irremovable_packages: IndexMap::new(),
            require_constraints_per_package: IndexMap::new(),
            conflict_constraints_per_package: IndexMap::new(),
            packages_to_remove: IndexMap::new(),
            aliases_per_package: IndexMap::new(),
            removed_versions_by_package: IndexMap::new(),
        }
    }

    pub fn optimize(&mut self, request: &Request, pool: &Pool) -> Result<Pool> {
        self.prepare(request, pool);

        self.optimize_by_identical_dependencies(request, pool)?;

        self.optimize_impossible_packages_away(request, pool);

        let optimized_pool = self.apply_removals_to_pool(pool);

        // No need to run this recursively at the moment
        // because the current optimizations cannot provide
        // even more gains when ran again. Might change
        // in the future with additional optimizations.

        self.irremovable_packages = IndexMap::new();
        self.require_constraints_per_package = IndexMap::new();
        self.conflict_constraints_per_package = IndexMap::new();
        self.packages_to_remove = IndexMap::new();
        self.aliases_per_package = IndexMap::new();
        self.removed_versions_by_package = IndexMap::new();

        Ok(optimized_pool)
    }

    fn prepare(&mut self, request: &Request, pool: &Pool) {
        let mut irremovable_package_constraint_groups: IndexMap<
            String,
            Vec<Box<dyn ConstraintInterface>>,
        > = IndexMap::new();

        // Mark fixed or locked packages as irremovable
        for (_, package) in request.get_fixed_or_locked_packages() {
            irremovable_package_constraint_groups
                .entry(package.get_name().to_string())
                .or_insert_with(Vec::new)
                .push(Box::new(Constraint::new("==", package.get_version())));
        }

        // Extract requested package requirements
        for (require, constraint) in request.get_requires() {
            // TODO(phase-b): clone Box<dyn ConstraintInterface>
            self.extract_require_constraints_per_package(require, todo!("constraint.clone_box()"));
        }

        // First pass over all packages to extract information and mark package constraints irremovable
        for package in pool.get_packages() {
            // Extract package requirements
            for link in package.get_requires().values() {
                self.extract_require_constraints_per_package(
                    link.get_target(),
                    // TODO(phase-b): clone constraint
                    todo!("link.get_constraint().clone_box()"),
                );
            }
            // Extract package conflicts
            for link in package.get_conflicts() {
                self.extract_conflict_constraints_per_package(
                    link.get_target(),
                    // TODO(phase-b): clone constraint
                    todo!("link.get_constraint().clone_box()"),
                );
            }

            // Keep track of alias packages for every package so if either the alias or aliased is kept
            // we keep the others as they are a unit of packages really
            if let Some(alias_pkg) = (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>() {
                self.aliases_per_package
                    .entry(alias_pkg.get_alias_of().id)
                    .or_insert_with(Vec::new)
                    .push(package.clone_box());
            }
        }

        let mut irremovable_package_constraints: IndexMap<String, Box<dyn ConstraintInterface>> =
            IndexMap::new();
        for (package_name, constraints) in irremovable_package_constraint_groups {
            // TODO(phase-b): MultiConstraint::new signature; move ownership of constraints vec
            irremovable_package_constraints.insert(
                package_name,
                if 1 == constraints.len() {
                    todo!("constraints[0] moved out")
                } else {
                    Box::new(MultiConstraint::new(constraints, false))
                },
            );
        }
        // PHP: unset($irremovablePackageConstraintGroups);

        // Mark the packages as irremovable based on the constraints
        for package in pool.get_packages() {
            if !irremovable_package_constraints.contains_key(package.get_name()) {
                continue;
            }

            let constraint = irremovable_package_constraints
                .get(package.get_name())
                .unwrap();
            if CompilingMatcher::r#match(
                constraint.as_ref(),
                Constraint::OP_EQ,
                package.get_version(),
            ) {
                self.mark_package_irremovable(package);
            }
        }
    }

    fn mark_package_irremovable(&mut self, package: &dyn BasePackage) {
        self.irremovable_packages.insert(package.id, true);
        if let Some(alias_pkg) = (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>() {
            // recursing here so aliasesPerPackage for the aliasOf can be checked
            // and all its aliases marked as irremovable as well
            self.mark_package_irremovable(alias_pkg.get_alias_of());
        }
        // PHP: foreach ($this->aliasesPerPackage[$package->id] as $aliasPackage)
        let aliases = self
            .aliases_per_package
            .get(&package.id)
            .cloned()
            .unwrap_or_default();
        for alias_package in aliases {
            self.irremovable_packages.insert(alias_package.id, true);
        }
    }

    /// @return Pool Optimized pool
    fn apply_removals_to_pool(&self, pool: &Pool) -> Pool {
        let mut packages: Vec<Box<dyn BasePackage>> = vec![];
        let mut removed_versions: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
        for package in pool.get_packages() {
            if !self.packages_to_remove.contains_key(&package.id) {
                packages.push(package.clone_box());
            } else {
                removed_versions
                    .entry(package.get_name().to_string())
                    .or_insert_with(IndexMap::new)
                    .insert(
                        package.get_version().to_string(),
                        package.get_pretty_version().to_string(),
                    );
            }
        }

        Pool::new(
            packages,
            // TODO(phase-b): clone Vec<Box<BasePackage>> from getter
            todo!("pool.get_unacceptable_fixed_or_locked_packages().clone()"),
            removed_versions,
            self.removed_versions_by_package.clone(),
            pool.get_all_security_removed_package_versions().clone(),
            pool.get_all_abandoned_removed_package_versions().clone(),
        )
    }

    fn optimize_by_identical_dependencies(
        &mut self,
        _request: &Request,
        pool: &Pool,
    ) -> Result<()> {
        let mut identical_definitions_per_package: IndexMap<
            String,
            IndexMap<String, IndexMap<String, Vec<Box<dyn BasePackage>>>>,
        > = IndexMap::new();
        let mut package_identical_definition_lookup: IndexMap<
            i64,
            IndexMap<String, IdenticalDefinitionPointers>,
        > = IndexMap::new();

        for package in pool.get_packages() {
            // If that package was already marked irremovable, we can skip
            // the entire process for it
            if self.irremovable_packages.contains_key(&package.id) {
                continue;
            }

            self.mark_package_for_removal(package.id)?;

            let dependency_hash = self.calculate_dependency_hash(package);

            for package_name in package.get_names(false) {
                if !self
                    .require_constraints_per_package
                    .contains_key(&package_name)
                {
                    continue;
                }

                let require_constraints = self
                    .require_constraints_per_package
                    .get(&package_name)
                    .cloned()
                    .unwrap_or_default();
                for (_, require_constraint) in require_constraints.iter() {
                    let mut group_hash_parts: Vec<String> = vec![];

                    if CompilingMatcher::r#match(
                        require_constraint.as_ref(),
                        Constraint::OP_EQ,
                        package.get_version(),
                    ) {
                        group_hash_parts.push(format!("require:{}", require_constraint));
                    }

                    if package.get_replaces().len() > 0 {
                        for link in package.get_replaces() {
                            if CompilingMatcher::r#match(
                                link.get_constraint(),
                                Constraint::OP_EQ,
                                package.get_version(),
                            ) {
                                // Use the same hash part as the regular require hash because that's what the replacement does
                                group_hash_parts.push(format!("require:{}", link.get_constraint()));
                            }
                        }
                    }

                    if let Some(conflict_constraints) =
                        self.conflict_constraints_per_package.get(&package_name)
                    {
                        for (_, conflict_constraint) in conflict_constraints {
                            if CompilingMatcher::r#match(
                                conflict_constraint.as_ref(),
                                Constraint::OP_EQ,
                                package.get_version(),
                            ) {
                                group_hash_parts.push(format!("conflict:{}", conflict_constraint));
                            }
                        }
                    }

                    if 0 == group_hash_parts.len() {
                        continue;
                    }

                    let group_hash = implode("", &group_hash_parts);
                    identical_definitions_per_package
                        .entry(package_name.clone())
                        .or_insert_with(IndexMap::new)
                        .entry(group_hash.clone())
                        .or_insert_with(IndexMap::new)
                        .entry(dependency_hash.clone())
                        .or_insert_with(Vec::new)
                        .push(package.clone_box());
                    package_identical_definition_lookup
                        .entry(package.id)
                        .or_insert_with(IndexMap::new)
                        .insert(
                            package_name.clone(),
                            IdenticalDefinitionPointers {
                                group_hash,
                                dependency_hash: dependency_hash.clone(),
                            },
                        );
                }
            }
        }

        // PHP: foreach ($identicalDefinitionsPerPackage as $constraintGroups)
        let identical_clone = identical_definitions_per_package.clone();
        for (_, constraint_groups) in identical_clone.iter() {
            for (_, constraint_group) in constraint_groups.iter() {
                for (_, packages) in constraint_group.iter() {
                    // Only one package in this constraint group has the same requirements, we're not allowed to remove that package
                    if 1 == packages.len() {
                        self.keep_package(
                            &packages[0],
                            &identical_definitions_per_package,
                            &package_identical_definition_lookup,
                        );
                        continue;
                    }

                    // Otherwise we find out which one is the preferred package in this constraint group which is
                    // then not allowed to be removed either
                    let mut literals: Vec<i64> = vec![];

                    for package in packages {
                        literals.push(package.id);
                    }

                    for preferred_literal in self
                        .policy
                        .select_preferred_packages(pool, literals.clone())
                    {
                        self.keep_package(
                            &pool.literal_to_package(preferred_literal),
                            &identical_definitions_per_package,
                            &package_identical_definition_lookup,
                        );
                    }
                }
            }
        }

        Ok(())
    }

    fn calculate_dependency_hash(&self, package: &dyn BasePackage) -> String {
        let mut hash = String::new();

        let hash_relevant_links: Vec<(&str, Vec<crate::package::link::Link>)> = vec![
            (
                "requires",
                package.get_requires().values().cloned().collect(),
            ),
            ("conflicts", package.get_conflicts()),
            ("replaces", package.get_replaces()),
            ("provides", package.get_provides()),
        ];

        for (key, links) in hash_relevant_links {
            if 0 == links.len() {
                continue;
            }

            // start new hash section
            hash.push_str(&format!("{}:", key));

            let mut subhash: IndexMap<String, String> = IndexMap::new();

            for link in links {
                // To get the best dependency hash matches we should use Intervals::compactConstraint() here.
                // However, the majority of projects are going to specify their constraints already pretty
                // much in the best variant possible. In other words, we'd be wasting time here and it would actually hurt
                // performance more than the additional few packages that could be filtered out would benefit the process.
                subhash.insert(
                    link.get_target().to_string(),
                    link.get_constraint().to_string(),
                );
            }

            // Sort for best result
            ksort(&mut subhash);

            for (target, constraint) in subhash {
                hash.push_str(&format!("{}@{}", target, constraint));
            }
        }

        hash
    }

    fn mark_package_for_removal(&mut self, id: i64) -> Result<()> {
        // We are not allowed to remove packages if they have been marked as irremovable
        if self.irremovable_packages.contains_key(&id) {
            return Err(LogicException {
                message: "Attempted removing a package which was previously marked irremovable"
                    .to_string(),
                code: 0,
            }
            .into());
        }

        self.packages_to_remove.insert(id, true);
        Ok(())
    }

    /// @param array<string, array<string, array<string, list<BasePackage>>>> $identicalDefinitionsPerPackage
    /// @param array<int, array<string, array{groupHash: string, dependencyHash: string}>> $packageIdenticalDefinitionLookup
    fn keep_package(
        &mut self,
        package: &dyn BasePackage,
        identical_definitions_per_package: &IndexMap<
            String,
            IndexMap<String, IndexMap<String, Vec<Box<dyn BasePackage>>>>,
        >,
        package_identical_definition_lookup: &IndexMap<
            i64,
            IndexMap<String, IdenticalDefinitionPointers>,
        >,
    ) {
        // Already marked to keep
        if !self.packages_to_remove.contains_key(&package.id) {
            return;
        }

        self.packages_to_remove.shift_remove(&package.id);

        if let Some(alias_pkg) = (package.as_any() as &dyn Any).downcast_ref::<AliasPackage>() {
            // recursing here so aliasesPerPackage for the aliasOf can be checked
            // and all its aliases marked to be kept as well
            self.keep_package(
                alias_pkg.get_alias_of(),
                identical_definitions_per_package,
                package_identical_definition_lookup,
            );
        }

        // record all the versions of the package group so we can list them later in Problem output
        for name in package.get_names(false) {
            if let Some(per_name) = package_identical_definition_lookup.get(&package.id) {
                if let Some(package_group_pointers) = per_name.get(&name) {
                    let package_group = identical_definitions_per_package
                        .get(&name)
                        .and_then(|m| m.get(&package_group_pointers.group_hash))
                        .and_then(|m| m.get(&package_group_pointers.dependency_hash));
                    if let Some(package_group) = package_group {
                        for pkg in package_group {
                            let pkg = if let Some(alias_pkg) =
                                (pkg.as_any() as &dyn Any).downcast_ref::<AliasPackage>()
                            {
                                if alias_pkg.get_pretty_version()
                                    == VersionParser::DEFAULT_BRANCH_ALIAS
                                {
                                    alias_pkg.get_alias_of().clone_box()
                                } else {
                                    pkg.clone_box()
                                }
                            } else {
                                pkg.clone_box()
                            };
                            self.removed_versions_by_package
                                .entry(spl_object_hash(package))
                                .or_insert_with(IndexMap::new)
                                .insert(
                                    pkg.get_version().to_string(),
                                    pkg.get_pretty_version().to_string(),
                                );
                        }
                    }
                }
            }
        }

        let aliases = self
            .aliases_per_package
            .get(&package.id)
            .cloned()
            .unwrap_or_default();
        for alias_package in aliases {
            self.packages_to_remove.shift_remove(&alias_package.id);

            // record all the versions of the package group so we can list them later in Problem output
            for name in alias_package.get_names(false) {
                if let Some(per_name) = package_identical_definition_lookup.get(&alias_package.id) {
                    if let Some(package_group_pointers) = per_name.get(&name) {
                        let package_group = identical_definitions_per_package
                            .get(&name)
                            .and_then(|m| m.get(&package_group_pointers.group_hash))
                            .and_then(|m| m.get(&package_group_pointers.dependency_hash));
                        if let Some(package_group) = package_group {
                            for pkg in package_group {
                                let pkg = if let Some(alias_pkg) =
                                    (pkg.as_any() as &dyn Any).downcast_ref::<AliasPackage>()
                                {
                                    if alias_pkg.get_pretty_version()
                                        == VersionParser::DEFAULT_BRANCH_ALIAS
                                    {
                                        alias_pkg.get_alias_of().clone_box()
                                    } else {
                                        pkg.clone_box()
                                    }
                                } else {
                                    pkg.clone_box()
                                };
                                self.removed_versions_by_package
                                    .entry(spl_object_hash(alias_package.as_ref()))
                                    .or_insert_with(IndexMap::new)
                                    .insert(
                                        pkg.get_version().to_string(),
                                        pkg.get_pretty_version().to_string(),
                                    );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Use the list of locked packages to constrain the loaded packages
    /// This will reduce packages with significant numbers of historical versions to a smaller number
    /// and reduce the resulting rule set that is generated
    fn optimize_impossible_packages_away(&mut self, request: &Request, pool: &Pool) {
        if request.get_locked_packages().len() == 0 {
            return;
        }

        let mut package_index: IndexMap<String, IndexMap<i64, Box<dyn BasePackage>>> =
            IndexMap::new();

        for package in pool.get_packages() {
            let id = package.id;

            // Do not remove irremovable packages
            if self.irremovable_packages.contains_key(&id) {
                continue;
            }
            // Do not remove a package aliased by another package, nor aliases
            if self.aliases_per_package.contains_key(&id)
                || (package.as_any() as &dyn Any)
                    .downcast_ref::<AliasPackage>()
                    .is_some()
            {
                continue;
            }
            // Do not remove locked packages
            if request.is_fixed_package(package)
                || request.is_locked_package(todo!("package as &dyn PackageInterface"))
            {
                continue;
            }

            package_index
                .entry(package.get_name().to_string())
                .or_insert_with(IndexMap::new)
                .insert(package.id, package.clone_box());
        }

        for (_, package) in request.get_locked_packages() {
            // If this locked package is no longer required by root or anything in the pool, it may get uninstalled so do not apply its requirements
            // In a case where a requirement WERE to appear in the pool by a package that would not be used, it would've been unlocked and so not filtered still
            let mut is_unused_package = true;
            for package_name in package.get_names(false) {
                if self
                    .require_constraints_per_package
                    .contains_key(&package_name)
                {
                    is_unused_package = false;
                    break;
                }
            }

            if is_unused_package {
                continue;
            }

            for link in package.get_requires().values() {
                let require = link.get_target();
                if !package_index.contains_key(require) {
                    continue;
                }

                let link_constraint = link.get_constraint();
                let ids: Vec<i64> = package_index
                    .get(require)
                    .map(|m| m.keys().copied().collect())
                    .unwrap_or_default();
                for id in ids {
                    let required_pkg = package_index.get(require).unwrap().get(&id).cloned();
                    if let Some(required_pkg) = required_pkg {
                        if false
                            == CompilingMatcher::r#match(
                                link_constraint,
                                Constraint::OP_EQ,
                                required_pkg.get_version(),
                            )
                        {
                            // TODO(phase-b): mark_package_for_removal returns Result; ignoring here
                            let _ = self.mark_package_for_removal(id);
                            if let Some(map) = package_index.get_mut(require) {
                                map.shift_remove(&id);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Disjunctive require constraints need to be considered in their own group. E.g. "^2.14 || ^3.3" needs to generate
    /// two require constraint groups in order for us to keep the best matching package for "^2.14" AND "^3.3" as otherwise, we'd
    /// only keep either one which can cause trouble (e.g. when using --prefer-lowest).
    ///
    /// @return void
    fn extract_require_constraints_per_package(
        &mut self,
        package: &str,
        constraint: Box<dyn ConstraintInterface>,
    ) {
        for expanded in self.expand_disjunctive_multi_constraints(constraint) {
            self.require_constraints_per_package
                .entry(package.to_string())
                .or_insert_with(IndexMap::new)
                .insert(expanded.to_string(), expanded);
        }
    }

    /// Disjunctive conflict constraints need to be considered in their own group. E.g. "^2.14 || ^3.3" needs to generate
    /// two conflict constraint groups in order for us to keep the best matching package for "^2.14" AND "^3.3" as otherwise, we'd
    /// only keep either one which can cause trouble (e.g. when using --prefer-lowest).
    ///
    /// @return void
    fn extract_conflict_constraints_per_package(
        &mut self,
        package: &str,
        constraint: Box<dyn ConstraintInterface>,
    ) {
        for expanded in self.expand_disjunctive_multi_constraints(constraint) {
            self.conflict_constraints_per_package
                .entry(package.to_string())
                .or_insert_with(IndexMap::new)
                .insert(expanded.to_string(), expanded);
        }
    }

    /// @return ConstraintInterface[]
    fn expand_disjunctive_multi_constraints(
        &self,
        constraint: Box<dyn ConstraintInterface>,
    ) -> Vec<Box<dyn ConstraintInterface>> {
        // TODO(phase-b): Intervals::compactConstraint expects/returns ConstraintInterface
        let constraint = Intervals::compact_constraint(constraint);

        if let Some(multi) = (constraint.as_ref() as &dyn Any).downcast_ref::<MultiConstraint>() {
            if multi.is_disjunctive() {
                // No need to call ourselves recursively here because Intervals::compactConstraint() ensures that there
                // are no nested disjunctive MultiConstraint instances possible
                return multi.get_constraints();
            }
        }

        // Regular constraints and conjunctive MultiConstraints
        vec![constraint]
    }
}
