//! ref: composer/src/Composer/DependencyResolver/PoolBuilder.php

use crate::io::io_interface;
use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::composer::semver::compiling_matcher::CompilingMatcher;
use shirabe_external_packages::composer::semver::intervals::Intervals;
use shirabe_php_shim::{
    LogicException, PhpMixed, array_chunk, array_flip, array_map, array_merge, array_search, count,
    in_array, microtime, number_format, round, spl_object_hash, sprintf, strpos,
};
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;
use shirabe_semver::constraint::match_all_constraint::MatchAllConstraint;
use shirabe_semver::constraint::multi_constraint::MultiConstraint;

use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::pool_optimizer::PoolOptimizer;
use crate::dependency_resolver::request::Request;
use crate::dependency_resolver::security_advisory_pool_filter::SecurityAdvisoryPoolFilter;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::io::io_interface::IOInterface;
use crate::package::alias_package::AliasPackage;
use crate::package::base_package::BasePackage;
use crate::package::complete_alias_package::CompleteAliasPackage;
use crate::package::complete_package::CompletePackage;
use crate::package::package_interface::PackageInterface;
use crate::package::version::stability_filter::StabilityFilter;
use crate::plugin::plugin_events::PluginEvents;
use crate::plugin::pre_pool_create_event::PrePoolCreateEvent;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_interface::RepositoryInterface;
use crate::repository::root_package_repository::RootPackageRepository;

#[derive(Debug)]
pub struct PoolBuilder {
    acceptable_stabilities: IndexMap<String, i64>,
    stability_flags: IndexMap<String, i64>,
    root_aliases: IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
    root_references: IndexMap<String, String>,
    temporary_constraints: IndexMap<String, Box<dyn ConstraintInterface>>,
    event_dispatcher: Option<EventDispatcher>,
    pool_optimizer: Option<PoolOptimizer>,
    io: Box<dyn IOInterface>,
    alias_map: IndexMap<String, IndexMap<i64, AliasPackage>>,
    packages_to_load: IndexMap<String, Box<dyn ConstraintInterface>>,
    loaded_packages: IndexMap<String, Box<dyn ConstraintInterface>>,
    loaded_per_repo: IndexMap<i64, IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>>,
    packages: IndexMap<i64, Box<dyn BasePackage>>,
    unacceptable_fixed_or_locked_packages: Vec<Box<dyn BasePackage>>,
    update_allow_list: Vec<String>,
    skipped_load: IndexMap<String, Vec<Box<dyn PackageInterface>>>,
    ignored_types: Vec<String>,
    allowed_types: Option<Vec<String>>,
    /// If provided, only these package names are loaded
    ///
    /// This is a special-use functionality of the Request class to optimize the pool creation process
    /// when only a minimal subset of packages is needed and we do not need their dependencies.
    restricted_packages_list: Option<IndexMap<String, i64>>,
    /// Keeps a list of dependencies which are locked but were auto-unlocked as they are path repositories
    ///
    /// This half-unlocked state means the package itself will update but the UPDATE_LISTED_WITH_TRANSITIVE_DEPS*
    /// flags will not apply until the package really gets unlocked in some other way than being a path repo
    path_repo_unlocked: IndexMap<String, bool>,
    /// Keeps a list of dependencies which are root requirements, and as such
    /// have already their maximum required range loaded and can not be
    /// extended by markPackageNameForLoading
    ///
    /// Packages get cleared from this list if they get unlocked as in that case
    /// we need to actually load them
    max_extended_reqs: IndexMap<String, bool>,
    update_allow_warned: IndexMap<String, bool>,
    index_counter: i64,
    security_advisory_pool_filter: Option<SecurityAdvisoryPoolFilter>,
}

impl PoolBuilder {
    const LOAD_BATCH_SIZE: i64 = 50;

    pub fn new(
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        root_aliases: IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
        root_references: IndexMap<String, String>,
        io: Box<dyn IOInterface>,
        event_dispatcher: Option<EventDispatcher>,
        pool_optimizer: Option<PoolOptimizer>,
        temporary_constraints: IndexMap<String, Box<dyn ConstraintInterface>>,
        security_advisory_pool_filter: Option<SecurityAdvisoryPoolFilter>,
    ) -> Self {
        Self {
            acceptable_stabilities,
            stability_flags,
            root_aliases,
            root_references,
            event_dispatcher,
            pool_optimizer,
            io,
            temporary_constraints,
            security_advisory_pool_filter,
            alias_map: IndexMap::new(),
            packages_to_load: IndexMap::new(),
            loaded_packages: IndexMap::new(),
            loaded_per_repo: IndexMap::new(),
            packages: IndexMap::new(),
            unacceptable_fixed_or_locked_packages: vec![],
            update_allow_list: vec![],
            skipped_load: IndexMap::new(),
            ignored_types: vec![],
            allowed_types: None,
            restricted_packages_list: None,
            path_repo_unlocked: IndexMap::new(),
            max_extended_reqs: IndexMap::new(),
            update_allow_warned: IndexMap::new(),
            index_counter: 0,
        }
    }

