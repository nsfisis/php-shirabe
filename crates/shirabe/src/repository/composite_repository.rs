//! ref: composer/src/Composer/Repository/CompositeRepository.php

use std::any::Any;

use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;

use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::repository::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface,
    RepositoryInterfaceHandle, SearchResult,
};

#[derive(Debug)]
pub struct CompositeRepository {
    repositories: Vec<RepositoryInterfaceHandle>,
}

impl CompositeRepository {
    pub fn new(repositories: Vec<RepositoryInterfaceHandle>) -> Self {
        let mut this = Self {
            repositories: vec![],
        };
        for repo in repositories {
            this.add_repository(repo);
        }
        this
    }

    pub fn get_repositories(&self) -> &Vec<RepositoryInterfaceHandle> {
        &self.repositories
    }

    pub fn remove_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<()> {
        for repository in &self.repositories {
            if let Some(writable) = repository
                .borrow_mut()
                .as_writable_repository_interface_mut()
            {
                writable.remove_package(package.clone())?;
            }
        }
        Ok(())
    }

    pub fn add_repository(&mut self, repository: RepositoryInterfaceHandle) {
        let nested: Option<Vec<RepositoryInterfaceHandle>> = {
            let repo_ref = repository.borrow();
            repo_ref
                .as_any()
                .downcast_ref::<CompositeRepository>()
                .map(|composite| composite.get_repositories().clone())
        };
        if let Some(nested) = nested {
            for repo in nested {
                self.repositories.push(repo);
            }
        } else {
            self.repositories.push(repository);
        }
    }
}

impl RepositoryInterface for CompositeRepository {
    fn count(&self) -> anyhow::Result<usize> {
        let mut total = 0;
        for repository in &self.repositories {
            total += repository.count()?;
        }

        Ok(total)
    }

    fn get_repo_name(&self) -> String {
        let names: Vec<String> = self
            .repositories
            .iter()
            .map(|r| r.get_repo_name())
            .collect();
        format!("composite repo ({})", names.join(", "))
    }

    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        for repository in &self.repositories {
            if repository.has_package(package.clone()) {
                return true;
            }
        }
        false
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        for repository in &self.repositories {
            let package = repository.find_package(name, constraint.clone())?;
            if package.is_some() {
                return Ok(package);
            }
        }
        Ok(None)
    }

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        let mut packages = vec![];
        for repository in &self.repositories {
            packages.extend(repository.find_packages(name, constraint.clone())?);
        }
        Ok(packages)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        let mut packages = vec![];
        for repository in &self.repositories {
            packages.extend(repository.get_packages()?);
        }
        Ok(packages)
    }

    fn load_packages(
        &mut self,
        package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        let mut all_packages = IndexMap::new();
        let mut all_names_found = vec![];

        for repository in &self.repositories {
            let name_map_cloned: IndexMap<String, Option<AnyConstraint>> = package_name_map
                .iter()
                .map(|(k, v)| (k.clone(), v.as_ref().map(|c| c.clone())))
                .collect();
            let result = repository.load_packages(
                name_map_cloned,
                acceptable_stabilities.clone(),
                stability_flags.clone(),
                already_loaded.clone(),
            )?;
            all_packages.extend(result.packages);
            all_names_found.extend(result.names_found);
        }

        let mut seen = std::collections::HashSet::new();
        let unique_names: Vec<String> = all_names_found
            .into_iter()
            .filter(|s| seen.insert(s.clone()))
            .collect();

        Ok(LoadPackagesResult {
            packages: all_packages,
            names_found: unique_names,
        })
    }

    fn search(
        &mut self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let mut matches = vec![];
        for repository in &self.repositories {
            matches.extend(repository.search(query.clone(), mode, r#type.clone())?);
        }
        Ok(matches)
    }

    fn get_providers(
        &mut self,
        package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>> {
        let mut results = IndexMap::new();
        for repository in &self.repositories {
            results.extend(repository.get_providers(package_name.clone())?);
        }
        Ok(results)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
