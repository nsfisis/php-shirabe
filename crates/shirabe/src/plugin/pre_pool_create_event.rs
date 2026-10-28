//! ref: composer/src/Composer/Plugin/PrePoolCreateEvent.php

use crate::dependency_resolver::Request;
use crate::event_dispatcher::{Event, EventInterface};
use crate::package::BasePackageHandle;
use crate::repository::RepositoryInterfaceHandle;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct PrePoolCreateEvent {
    inner: Event,
    repositories: Vec<RepositoryInterfaceHandle>,
    request: Request,
    acceptable_stabilities: IndexMap<String, i64>,
    stability_flags: IndexMap<String, i64>,
    root_aliases: IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
    root_references: IndexMap<String, String>,
    // TODO(type-model): PoolBuilder's package array keeps the keys of the entries it unset, which
    // a listener sees in PHP; the list here is reindexed.
    packages: Vec<BasePackageHandle>,
    unacceptable_fixed_packages: Vec<BasePackageHandle>,
}

impl PrePoolCreateEvent {
    pub fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        repositories: Vec<RepositoryInterfaceHandle>,
        request: Request,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        root_aliases: IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
        root_references: IndexMap<String, String>,
        packages: Vec<BasePackageHandle>,
        unacceptable_fixed_packages: Vec<BasePackageHandle>,
    ) -> Self {
        Self {
            inner: Event::new(name, vec![], IndexMap::new()),
            repositories,
            request,
            acceptable_stabilities,
            stability_flags,
            root_aliases,
            root_references,
            packages,
            unacceptable_fixed_packages,
        }
    }

    pub fn get_repositories(&self) -> &Vec<RepositoryInterfaceHandle> {
        &self.repositories
    }

    pub fn get_request(&self) -> &Request {
        &self.request
    }

    /// Hands back the request the event was built with, together with the package lists as the
    /// listeners left them. `PoolBuilder` lends the event the `Request` it is building the pool
    /// for, which PHP shares by reference, and takes it back once the listeners have run, so
    /// whatever they changed on it reaches the solver.
    pub(crate) fn into_parts(self) -> (Request, Vec<BasePackageHandle>, Vec<BasePackageHandle>) {
        (
            self.request,
            self.packages,
            self.unacceptable_fixed_packages,
        )
    }

    pub fn get_acceptable_stabilities(&self) -> &IndexMap<String, i64> {
        &self.acceptable_stabilities
    }

    pub fn get_stability_flags(&self) -> &IndexMap<String, i64> {
        &self.stability_flags
    }

    pub fn get_root_aliases(
        &self,
    ) -> &IndexMap<String, IndexMap<String, IndexMap<String, String>>> {
        &self.root_aliases
    }

    pub fn get_root_references(&self) -> &IndexMap<String, String> {
        &self.root_references
    }

    pub fn get_packages(&self) -> &Vec<BasePackageHandle> {
        &self.packages
    }

    pub fn get_unacceptable_fixed_packages(&self) -> &Vec<BasePackageHandle> {
        &self.unacceptable_fixed_packages
    }

    pub fn set_packages(&mut self, packages: Vec<BasePackageHandle>) {
        self.packages = packages;
    }

    pub fn set_unacceptable_fixed_packages(&mut self, packages: Vec<BasePackageHandle>) {
        self.unacceptable_fixed_packages = packages;
    }
}

impl EventInterface for PrePoolCreateEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    fn get_arguments(&self) -> &Vec<String> {
        self.inner.get_arguments()
    }

    fn get_flags(&self) -> &IndexMap<String, PhpMixed> {
        self.inner.get_flags()
    }

    fn is_propagation_stopped(&self) -> bool {
        self.inner.is_propagation_stopped()
    }

    fn stop_propagation(&mut self) {
        self.inner.stop_propagation();
    }
}