    /// Packages of those types are ignored
    pub fn set_ignored_types(&mut self, types: Vec<String>) {
        self.ignored_types = types;
    }

    /// Only packages of those types are allowed if set to non-null
    pub fn set_allowed_types(&mut self, types: Option<Vec<String>>) {
        self.allowed_types = types;
    }

    pub fn build_pool(
        &mut self,
        repositories: Vec<Box<dyn RepositoryInterface>>,
        request: &mut Request,
    ) -> anyhow::Result<Pool> {
        self.restricted_packages_list = if request.get_restricted_packages().is_some() {
            Some(array_flip(&request.get_restricted_packages().unwrap()))
        } else {
            None
        };

        if count(&request.get_update_allow_list()) > 0 {
            self.update_allow_list = request.get_update_allow_list();
            self.warn_about_non_matching_update_allow_list(request)?;

            if request.get_locked_repository().is_none() {
                return Err(LogicException {
                    message: "No lock repo present and yet a partial update was requested."
                        .to_string(),
                    code: 0,
                }
                .into());
            }

            for locked_package in request.get_locked_repository().unwrap().get_packages() {
                if !self.is_update_allowed(&*locked_package) {
                    // remember which packages we skipped loading remote content for in this partial update
                    self.skipped_load
                        .entry(locked_package.get_name().to_string())
                        .or_insert_with(Vec::new)
                        .push(locked_package.clone_box());
                    for (_k, link) in &locked_package.get_replaces() {
                        self.skipped_load
                            .entry(link.get_target().to_string())
                            .or_insert_with(Vec::new)
                            .push(locked_package.clone_box());
                    }

                    // Path repo packages are never loaded from lock, to force them to always remain in sync
                    // unless symlinking is disabled in which case we probably should rather treat them like
                    // regular packages. We mark them specially so they can be reloaded fully including update propagation
                    // if they do get unlocked, but by default they are unlocked without update propagation.
                    if locked_package.get_dist_type().as_deref() == Some("path") {
                        let transport_options = locked_package.get_transport_options();
                        let symlink_disabled = transport_options
                            .get("symlink")
                            .map(|v| v.as_bool() == Some(false))
                            .unwrap_or(false);
                        if !transport_options.contains_key("symlink") || !symlink_disabled {
                            self.path_repo_unlocked
                                .insert(locked_package.get_name().to_string(), true);
                            continue;
                        }
                    }

                    request.lock_package(&*locked_package);
                }
            }
        }

        for package in request.get_fixed_or_locked_packages() {
            // using MatchAllConstraint here because fixed packages do not need to retrigger
            // loading any packages
            self.loaded_packages.insert(
                package.get_name().to_string(),
                Box::new(MatchAllConstraint::new()),
            );

            // replace means conflict, so if a fixed package replaces a name, no need to load that one, packages would conflict anyways
            for (_k, link) in &package.get_replaces() {
                self.loaded_packages.insert(
                    link.get_target().to_string(),
                    Box::new(MatchAllConstraint::new()),
                );
            }

            // TODO in how far can we do the above for conflicts? It's more tricky cause conflicts can be limited to
            // specific versions while replace is a conflict with all versions of the name

            let in_root_or_platform = package
                .get_repository()
                .map(|r| {
                    r.as_any().is::<RootPackageRepository>()
                        || r.as_any().is::<PlatformRepository>()
                })
                .unwrap_or(false);
            if in_root_or_platform
                || StabilityFilter::is_package_acceptable(
                    &self.acceptable_stabilities,
                    &self.stability_flags,
                    &package.get_names(true),
                    package.get_stability(),
                )
            {
                self.load_package(request, &repositories, &*package, false)?;
            } else {
                self.unacceptable_fixed_or_locked_packages
                    .push(package.clone_box());
            }
        }

        for (package_name, constraint) in &request.get_requires() {
            // fixed and locked packages have already been added, so if a root require needs one of them, no need to do anything
            if self.loaded_packages.contains_key(package_name) {
                continue;
            }

            self.packages_to_load
                .insert(package_name.clone(), constraint.clone_box());
            self.max_extended_reqs.insert(package_name.clone(), true);
        }

        // clean up packagesToLoad for anything we manually marked loaded above
        let to_remove: Vec<String> = self
            .packages_to_load
            .keys()
            .filter(|name| self.loaded_packages.contains_key(*name))
            .cloned()
            .collect();
        for name in to_remove {
            self.packages_to_load.shift_remove(&name);
        }

        while count(&self.packages_to_load) > 0 {
            self.load_packages_marked_for_loading(request, &repositories)?;
        }

        if count(&self.temporary_constraints) > 0 {
            let indices: Vec<i64> = self.packages.keys().cloned().collect();
            for i in indices {
                let package = match self.packages.get(&i) {
                    Some(p) => p.clone_box(),
                    None => continue,
                };
                // we check all alias related packages at once, so no need to check individual aliases
                if package.as_alias_package().is_some() {
                    continue;
                }

                for package_name in package.get_names(true) {
                    let constraint = match self.temporary_constraints.get(&package_name) {
                        Some(c) => c.clone_box(),
                        None => continue,
                    };

                    let mut package_and_aliases: IndexMap<i64, Box<dyn BasePackage>> =
                        IndexMap::new();
                    package_and_aliases.insert(i, package.clone_box());
                    if let Some(aliases) = self.alias_map.get(&spl_object_hash(&*package)) {
                        for (idx, alias) in aliases {
                            package_and_aliases.insert(*idx, Box::new(alias.clone()));
                        }
                    }

                    let mut found = false;
                    for (_idx, package_or_alias) in &package_and_aliases {
                        if CompilingMatcher::matches(
                            &*constraint,
                            Constraint::OP_EQ,
                            package_or_alias.get_version(),
                        ) {
                            found = true;
                        }
                    }

                    if !found {
                        for (index, _) in &package_and_aliases {
                            self.packages.shift_remove(index);
                        }
                    }
                }
            }
        }

        if self.event_dispatcher.is_some() {
            let mut pre_pool_create_event = PrePoolCreateEvent::new(
                PluginEvents::PRE_POOL_CREATE,
                repositories.clone(),
                request,
                self.acceptable_stabilities.clone(),
                self.stability_flags.clone(),
                self.root_aliases.clone(),
                self.root_references.clone(),
                self.packages.values().map(|p| p.clone_box()).collect(),
                self.unacceptable_fixed_or_locked_packages
                    .iter()
                    .map(|p| p.clone_box())
                    .collect(),
            );
            self.event_dispatcher
                .as_mut()
                .unwrap()
                .dispatch(pre_pool_create_event.get_name(), &mut pre_pool_create_event);
            // PHP rebinds $this->packages to a list-style array; preserve indices via reindexing.
            self.packages = pre_pool_create_event
                .get_packages()
                .into_iter()
                .enumerate()
                .map(|(i, p)| (i as i64, p))
                .collect();
            self.unacceptable_fixed_or_locked_packages =
                pre_pool_create_event.get_unacceptable_fixed_packages();
        }

        let mut pool = Pool::new(
            self.packages.values().map(|p| p.clone_box()).collect(),
            self.unacceptable_fixed_or_locked_packages
                .iter()
                .map(|p| p.clone_box())
                .collect(),
        );

        self.alias_map = IndexMap::new();
        self.packages_to_load = IndexMap::new();
        self.loaded_packages = IndexMap::new();
        self.loaded_per_repo = IndexMap::new();
        self.packages = IndexMap::new();
        self.unacceptable_fixed_or_locked_packages = vec![];
        self.max_extended_reqs = IndexMap::new();
        self.skipped_load = IndexMap::new();
        self.index_counter = 0;

        self.io.debug("Built pool.");

        // filter vulnerable packages before optimizing the pool otherwise we may end up with inconsistent state where the optimizer took away versions
        // that were not vulnerable and now suddenly the vulnerable ones are removed and we are missing some versions to make it solvable
        pool = self.run_security_advisory_filter(pool, &repositories, request);
        pool = self.run_optimizer(request, pool);

        Intervals::clear();

        Ok(pool)
    }

