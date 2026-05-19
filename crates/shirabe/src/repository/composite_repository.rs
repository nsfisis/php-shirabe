//! ref: composer/src/Composer/Repository/CompositeRepository.php

use std::any::Any;

use indexmap::IndexMap;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

use crate::package::base_package::BasePackage;
use crate::package::package_interface::PackageInterface;
use crate::repository::repository_interface::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface, SearchResult,
};

#[derive(Debug)]
pub struct CompositeRepository {
    repositories: Vec<Box<dyn RepositoryInterface>>,
}

impl CompositeRepository {
    pub fn new(repositories: Vec<Box<dyn RepositoryInterface>>) -> Self {
        let mut this = Self {
            repositories: vec![],
        };
        for repo in repositories {
            this.add_repository(repo);
        }
        this
    }

    pub fn get_repositories(&self) -> &Vec<Box<dyn RepositoryInterface>> {
        &self.repositories
    }

    pub fn remove_package(&mut self, _package: &dyn PackageInterface) {
        // TODO(phase-b): only call remove_package on WritableRepositoryInterface implementors;
        // requires a downcast helper such as `as_writable() -> Option<&mut dyn WritableRepositoryInterface>` on RepositoryInterface.
        for _repository in &mut self.repositories {
            todo!()
        }
    }

    pub fn add_repository(&mut self, repository: Box<dyn RepositoryInterface>) {
        if let Some(composite) = repository.as_any().downcast_ref::<CompositeRepository>() {
            for repo in composite.get_repositories() {
                self.repositories.push(repo.clone_box());
            }
        } else {
            self.repositories.push(repository);
        }
    }
}

impl shirabe_php_shim::Countable for CompositeRepository {
    fn count(&self) -> i64 {
        self.repositories.iter().map(|r| r.count()).sum()
    }
}

impl RepositoryInterface for CompositeRepository {
    fn get_repo_name(&self) -> String {
        let names: Vec<String> = self
            .repositories
            .iter()
            .map(|r| r.get_repo_name())
            .collect();
        format!("composite repo ({})", names.join(", "))
    }

    fn has_package(&self, package: &dyn PackageInterface) -> bool {
        for repository in &self.repositories {
            if repository.has_package(package) {
                return true;
            }
        }
        false
    }

    fn find_package(
        &self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> Option<Box<dyn BasePackage>> {
        for repository in &self.repositories {
            let package = repository.find_package(name, constraint.clone());
            if package.is_some() {
                return package;
            }
        }
        None
    }

    fn find_packages(
        &self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> Vec<Box<dyn BasePackage>> {
        let mut packages = vec![];
        for repository in &self.repositories {
            packages.extend(repository.find_packages(name, constraint.clone()));
        }
        packages
    }

    fn get_packages(&self) -> Vec<Box<dyn BasePackage>> {
        let mut packages = vec![];
        for repository in &self.repositories {
            packages.extend(repository.get_packages());
        }
        packages
    }

    fn load_packages(
        &self,
        package_name_map: IndexMap<String, Option<Box<dyn ConstraintInterface>>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> LoadPackagesResult {
        let mut all_packages = vec![];
        let mut all_names_found = vec![];

        for repository in &self.repositories {
            // TODO(phase-b): manual deep clone since trait objects in maps don't derive Clone.
            let name_map_cloned: IndexMap<String, Option<Box<dyn ConstraintInterface>>> =
                package_name_map
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_ref().map(|c| c.clone_box())))
                    .collect();
            let already_loaded_cloned: IndexMap<
                String,
                IndexMap<String, Box<dyn PackageInterface>>,
            > = already_loaded
                .iter()
                .map(|(k, inner)| {
                    let inner_cloned: IndexMap<String, Box<dyn PackageInterface>> = inner
                        .iter()
                        .map(|(ik, iv)| (ik.clone(), iv.clone_package_box()))
                        .collect();
                    (k.clone(), inner_cloned)
                })
                .collect();
            let result = repository.load_packages(
                name_map_cloned,
                acceptable_stabilities.clone(),
                stability_flags.clone(),
                already_loaded_cloned,
            );
            all_packages.extend(result.packages);
            all_names_found.extend(result.names_found);
        }

        let mut seen = std::collections::HashSet::new();
        let unique_names: Vec<String> = all_names_found
            .into_iter()
            .filter(|s| seen.insert(s.clone()))
            .collect();

        LoadPackagesResult {
            packages: all_packages,
            names_found: unique_names,
        }
    }

    fn search(&self, query: String, mode: i64, r#type: Option<String>) -> Vec<SearchResult> {
        let mut matches = vec![];
        for repository in &self.repositories {
            matches.extend(repository.search(query.clone(), mode, r#type.clone()));
        }
        matches
    }

    fn get_providers(&self, package_name: String) -> IndexMap<String, ProviderInfo> {
        let mut results = IndexMap::new();
        for repository in &self.repositories {
            results.extend(repository.get_providers(package_name.clone()));
        }
        results
    }

    fn as_any(&self) -> &dyn std::any::Any {
        todo!()
    }
}
