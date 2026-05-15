//! ref: composer/src/Composer/Plugin/PrePoolCreateEvent.php

use indexmap::IndexMap;

use crate::dependency_resolver::request::Request;
use crate::event_dispatcher::event::Event;
use crate::package::base_package::BasePackage;
use crate::repository::repository_interface::RepositoryInterface;

#[derive(Debug)]
pub struct PrePoolCreateEvent {
    inner: Event,
    repositories: Vec<Box<dyn RepositoryInterface>>,
    request: Request,
    acceptable_stabilities: IndexMap<String, i64>,
    stability_flags: IndexMap<String, i64>,
    root_aliases: IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
    root_references: IndexMap<String, String>,
    packages: Vec<BasePackage>,
    unacceptable_fixed_packages: Vec<BasePackage>,
}

impl PrePoolCreateEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        repositories: Vec<Box<dyn RepositoryInterface>>,
        request: Request,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        root_aliases: IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
        root_references: IndexMap<String, String>,
        packages: Vec<BasePackage>,
        unacceptable_fixed_packages: Vec<BasePackage>,
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

    pub fn get_repositories(&self) -> &Vec<Box<dyn RepositoryInterface>> {
        &self.repositories
    }

    pub fn get_request(&self) -> &Request {
        &self.request
    }

    pub fn get_acceptable_stabilities(&self) -> &IndexMap<String, i64> {
        &self.acceptable_stabilities
    }

    pub fn get_stability_flags(&self) -> &IndexMap<String, i64> {
        &self.stability_flags
    }

    pub fn get_root_aliases(&self) -> &IndexMap<String, IndexMap<String, IndexMap<String, String>>> {
        &self.root_aliases
    }

    pub fn get_root_references(&self) -> &IndexMap<String, String> {
        &self.root_references
    }

    pub fn get_packages(&self) -> &Vec<BasePackage> {
        &self.packages
    }

    pub fn get_unacceptable_fixed_packages(&self) -> &Vec<BasePackage> {
        &self.unacceptable_fixed_packages
    }

    pub fn set_packages(&mut self, packages: Vec<BasePackage>) {
        self.packages = packages;
    }

    pub fn set_unacceptable_fixed_packages(&mut self, packages: Vec<BasePackage>) {
        self.unacceptable_fixed_packages = packages;
    }
}