    fn mark_package_name_for_loading(
        &mut self,
        request: &Request,
        name: &str,
        constraint: Box<dyn ConstraintInterface>,
    ) {
        // Skip platform requires at this stage
        if PlatformRepository::is_platform_package(name) {
            return;
        }

        // Root require (which was not unlocked) already loaded the maximum range so no
        // need to check anything here
        if self.max_extended_reqs.contains_key(name) {
            return;
        }

        // Root requires can not be overruled by dependencies so there is no point in
        // extending the loaded constraint for those.
        // This is triggered when loading a root require which was locked but got unlocked, then
        // we make sure that we load at most the intervals covered by the root constraint.
        let root_requires = request.get_requires();
        let mut constraint = constraint;
        if let Some(root_constraint) = root_requires.get(name) {
            if !Intervals::is_subset_of(&*constraint, &**root_constraint) {
                constraint = root_constraint.clone_box();
            }
        }

        // Not yet loaded or already marked for a reload, set the constraint to be loaded
        if !self.loaded_packages.contains_key(name) {
            // Maybe it was already marked before but not loaded yet. In that case
            // we have to extend the constraint (we don't check if they are identical because
            // MultiConstraint::create() will optimize anyway)
            if let Some(existing) = self.packages_to_load.get(name) {
                // Already marked for loading and this does not expand the constraint to be loaded, nothing to do
                if Intervals::is_subset_of(&*constraint, &**existing) {
                    return;
                }

                // extend the constraint to be loaded
                constraint = Intervals::compact_constraint(MultiConstraint::create(
                    vec![existing.clone_box(), constraint.clone_box()],
                    false,
                ));
            }

            self.packages_to_load.insert(name.to_string(), constraint);

            return;
        }

        // No need to load this package with this constraint because it is
        // a subset of the constraint with which we have already loaded packages
        if Intervals::is_subset_of(&*constraint, &**self.loaded_packages.get(name).unwrap()) {
            return;
        }

        // We have already loaded that package but not in the constraint that's
        // required. We extend the constraint and mark that package as not being loaded
        // yet so we get the required package versions
        self.packages_to_load.insert(
            name.to_string(),
            Intervals::compact_constraint(MultiConstraint::create(
                vec![
                    self.loaded_packages.get(name).unwrap().clone_box(),
                    constraint,
                ],
                false,
            )),
        );
        self.loaded_packages.shift_remove(name);
    }

