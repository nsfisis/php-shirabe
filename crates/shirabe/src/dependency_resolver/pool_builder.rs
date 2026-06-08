//! ref: composer/src/Composer/DependencyResolver/PoolBuilder.php

use crate::io::io_interface;
use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::composer::semver::CompilingMatcher;
use shirabe_external_packages::composer::semver::Intervals;
use shirabe_php_shim::{
    LogicException, PhpMixed, array_flip, array_flip_strings, array_map, array_merge, array_search,
    array_search_mixed, count, in_array, microtime, number_format, round, sprintf, strpos,
};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::MatchAllConstraint;
use shirabe_semver::constraint::MultiConstraint;
use shirabe_semver::constraint::SimpleConstraint;

use crate::dependency_resolver::Pool;
use crate::dependency_resolver::PoolOptimizer;
use crate::dependency_resolver::Request;
use crate::dependency_resolver::SecurityAdvisoryPoolFilter;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::AliasPackageHandle;
use crate::package::BasePackageHandle;
use crate::package::CompleteAliasPackageHandle;
use crate::package::CompletePackage;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::package::base_package;
use crate::package::version::StabilityFilter;
use crate::plugin::PluginEvents;
use crate::plugin::PrePoolCreateEvent;
use crate::repository::CanonicalPackagesTrait;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterface;
use crate::repository::RepositoryInterfaceHandle;
use crate::repository::RootPackageRepository;

