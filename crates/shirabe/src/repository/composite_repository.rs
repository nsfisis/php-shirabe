//! ref: composer/src/Composer/Repository/CompositeRepository.php

use crate::package::{BasePackageHandle, PackageInterfaceHandle};
use crate::repository::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface,
    RepositoryInterfaceHandle, SearchResult,
};
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{bconcat, implode};
use shirabe_semver::constraint::AnyConstraint;

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
    fn count(&mut self) -> anyhow::Result<usize> {
        let mut total = 0;
        for repository in &self.repositories {
            total += repository.count()?;
        }

        Ok(total)
    }

    fn get_repo_name(&self) -> anyhow::Result<BString> {
        let names: Vec<BString> = self
            .repositories
            .iter()
            .map(|r| r.get_repo_name())
            .collect::<anyhow::Result<_>>()?;
        Ok(bconcat!("composite repo (", implode(", ", &names), ")"))
    }

    fn has_package(&mut self, package: PackageInterfaceHandle) -> anyhow::Result<bool> {
        for repository in &self.repositories {
            if repository.has_package(package.clone())? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn find_package(
        &mut self,
        name: &BStr,
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
        name: &BStr,
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
        package_name_map: IndexMap<BString, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<BString, i64>,
        stability_flags: IndexMap<BString, i64>,
        already_loaded: IndexMap<BString, IndexMap<BString, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        let mut all_packages = IndexMap::new();
        let mut all_names_found = vec![];

        for repository in &self.repositories {
            let name_map_cloned: IndexMap<BString, Option<AnyConstraint>> = package_name_map
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
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

        let mut seen = indexmap::IndexSet::new();
        let unique_names: Vec<BString> = all_names_found
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
        query: BString,
        mode: i64,
        r#type: Option<BString>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let mut matches = vec![];
        for repository in &self.repositories {
            matches.extend(repository.search(query.clone(), mode, r#type.clone())?);
        }
        Ok(matches)
    }

    fn get_providers(
        &mut self,
        package_name: BString,
    ) -> anyhow::Result<IndexMap<BString, ProviderInfo>> {
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