    fn load_packages_marked_for_loading(
        &mut self,
        request: &mut Request,
        repositories: &Vec<Box<dyn RepositoryInterface>>,
    ) -> anyhow::Result<()> {
        let to_remove: Vec<String> = self
            .packages_to_load
            .keys()
            .filter(|name| {
                self.restricted_packages_list
                    .as_ref()
                    .map(|r| !r.contains_key(*name))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        for name in to_remove {
            self.packages_to_load.shift_remove(&name);
        }
        let snapshot: Vec<(String, Box<dyn ConstraintInterface>)> = self
            .packages_to_load
            .iter()
            .map(|(k, v)| (k.clone(), v.clone_box()))
            .collect();
        for (name, constraint) in &snapshot {
            self.loaded_packages
                .insert(name.clone(), constraint.clone_box());
        }

        // Load packages in chunks of 50 to prevent memory usage build-up due to caches of all sorts
        let mut package_batches = array_chunk(&self.packages_to_load, Self::LOAD_BATCH_SIZE, true);
        self.packages_to_load = IndexMap::new();

        for (repo_index, repository) in repositories.iter().enumerate() {
            // these repos have their packages fixed or locked if they need to be loaded so we
            // never need to load anything else from them
            let is_locked_repo = request
                .get_locked_repository()
                .map(|lr| {
                    std::ptr::eq(
                        lr as *const _ as *const u8,
                        repository.as_ref() as *const _ as *const u8,
                    )
                })
                .unwrap_or(false);
            if repository.as_any().is::<PlatformRepository>() || is_locked_repo {
                continue;
            }

            if 0 == count(&package_batches) {
                break;
            }

            for (batch_index, package_batch) in package_batches.clone().iter().enumerate() {
                let result = repository.load_packages(
                    package_batch,
                    &self.acceptable_stabilities,
                    &self.stability_flags,
                    self.loaded_per_repo
                        .get(&(repo_index as i64))
                        .cloned()
                        .unwrap_or_default(),
                );

                let names_found = result
                    .get("namesFound")
                    .and_then(|v| v.as_list())
                    .cloned()
                    .unwrap_or_default();
                for name in &names_found {
                    // avoid loading the same package again from other repositories once it has been found
                    if let Some(b) = package_batches.get_mut(batch_index) {
                        b.shift_remove(name.as_string().unwrap_or(""));
                    }
                }
                let packages_in_result = result
                    .get("packages")
                    .and_then(|v| v.as_list())
                    .cloned()
                    .unwrap_or_default();
                for package in &packages_in_result {
                    let pkg = match package.as_package_interface() {
                        Some(p) => p,
                        None => continue,
                    };
                    self.loaded_per_repo
                        .entry(repo_index as i64)
                        .or_insert_with(IndexMap::new)
                        .entry(pkg.get_name().to_string())
                        .or_insert_with(IndexMap::new)
                        .insert(pkg.get_version().to_string(), pkg.clone_box());

                    if in_array(pkg.get_type(), &self.ignored_types, true)
                        || (self.allowed_types.is_some()
                            && !in_array(
                                pkg.get_type(),
                                self.allowed_types.as_ref().unwrap(),
                                true,
                            ))
                    {
                        continue;
                    }
                    if let Some(bp) = pkg.as_base_package() {
                        let propagate = !self.path_repo_unlocked.contains_key(pkg.get_name());
                        self.load_package(request, repositories, &*bp, propagate)?;
                    }
                }
            }

            // PHP: array_chunk(array_merge(...$packageBatches), self::LOAD_BATCH_SIZE, true)
            let mut merged: IndexMap<String, Box<dyn ConstraintInterface>> = IndexMap::new();
            for batch in &package_batches {
                for (k, v) in batch {
                    merged.insert(k.clone(), v.clone_box());
                }
            }
            package_batches = array_chunk(&merged, Self::LOAD_BATCH_SIZE, true);
        }
        Ok(())
    }

    fn load_package(
        &mut self,
        request: &mut Request,
        repositories: &Vec<Box<dyn RepositoryInterface>>,
        package: &dyn BasePackage,
        propagate_update: bool,
    ) -> anyhow::Result<()> {
        let index = self.index_counter;
        self.index_counter += 1;
        self.packages.insert(index, package.clone_box());

        if let Some(alias) = package.as_alias_package() {
            self.alias_map
                .entry(spl_object_hash(alias.get_alias_of()))
                .or_insert_with(IndexMap::new)
                .insert(index, alias.clone());
        }

        let name = package.get_name().to_string();

        // we're simply setting the root references on all versions for a name here and rely on the solver to pick the
        // right version. It'd be more work to figure out which versions and which aliases of those versions this may
        // apply to
        if let Some(reference) = self.root_references.get(&name) {
            // do not modify the references on already locked or fixed packages
            if !request.is_locked_package(package) && !request.is_fixed_package(package) {
                package.set_source_dist_references(reference);
            }
        }

        // if propagateUpdate is false we are loading a fixed or locked package, root aliases do not apply as they are
        // manually loaded as separate packages in this case
        //
        // packages in pathRepoUnlocked however need to also load root aliases, they have propagateUpdate set to
        // false because their deps should not be unlocked, but that is irrelevant for root aliases
        let path_repo_match = self.path_repo_unlocked.contains_key(package.get_name());
        let alias_for_version = self
            .root_aliases
            .get(&name)
            .and_then(|m| m.get(package.get_version()))
            .cloned();
        if (propagate_update || path_repo_match) && alias_for_version.is_some() {
            let alias = alias_for_version.unwrap();
            let base_package: Box<dyn BasePackage> = if let Some(ap) = package.as_alias_package() {
                ap.get_alias_of().clone_box()
            } else {
                package.clone_box()
            };
            let alias_package: Box<dyn BasePackage> =
                if base_package.as_any().is::<CompletePackage>() {
                    Box::new(CompleteAliasPackage::new(
                        base_package.clone_box(),
                        alias.get("alias_normalized").cloned().unwrap_or_default(),
                        alias.get("alias").cloned().unwrap_or_default(),
                    ))
                } else {
                    Box::new(AliasPackage::new(
                        base_package.clone_box(),
                        alias.get("alias_normalized").cloned().unwrap_or_default(),
                        alias.get("alias").cloned().unwrap_or_default(),
                    ))
                };
            // PHP: $aliasPackage->setRootPackageAlias(true);
            // BasePackage doesn't expose this directly; the AliasPackage trait method handles it.

            let new_index = self.index_counter;
            self.index_counter += 1;
            self.packages.insert(new_index, alias_package.clone_box());
            if let Some(ap) = alias_package.as_alias_package() {
                self.alias_map
                    .entry(spl_object_hash(ap.get_alias_of()))
                    .or_insert_with(IndexMap::new)
                    .insert(new_index, ap.clone());
            }
        }

        let requires = package.get_requires();
        for (_k, link) in &requires {
            let require = link.get_target().to_string();
            let link_constraint = link.get_constraint();

            // if the required package is loaded as a locked package only and hasn't had its deps analyzed
            if self.skipped_load.contains_key(&require) {
                // if we're doing a full update or this is a partial update with transitive deps and we're currently
                // looking at a package which needs to be updated we need to unlock the package we now know is a
                // dependency of another package which we are trying to update, and then attempt to load it again
                if propagate_update && request.get_update_allow_transitive_dependencies() {
                    let skipped_root_requires = self.get_skipped_root_requires(request, &require);

                    if request.get_update_allow_transitive_root_dependencies()
                        || 0 == count(&skipped_root_requires)
                    {
                        self.unlock_package(request, repositories, &require)?;
                        self.mark_package_name_for_loading(request, &require, link_constraint);
                    } else {
                        for root_require in &skipped_root_requires {
                            if !self.update_allow_warned.contains_key(root_require) {
                                self.update_allow_warned.insert(root_require.clone(), true);
                                self.io.write_error(&format!("<warning>Dependency {} is also a root requirement. Package has not been listed as an update argument, so keeping locked at old version. Use --with-all-dependencies (-W) to include root dependencies.</warning>", root_require));
                            }
                        }
                    }
                } else if self.path_repo_unlocked.contains_key(&require)
                    && !self.loaded_packages.contains_key(&require)
                {
                    // if doing a partial update and a package depends on a path-repo-unlocked package which is not referenced by the root, we need to ensure it gets loaded as it was not loaded by the request's root requirements
                    // and would not be loaded above if update propagation is not allowed (which happens if the requirer is itself a path-repo-unlocked package) or if transitive deps are not allowed to be unlocked
                    self.mark_package_name_for_loading(request, &require, link_constraint);
                }
            } else {
                self.mark_package_name_for_loading(request, &require, link_constraint);
            }
        }

        // if we're doing a partial update with deps we also need to unlock packages which are being replaced in case
        // they are currently locked and thus prevent this updateable package from being installable/updateable
        if propagate_update && request.get_update_allow_transitive_dependencies() {
            for (_k, link) in &package.get_replaces() {
                let replace = link.get_target().to_string();
                if self.loaded_packages.contains_key(&replace)
                    && self.skipped_load.contains_key(&replace)
                {
                    let skipped_root_requires = self.get_skipped_root_requires(request, &replace);

                    if request.get_update_allow_transitive_root_dependencies()
                        || 0 == count(&skipped_root_requires)
                    {
                        self.unlock_package(request, repositories, &replace)?;
                        // the replaced package only needs to be loaded if something else requires it
                        self.mark_package_name_for_loading_if_required(request, &replace);
                    } else {
                        for root_require in &skipped_root_requires {
                            if !self.update_allow_warned.contains_key(root_require) {
                                self.update_allow_warned.insert(root_require.clone(), true);
                                self.io.write_error(&format!("<warning>Dependency {} is also a root requirement. Package has not been listed as an update argument, so keeping locked at old version. Use --with-all-dependencies (-W) to include root dependencies.</warning>", root_require));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Checks if a particular name is required directly in the request
    fn is_root_require(&self, request: &Request, name: &str) -> bool {
        let root_requires = request.get_requires();

        root_requires.contains_key(name)
    }

    fn get_skipped_root_requires(&self, request: &Request, name: &str) -> Vec<String> {
        if !self.skipped_load.contains_key(name) {
            return vec![];
        }

        let root_requires = request.get_requires();
        let mut matches: Vec<String> = vec![];

        if root_requires.contains_key(name) {
            let name_owned = name.to_string();
            return array_map(
                |package: &Box<dyn PackageInterface>| -> String {
                    if name_owned != package.get_name() {
                        format!("{} (via replace of {})", package.get_name(), name_owned)
                    } else {
                        package.get_name().to_string()
                    }
                },
                &self.skipped_load[name],
            );
        }

        for package_or_replacer in &self.skipped_load[name] {
            if root_requires.contains_key(package_or_replacer.get_name()) {
                matches.push(package_or_replacer.get_name().to_string());
            }
            for (_k, link) in &package_or_replacer.get_replaces() {
                if root_requires.contains_key(link.get_target()) {
                    if name != package_or_replacer.get_name() {
                        matches.push(format!(
                            "{} (via replace of {})",
                            package_or_replacer.get_name(),
                            name
                        ));
                    } else {
                        matches.push(package_or_replacer.get_name().to_string());
                    }
                    break;
                }
            }
        }

        matches
    }

    /// Checks whether the update allow list allows this package in the lock file to be updated
    fn is_update_allowed(&self, package: &dyn BasePackage) -> bool {
        for pattern in &self.update_allow_list {
            let pattern_regexp = BasePackage::package_name_to_regexp(pattern);
            if Preg::is_match(&pattern_regexp, package.get_name(), None).unwrap_or(false) {
                return true;
            }
        }

        false
    }

    fn warn_about_non_matching_update_allow_list(&self, request: &Request) -> anyhow::Result<()> {
        if request.get_locked_repository().is_none() {
            return Err(LogicException {
                message: "No lock repo present and yet a partial update was requested.".to_string(),
                code: 0,
            }
            .into());
        }

        'outer: for pattern in &self.update_allow_list {
            let mut matched_platform_package = false;

            let pattern_regexp = BasePackage::package_name_to_regexp(pattern);
            // update pattern matches a locked package? => all good
            for package in request.get_locked_repository().unwrap().get_packages() {
                if Preg::is_match(&pattern_regexp, package.get_name(), None).unwrap_or(false) {
                    continue 'outer;
                }
            }
            // update pattern matches a root require? => all good, probably a new package
            for (package_name, _constraint) in &request.get_requires() {
                if Preg::is_match(&pattern_regexp, package_name, None).unwrap_or(false) {
                    if PlatformRepository::is_platform_package(package_name) {
                        matched_platform_package = true;
                        continue;
                    }
                    continue 'outer;
                }
            }
            if matched_platform_package {
                self.io.write_error(&format!(
                    "<warning>Pattern \"{}\" listed for update matches platform packages, but these cannot be updated by Composer.</warning>",
                    pattern
                ));
            } else if strpos(pattern, "*").is_some() {
                self.io.write_error(&format!(
                    "<warning>Pattern \"{}\" listed for update does not match any locked packages.</warning>",
                    pattern
                ));
            } else {
                self.io.write_error(&format!(
                    "<warning>Package \"{}\" listed for update is not locked.</warning>",
                    pattern
                ));
            }
        }
        Ok(())
    }

    /// Reverts the decision to use a locked package if a partial update with transitive dependencies
    /// found that this package actually needs to be updated
    fn unlock_package(
        &mut self,
        request: &mut Request,
        repositories: &Vec<Box<dyn RepositoryInterface>>,
        name: &str,
    ) -> anyhow::Result<()> {
        let skipped: Vec<Box<dyn PackageInterface>> = self
            .skipped_load
            .get(name)
            .map(|v| v.iter().map(|p| p.clone_box()).collect())
            .unwrap_or_default();
        for package_or_replacer in &skipped {
            // if we unfixed a replaced package name, we also need to unfix the replacer itself
            // as long as it was not unfixed yet
            if package_or_replacer.get_name() != name
                && self
                    .skipped_load
                    .contains_key(package_or_replacer.get_name())
            {
                let replacer_name = package_or_replacer.get_name().to_string();
                if request.get_update_allow_transitive_root_dependencies()
                    || (!self.is_root_require(request, name)
                        && !self.is_root_require(request, &replacer_name))
                {
                    self.unlock_package(request, repositories, &replacer_name)?;

                    if self.is_root_require(request, &replacer_name) {
                        self.mark_package_name_for_loading(
                            request,
                            &replacer_name,
                            Box::new(MatchAllConstraint::new()),
                        );
                    } else {
                        let pkgs: Vec<Box<dyn BasePackage>> =
                            self.packages.values().map(|p| p.clone_box()).collect();
                        for loaded_package in &pkgs {
                            let requires = loaded_package.get_requires();
                            if let Some(req_link) = requires.get(&replacer_name) {
                                self.mark_package_name_for_loading(
                                    request,
                                    &replacer_name,
                                    req_link.get_constraint(),
                                );
                            }
                        }
                    }
                }
            }
        }

        if self.path_repo_unlocked.contains_key(name) {
            let entries: Vec<(i64, Box<dyn BasePackage>)> = self
                .packages
                .iter()
                .filter(|(_, p)| p.get_name() == name)
                .map(|(i, p)| (*i, p.clone_box()))
                .collect();
            for (index, package) in &entries {
                self.remove_loaded_package(request, repositories, &**package, *index);
            }
        }

        self.skipped_load.shift_remove(name);
        self.loaded_packages.shift_remove(name);
        self.max_extended_reqs.shift_remove(name);
        self.path_repo_unlocked.shift_remove(name);

        // remove locked package by this name which was already initialized
        let locked_packages: Vec<Box<dyn BasePackage>> = request
            .get_locked_packages()
            .iter()
            .map(|p| p.clone_box())
            .collect();
        for locked_package in &locked_packages {
            if locked_package.as_alias_package().is_none() && locked_package.get_name() == name {
                let pkgs: Vec<Box<dyn BasePackage>> =
                    self.packages.values().map(|p| p.clone_box()).collect();
                let index_opt = array_search(&**locked_package, &pkgs, true);
                if let Some(index) = index_opt {
                    request.unlock_package(&**locked_package);
                    self.remove_loaded_package(request, repositories, &**locked_package, index);

                    // make sure that any requirements for this package by other locked or fixed packages are now
                    // also loaded, as they were previously ignored because the locked (now unlocked) package already
                    // satisfied their requirements
                    // and if this package is replacing another that is required by a locked or fixed package, ensure
                    // that we load that replaced package in case an update to this package removes the replacement
                    let fixed_or_locked: Vec<Box<dyn BasePackage>> = request
                        .get_fixed_or_locked_packages()
                        .iter()
                        .map(|p| p.clone_box())
                        .collect();
                    for fixed_or_locked_package in &fixed_or_locked {
                        if std::ptr::eq(
                            fixed_or_locked_package.as_ref() as *const _,
                            locked_package.as_ref() as *const _,
                        ) {
                            continue;
                        }

                        if self
                            .skipped_load
                            .contains_key(fixed_or_locked_package.get_name())
                        {
                            let requires = fixed_or_locked_package.get_requires();
                            if let Some(req_link) = requires.get(locked_package.get_name()) {
                                self.mark_package_name_for_loading(
                                    request,
                                    locked_package.get_name(),
                                    req_link.get_constraint(),
                                );
                            }

                            for (_k, replace) in &locked_package.get_replaces() {
                                if requires.contains_key(replace.get_target())
                                    && self.skipped_load.contains_key(replace.get_target())
                                {
                                    self.unlock_package(
                                        request,
                                        repositories,
                                        replace.get_target(),
                                    )?;
                                    // this package is in $requires so no need to call markPackageNameForLoadingIfRequired
                                    self.mark_package_name_for_loading(
                                        request,
                                        replace.get_target(),
                                        replace.get_constraint(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn mark_package_name_for_loading_if_required(&mut self, request: &Request, name: &str) {
        if self.is_root_require(request, name) {
            self.mark_package_name_for_loading(
                request,
                name,
                request.get_requires()[name].clone_box(),
            );
        }

        let pkgs: Vec<Box<dyn BasePackage>> =
            self.packages.values().map(|p| p.clone_box()).collect();
        for package in &pkgs {
            for (_k, link) in &package.get_requires() {
                if name == link.get_target() {
                    self.mark_package_name_for_loading(
                        request,
                        link.get_target(),
                        link.get_constraint(),
                    );
                }
            }
        }
    }

    fn remove_loaded_package(
        &mut self,
        _request: &Request,
        repositories: &Vec<Box<dyn RepositoryInterface>>,
        package: &dyn BasePackage,
        index: i64,
    ) {
        let repos_box: Vec<Box<dyn RepositoryInterface>> =
            repositories.iter().map(|r| r.clone_box()).collect();
        let repo_index = match package.get_repository() {
            Some(repo) => array_search(&*repo, &repos_box, true).unwrap_or(-1),
            None => -1,
        };

        if repo_index >= 0 {
            if let Some(repo_map) = self.loaded_per_repo.get_mut(&repo_index) {
                if let Some(name_map) = repo_map.get_mut(package.get_name()) {
                    name_map.shift_remove(package.get_version());
                }
            }
        }
        self.packages.shift_remove(&index);
        let object_hash = spl_object_hash(package);
        if let Some(aliases) = self.alias_map.get(&object_hash).cloned() {
            for (alias_index, alias_package) in &aliases {
                if repo_index >= 0 {
                    if let Some(repo_map) = self.loaded_per_repo.get_mut(&repo_index) {
                        if let Some(name_map) = repo_map.get_mut(alias_package.get_name()) {
                            name_map.shift_remove(alias_package.get_version());
                        }
                    }
                }
                self.packages.shift_remove(alias_index);
            }
            self.alias_map.shift_remove(&object_hash);
        }
    }

    fn run_optimizer(&mut self, request: &Request, pool: Pool) -> Pool {
        if self.pool_optimizer.is_none() {
            return pool;
        }

        self.io.debug("Running pool optimizer.");

        let before = microtime(true);
        let total = count(&pool.get_packages()) as f64;

        let pool = self
            .pool_optimizer
            .as_mut()
            .unwrap()
            .optimize(request, pool);

        let filtered = total - (count(&pool.get_packages()) as f64);

        if 0.0 == filtered {
            return pool;
        }

        self.io.write_with_verbosity(
            &sprintf(
                "Pool optimizer completed in %.3f seconds",
                &[(microtime(true) - before).into()],
            ),
            true,
            io_interface::VERY_VERBOSE,
        );
        self.io.write_with_verbosity(
            &sprintf(
                "<info>Found %s package versions referenced in your dependency graph. %s (%d%%) were optimized away.</info>",
                &[
                    number_format(total).into(),
                    number_format(filtered).into(),
                    round(100.0 / total * filtered).into(),
                ],
            ),
            true,
            io_interface::VERY_VERBOSE,
        );

        pool
    }

    fn run_security_advisory_filter(
        &mut self,
        pool: Pool,
        repositories: &Vec<Box<dyn RepositoryInterface>>,
        request: &Request,
    ) -> Pool {
        if self.security_advisory_pool_filter.is_none() {
            return pool;
        }

        self.io.debug("Running security advisory pool filter.");

        let before = microtime(true);
        let total = count(&pool.get_packages()) as f64;

        let pool = self.security_advisory_pool_filter.as_mut().unwrap().filter(
            pool,
            repositories,
            request,
        );

        let filtered = total - (count(&pool.get_packages()) as f64);

        if 0.0 == filtered {
            return pool;
        }

        self.io.write_with_verbosity(
            &sprintf(
                "Security advisory pool filter completed in %.3f seconds",
                &[(microtime(true) - before).into()],
            ),
            true,
            io_interface::VERY_VERBOSE,
        );
        self.io.write_with_verbosity(
            &sprintf(
                "<info>Found %s package versions referenced in your dependency graph. %s (%d%%) were filtered away.</info>",
                &[
                    number_format(total).into(),
                    number_format(filtered).into(),
                    round(100.0 / total * filtered).into(),
                ],
            ),
            true,
            io_interface::VERY_VERBOSE,
        );

        pool
    }
}