#[derive(Debug)]
pub struct PoolBuilder {
    acceptable_stabilities: IndexMap<String, i64>,
    stability_flags: IndexMap<String, i64>,
    root_aliases: IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
    root_references: IndexMap<String, String>,
    temporary_constraints: IndexMap<String, AnyConstraint>,
    event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
    pool_optimizer: Option<PoolOptimizer>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    alias_map: IndexMap<String, IndexMap<i64, AliasPackageHandle>>,
    packages_to_load: IndexMap<String, AnyConstraint>,
    loaded_packages: IndexMap<String, AnyConstraint>,
    loaded_per_repo: IndexMap<i64, IndexMap<String, IndexMap<String, PackageInterfaceHandle>>>,
    packages: IndexMap<i64, BasePackageHandle>,
    unacceptable_fixed_or_locked_packages: Vec<BasePackageHandle>,
    update_allow_list: Vec<String>,
    skipped_load: IndexMap<String, Vec<PackageInterfaceHandle>>,
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
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        pool_optimizer: Option<PoolOptimizer>,
        temporary_constraints: IndexMap<String, AnyConstraint>,
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
        repositories: Vec<RepositoryInterfaceHandle>,
        request: &mut Request,
    ) -> anyhow::Result<Pool> {
        self.restricted_packages_list = if request.get_restricted_packages().is_some() {
            Some(
                array_flip_strings(request.get_restricted_packages().unwrap())
                    .into_iter()
                    .map(|(k, v)| (k, v.as_int().unwrap_or(0)))
                    .collect(),
            )
        } else {
            None
        };

        if request.get_update_allow_list().len() > 0 {
            self.update_allow_list = request.get_update_allow_list().clone();
            self.warn_about_non_matching_update_allow_list(request)?;

            if request.get_locked_repository().is_none() {
                return Err(LogicException {
                    message: "No lock repo present and yet a partial update was requested."
                        .to_string(),
                    code: 0,
                }
                .into());
            }

            for locked_package in request
                .get_locked_repository()
                .unwrap()
                .borrow_mut()
                .get_canonical_packages()?
            {
                if !self.is_update_allowed(locked_package.clone()) {
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

                    request.lock_package(locked_package.into());
                }
            }
        }

        for (_, package) in request.get_fixed_or_locked_packages() {
            // using MatchAllConstraint here because fixed packages do not need to retrigger
            // loading any packages
            self.loaded_packages.insert(
                package.get_name().to_string(),
                MatchAllConstraint::new(None).into(),
            );

            // replace means conflict, so if a fixed package replaces a name, no need to load that one, packages would conflict anyways
            for (_k, link) in &package.get_replaces() {
                self.loaded_packages.insert(
                    link.get_target().to_string(),
                    MatchAllConstraint::new(None).into(),
                );
            }

            // TODO in how far can we do the above for conflicts? It's more tricky cause conflicts can be limited to
            // specific versions while replace is a conflict with all versions of the name

            let in_root_or_platform = package.get_repository().map_or(false, |r| {
                r.is::<RootPackageRepository>() || r.is::<PlatformRepository>()
            });
            if in_root_or_platform
                || StabilityFilter::is_package_acceptable(
                    &self.acceptable_stabilities,
                    &self.stability_flags,
                    &package.get_names(true),
                    &package.get_stability(),
                )
            {
                self.load_package(request, &repositories, package.clone(), false)?;
            } else {
                self.unacceptable_fixed_or_locked_packages.push(package);
            }
        }

        for (package_name, constraint) in request.get_requires() {
            // fixed and locked packages have already been added, so if a root require needs one of them, no need to do anything
            if self.loaded_packages.contains_key(package_name) {
                continue;
            }

            self.packages_to_load
                .insert(package_name.clone(), constraint.clone());
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

        while self.packages_to_load.len() > 0 {
            self.load_packages_marked_for_loading(request, &repositories)?;
        }

        if self.temporary_constraints.len() > 0 {
            let indices: Vec<i64> = self.packages.keys().cloned().collect();
            for i in indices {
                let package = match self.packages.get(&i) {
                    Some(p) => p.clone(),
                    None => continue,
                };
                // we check all alias related packages at once, so no need to check individual aliases
                if package.as_alias().is_some() {
                    continue;
                }

                for package_name in package.get_names(true) {
                    let constraint = match self.temporary_constraints.get(&package_name) {
                        Some(c) => c.clone(),
                        None => continue,
                    };

                    let mut package_and_aliases: Vec<(i64, BasePackageHandle)> = Vec::new();
                    package_and_aliases.push((i, package.clone()));
                    if let Some(aliases) = self.alias_map.get(&package.ptr_id().to_string()) {
                        for (idx, alias) in aliases {
                            package_and_aliases.push((*idx, alias.clone().into()));
                        }
                    }

                    let mut found = false;
                    for (_idx, package_or_alias) in &package_and_aliases {
                        if CompilingMatcher::matches(
                            &constraint,
                            SimpleConstraint::OP_EQ,
                            &package_or_alias.get_version(),
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
            // TODO(phase-b): PrePoolCreateEvent::new takes Request and Vec<Box<dyn RepositoryInterface>>
            // by value but neither can be cloned (PHP class shared semantics). Needs Rc-based migration.
            let mut pre_pool_create_event = PrePoolCreateEvent::new(
                PluginEvents::PRE_POOL_CREATE.to_string(),
                todo!("share repositories with PrePoolCreateEvent without moving"),
                todo!("share Request with PrePoolCreateEvent without moving"),
                self.acceptable_stabilities.clone(),
                self.stability_flags.clone(),
                self.root_aliases.clone(),
                self.root_references.clone(),
                self.packages.values().cloned().collect(),
                self.unacceptable_fixed_or_locked_packages
                    .iter()
                    .cloned()
                    .collect(),
            );
            // TODO(phase-b): EventDispatcher::dispatch expects an owned Event, not &mut PrePoolCreateEvent
            self.event_dispatcher
                .as_ref()
                .unwrap()
                .borrow_mut()
                .dispatch(Some(pre_pool_create_event.get_name()), None)?;
            // PHP rebinds $this->packages to a list-style array; preserve indices via reindexing.
            // TODO(plugin)/TODO(phase-c): rebind self.packages from the (handle-based) event packages
            // once EventDispatcher::dispatch returns the mutated event.
            let _ = &pre_pool_create_event;
        }

        let mut pool = Pool::new(
            self.packages.values().cloned().collect(),
            self.unacceptable_fixed_or_locked_packages
                .iter()
                .cloned()
                .collect(),
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
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

        self.io.debug("Built pool.", &[]);

        // filter vulnerable packages before optimizing the pool otherwise we may end up with inconsistent state where the optimizer took away versions
        // that were not vulnerable and now suddenly the vulnerable ones are removed and we are missing some versions to make it solvable
        pool = self.run_security_advisory_filter(pool, &repositories, request)?;
        pool = self.run_optimizer(request, pool);

        Intervals::clear();

        Ok(pool)
    }

    fn mark_package_name_for_loading(
        &mut self,
        request: &Request,
        name: &str,
        constraint: &AnyConstraint,
    ) {
        let constraint = constraint.clone();
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
            if !Intervals::is_subset_of(&constraint, root_constraint).unwrap_or(false) {
                constraint = root_constraint.clone();
            }
        }

        // Not yet loaded or already marked for a reload, set the constraint to be loaded
        if !self.loaded_packages.contains_key(name) {
            // Maybe it was already marked before but not loaded yet. In that case
            // we have to extend the constraint (we don't check if they are identical because
            // MultiConstraint::create() will optimize anyway)
            if let Some(existing) = self.packages_to_load.get(name) {
                // Already marked for loading and this does not expand the constraint to be loaded, nothing to do
                if Intervals::is_subset_of(&constraint, existing).unwrap_or(false) {
                    return;
                }

                // extend the constraint to be loaded
                constraint = Intervals::compact_constraint(
                    MultiConstraint::create(
                        vec![existing.clone(), constraint.clone()],
                        false,
                        None,
                    )
                    .unwrap_or_else(|_| MatchAllConstraint::new(None).into()),
                );
            }

            self.packages_to_load.insert(name.to_string(), constraint);

            return;
        }

        // No need to load this package with this constraint because it is
        // a subset of the constraint with which we have already loaded packages
        if Intervals::is_subset_of(&constraint, self.loaded_packages.get(name).unwrap())
            .unwrap_or(false)
        {
            return;
        }

        // We have already loaded that package but not in the constraint that's
        // required. We extend the constraint and mark that package as not being loaded
        // yet so we get the required package versions
        self.packages_to_load.insert(
            name.to_string(),
            Intervals::compact_constraint(
                MultiConstraint::create(
                    vec![self.loaded_packages.get(name).unwrap().clone(), constraint],
                    false,
                    None,
                )
                .unwrap_or_else(|_| MatchAllConstraint::new(None).into()),
            ),
        );
        self.loaded_packages.shift_remove(name);
    }

    fn load_packages_marked_for_loading(
        &mut self,
        request: &mut Request,
        repositories: &Vec<RepositoryInterfaceHandle>,
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
        let snapshot: Vec<(String, AnyConstraint)> = self
            .packages_to_load
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (name, constraint) in &snapshot {
            self.loaded_packages
                .insert(name.clone(), constraint.clone());
        }

        // Load packages in chunks of 50 to prevent memory usage build-up due to caches of all sorts
        let mut package_batches: Vec<IndexMap<String, AnyConstraint>> = {
            let mut chunks: Vec<IndexMap<String, AnyConstraint>> = Vec::new();
            let mut current: IndexMap<String, AnyConstraint> = IndexMap::new();
            for (k, v) in self.packages_to_load.iter() {
                current.insert(k.clone(), v.clone());
                if current.len() as i64 >= Self::LOAD_BATCH_SIZE {
                    chunks.push(std::mem::take(&mut current));
                }
            }
            if !current.is_empty() {
                chunks.push(current);
            }
            chunks
        };
        self.packages_to_load = IndexMap::new();

        for (repo_index, repository) in repositories.iter().enumerate() {
            // these repos have their packages fixed or locked if they need to be loaded so we
            // never need to load anything else from them
            let is_locked_repo = request
                .get_locked_repository()
                .map_or(false, |h| repository.ptr_eq(&h.into()));
            if repository.is::<PlatformRepository>() || is_locked_repo {
                continue;
            }

            if 0 == package_batches.len() {
                break;
            }

            // Iterate by index because we mutate package_batches inside the loop.
            for batch_index in 0..package_batches.len() {
                let package_batch: IndexMap<String, Option<AnyConstraint>> = package_batches
                    [batch_index]
                    .iter()
                    .map(|(k, v)| (k.clone(), Some(v.clone())))
                    .collect();
                let result = repository.load_packages(
                    package_batch,
                    self.acceptable_stabilities.clone(),
                    self.stability_flags.clone(),
                    self.loaded_per_repo
                        .get(&(repo_index as i64))
                        .map(|m| {
                            m.iter()
                                .map(|(k, inner)| {
                                    (
                                        k.clone(),
                                        inner
                                            .iter()
                                            .map(|(kk, vv)| (kk.clone(), vv.clone()))
                                            .collect(),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                )?;

                let names_found = result.names_found;
                for name in &names_found {
                    // avoid loading the same package again from other repositories once it has been found
                    if let Some(b) = package_batches.get_mut(batch_index) {
                        b.shift_remove(name);
                    }
                }
                let packages_in_result = result.packages;
                for (_, package) in &packages_in_result {
                    let pkg_name = package.get_name().to_string();
                    let pkg_version = package.get_version().to_string();
                    let pkg_type = package.get_type().to_string();

                    let pkg_type_mixed: PhpMixed = pkg_type.clone().into();
                    let ignored_mixed: PhpMixed = self
                        .ignored_types
                        .iter()
                        .cloned()
                        .map(PhpMixed::from)
                        .collect::<Vec<_>>()
                        .into();
                    if in_array(pkg_type_mixed.clone(), &ignored_mixed, true)
                        || (self.allowed_types.is_some() && {
                            let allowed_mixed: PhpMixed = self
                                .allowed_types
                                .as_ref()
                                .unwrap()
                                .iter()
                                .cloned()
                                .map(PhpMixed::from)
                                .collect::<Vec<_>>()
                                .into();
                            !in_array(pkg_type_mixed.clone(), &allowed_mixed, true)
                        })
                    {
                        continue;
                    }
                    let _ = (pkg_name, pkg_version);
                    let propagate = !self.path_repo_unlocked.contains_key(&package.get_name());
                    self.load_package(request, repositories, package.clone(), propagate)?;
                }
            }

            // PHP: array_chunk(array_merge(...$packageBatches), self::LOAD_BATCH_SIZE, true)
            let mut merged: IndexMap<String, AnyConstraint> = IndexMap::new();
            for batch in &package_batches {
                for (k, v) in batch {
                    merged.insert(k.clone(), v.clone());
                }
            }
            // Rebuild chunks from merged.
            package_batches = {
                let mut chunks: Vec<IndexMap<String, AnyConstraint>> = Vec::new();
                let mut current: IndexMap<String, AnyConstraint> = IndexMap::new();
                for (k, v) in merged.iter() {
                    current.insert(k.clone(), v.clone());
                    if current.len() as i64 >= Self::LOAD_BATCH_SIZE {
                        chunks.push(std::mem::take(&mut current));
                    }
                }
                if !current.is_empty() {
                    chunks.push(current);
                }
                chunks
            };
        }
        Ok(())
    }

    fn load_package(
        &mut self,
        request: &mut Request,
        repositories: &Vec<RepositoryInterfaceHandle>,
        package: BasePackageHandle,
        propagate_update: bool,
    ) -> anyhow::Result<()> {
        let index = self.index_counter;
        self.index_counter += 1;
        self.packages.insert(index, package.clone());

        if let Some(alias) = package.as_alias() {
            self.alias_map
                .entry(alias.get_alias_of().ptr_id().to_string())
                .or_insert_with(IndexMap::new)
                .insert(index, alias);
        }

        let name = package.get_name();

        // we're simply setting the root references on all versions for a name here and rely on the solver to pick the
        // right version. It'd be more work to figure out which versions and which aliases of those versions this may
        // apply to
        if let Some(reference) = self.root_references.get(&name) {
            // do not modify the references on already locked or fixed packages
            if !request.is_locked_package(package.clone())
                && !request.is_fixed_package(package.clone())
            {
                package.set_source_dist_references(reference);
            }
        }

        // if propagateUpdate is false we are loading a fixed or locked package, root aliases do not apply as they are
        // manually loaded as separate packages in this case
        //
        // packages in pathRepoUnlocked however need to also load root aliases, they have propagateUpdate set to
        // false because their deps should not be unlocked, but that is irrelevant for root aliases
        let path_repo_match = self.path_repo_unlocked.contains_key(&package.get_name());
        let alias_for_version = self
            .root_aliases
            .get(&name)
            .and_then(|m| m.get(&package.get_version()))
            .cloned();
        if (propagate_update || path_repo_match) && alias_for_version.is_some() {
            let alias = alias_for_version.unwrap();
            let base_package: BasePackageHandle = if let Some(ap) = package.as_alias() {
                ap.get_alias_of().into()
            } else {
                package.clone()
            };
            let alias_normalized = alias.get("alias_normalized").cloned().unwrap_or_default();
            let alias_pretty = alias.get("alias").cloned().unwrap_or_default();
            let alias_handle: AliasPackageHandle =
                if let Some(complete) = base_package.as_complete_package() {
                    CompleteAliasPackageHandle::new(complete, alias_normalized, alias_pretty).into()
                } else {
                    let real = base_package
                        .as_package()
                        .expect("non-alias base package must be a real Package");
                    AliasPackageHandle::new(real, alias_normalized, alias_pretty)
                };
            alias_handle.set_root_package_alias(true);

            let new_index = self.index_counter;
            self.index_counter += 1;
            self.packages.insert(new_index, alias_handle.clone().into());
            self.alias_map
                .entry(alias_handle.get_alias_of().ptr_id().to_string())
                .or_insert_with(IndexMap::new)
                .insert(new_index, alias_handle);
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
                        || 0 == skipped_root_requires.len()
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
                        || 0 == skipped_root_requires.len()
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
                |package: &PackageInterfaceHandle| -> String {
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
            if root_requires.contains_key(&package_or_replacer.get_name()) {
                matches.push(package_or_replacer.get_name());
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
    fn is_update_allowed(&self, package: PackageInterfaceHandle) -> bool {
        for pattern in &self.update_allow_list {
            let pattern_regexp = base_package::package_name_to_regexp(pattern);
            if Preg::is_match3(&pattern_regexp, &package.get_name(), None).unwrap_or(false) {
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

            let pattern_regexp = base_package::package_name_to_regexp(pattern);
            // update pattern matches a locked package? => all good
            for package in request
                .get_locked_repository()
                .unwrap()
                .borrow_mut()
                .get_canonical_packages()?
            {
                if Preg::is_match3(&pattern_regexp, &package.get_name(), None).unwrap_or(false) {
                    continue 'outer;
                }
            }
            // update pattern matches a root require? => all good, probably a new package
            for (package_name, _constraint) in request.get_requires() {
                if Preg::is_match3(&pattern_regexp, package_name, None).unwrap_or(false) {
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
        repositories: &Vec<RepositoryInterfaceHandle>,
        name: &str,
    ) -> anyhow::Result<()> {
        let skipped: Vec<PackageInterfaceHandle> = self
            .skipped_load
            .get(name)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default();
        for package_or_replacer in &skipped {
            // if we unfixed a replaced package name, we also need to unfix the replacer itself
            // as long as it was not unfixed yet
            if package_or_replacer.get_name() != name
                && self
                    .skipped_load
                    .contains_key(&package_or_replacer.get_name())
            {
                let replacer_name = package_or_replacer.get_name();
                if request.get_update_allow_transitive_root_dependencies()
                    || (!self.is_root_require(request, name)
                        && !self.is_root_require(request, &replacer_name))
                {
                    self.unlock_package(request, repositories, &replacer_name)?;

                    if self.is_root_require(request, &replacer_name) {
                        self.mark_package_name_for_loading(
                            request,
                            &replacer_name,
                            &MatchAllConstraint::new(None).into(),
                        );
                    } else {
                        let pkgs: Vec<BasePackageHandle> =
                            self.packages.values().cloned().collect();
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
            let entries: Vec<(i64, BasePackageHandle)> = self
                .packages
                .iter()
                .filter(|(_, p)| p.get_name() == name)
                .map(|(i, p)| (*i, p.clone()))
                .collect();
            for (index, package) in &entries {
                self.remove_loaded_package(request, repositories, package.clone(), *index);
            }
        }

        self.skipped_load.shift_remove(name);
        self.loaded_packages.shift_remove(name);
        self.max_extended_reqs.shift_remove(name);
        self.path_repo_unlocked.shift_remove(name);

        // remove locked package by this name which was already initialized
        let locked_packages: Vec<BasePackageHandle> =
            request.get_locked_packages().values().cloned().collect();
        for locked_package in &locked_packages {
            if locked_package.as_alias().is_none() && locked_package.get_name() == name {
                let pkgs: Vec<BasePackageHandle> = self.packages.values().cloned().collect();
                // PHP uses array_search with strict identity; map to pointer comparison.
                let index_opt = pkgs.iter().position(|p| p.ptr_eq(locked_package));
                if let Some(index) = index_opt {
                    request.unlock_package(locked_package.clone());
                    self.remove_loaded_package(
                        request,
                        repositories,
                        locked_package.clone(),
                        index as i64,
                    );

                    // make sure that any requirements for this package by other locked or fixed packages are now
                    // also loaded, as they were previously ignored because the locked (now unlocked) package already
                    // satisfied their requirements
                    // and if this package is replacing another that is required by a locked or fixed package, ensure
                    // that we load that replaced package in case an update to this package removes the replacement
                    let fixed_or_locked: Vec<BasePackageHandle> = request
                        .get_fixed_or_locked_packages()
                        .values()
                        .cloned()
                        .collect();
                    for fixed_or_locked_package in &fixed_or_locked {
                        if fixed_or_locked_package.ptr_eq(locked_package) {
                            continue;
                        }

                        if self
                            .skipped_load
                            .contains_key(&fixed_or_locked_package.get_name())
                        {
                            let requires = fixed_or_locked_package.get_requires();
                            if let Some(req_link) = requires.get(&locked_package.get_name()) {
                                self.mark_package_name_for_loading(
                                    request,
                                    &locked_package.get_name(),
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
            let cons = request.get_requires()[name].clone();
            self.mark_package_name_for_loading(request, name, &cons);
        }

        let pkgs: Vec<BasePackageHandle> = self.packages.values().cloned().collect();
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
        repositories: &Vec<RepositoryInterfaceHandle>,
        package: BasePackageHandle,
        index: i64,
    ) {
        let repo_index: i64 = package
            .get_repository()
            .and_then(|pkg_repo| {
                repositories
                    .iter()
                    .position(|r| r.ptr_eq(&pkg_repo))
                    .map(|i| i as i64)
            })
            .unwrap_or(-1);

        if repo_index >= 0 {
            if let Some(repo_map) = self.loaded_per_repo.get_mut(&repo_index) {
                if let Some(name_map) = repo_map.get_mut(&package.get_name()) {
                    name_map.shift_remove(&package.get_version());
                }
            }
        }
        self.packages.shift_remove(&index);
        let object_hash = package.ptr_id().to_string();
        if let Some(aliases) = self.alias_map.shift_remove(&object_hash) {
            for (alias_index, alias_package) in &aliases {
                if repo_index >= 0 {
                    if let Some(repo_map) = self.loaded_per_repo.get_mut(&repo_index) {
                        if let Some(name_map) = repo_map.get_mut(&alias_package.get_name()) {
                            name_map.shift_remove(&alias_package.get_version());
                        }
                    }
                }
                self.packages.shift_remove(alias_index);
            }
        }
    }

    fn run_optimizer(&mut self, request: &Request, pool: Pool) -> Pool {
        if self.pool_optimizer.is_none() {
            return pool;
        }

        self.io.debug("Running pool optimizer.", &[]);

        let before = microtime(true);
        let total = pool.get_packages().len() as f64;

        let pool = self
            .pool_optimizer
            .as_mut()
            .unwrap()
            .optimize(request, &pool);

        let filtered = total - (pool.get_packages().len() as f64);

        if 0.0 == filtered {
            return pool;
        }

        self.io.write3(
            &sprintf(
                "Pool optimizer completed in %.3f seconds",
                &[(microtime(true) - before).into()],
            ),
            true,
            io_interface::VERY_VERBOSE,
        );
        self.io.write3(
            &sprintf(
                "<info>Found %s package versions referenced in your dependency graph. %s (%d%%) were optimized away.</info>",
                &[
                    number_format(total, 0, ".", ",").into(),
                    number_format(filtered, 0, ".", ",").into(),
                    round(100.0 / total * filtered, 0).into(),
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
        repositories: &Vec<RepositoryInterfaceHandle>,
        request: &Request,
    ) -> anyhow::Result<Pool> {
        if self.security_advisory_pool_filter.is_none() {
            return Ok(pool);
        }

        self.io.debug("Running security advisory pool filter.", &[]);

        let before = microtime(true);
        let total = pool.get_packages().len() as f64;

        let repos_owned: Vec<RepositoryInterfaceHandle> = repositories.iter().cloned().collect();
        let pool = self
            .security_advisory_pool_filter
            .as_mut()
            .unwrap()
            .filter(pool, repos_owned, request)?;

        let filtered = total - (pool.get_packages().len() as f64);

        if 0.0 == filtered {
            return Ok(pool);
        }

        self.io.write3(
            &sprintf(
                "Security advisory pool filter completed in %.3f seconds",
                &[(microtime(true) - before).into()],
            ),
            true,
            io_interface::VERY_VERBOSE,
        );
        self.io.write3(
            &sprintf(
                "<info>Found %s package versions referenced in your dependency graph. %s (%d%%) were filtered away.</info>",
                &[
                    number_format(total, 0, ".", ",").into(),
                    number_format(filtered, 0, ".", ",").into(),
                    round(100.0 / total * filtered, 0).into(),
                ],
            ),
            true,
            io_interface::VERY_VERBOSE,
        );

        Ok(pool)
    }
}
