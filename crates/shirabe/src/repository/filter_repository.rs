//! ref: composer/src/Composer/Repository/FilterRepository.php

use crate::package::BasePackageHandle;
use crate::package::PackageInterfaceHandle;
use crate::package::base_package::{self};
use crate::repository::{AdvisoryProviderInterface, SecurityAdvisoryResult};
use crate::repository::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface,
    RepositoryInterfaceHandle, SearchResult,
};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed};
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug)]
pub struct FilterRepository {
    only: Option<String>,
    exclude: Option<String>,
    canonical: bool,
    repo: RepositoryInterfaceHandle,
}

impl FilterRepository {
    pub fn new(
        repo: RepositoryInterfaceHandle,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Self> {
        let mut only: Option<String> = None;
        let mut exclude: Option<String> = None;
        let mut canonical = true;

        if let Some(only_val) = options.get("only") {
            match only_val {
                PhpMixed::List(list) => {
                    let names: Vec<String> = list
                        .iter()
                        .filter_map(|v| {
                            if let PhpMixed::String(s) = v {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    only = Some(base_package::package_names_to_regexp(
                        &names,
                        "{^(?:%s)$}iD",
                    ));
                }
                _ => {
                    return Err(InvalidArgumentException {
                        message: format!(
                            r#""only" key for repository {} should be an array"#,
                            repo.get_repo_name()
                        ),
                        code: 0,
                    }
                    .into());
                }
            }
        }
        if let Some(exclude_val) = options.get("exclude") {
            match exclude_val {
                PhpMixed::List(list) => {
                    let names: Vec<String> = list
                        .iter()
                        .filter_map(|v| {
                            if let PhpMixed::String(s) = v {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    exclude = Some(base_package::package_names_to_regexp(
                        &names,
                        "{^(?:%s)$}iD",
                    ));
                }
                _ => {
                    return Err(InvalidArgumentException {
                        message: format!(
                            r#""exclude" key for repository {} should be an array"#,
                            repo.get_repo_name()
                        ),
                        code: 0,
                    }
                    .into());
                }
            }
        }
        if exclude.is_some() && only.is_some() {
            return Err(InvalidArgumentException {
                message: format!(
                    r#"Only one of "only" and "exclude" can be specified for repository {}"#,
                    repo.get_repo_name()
                ),
                code: 0,
            }
            .into());
        }
        if let Some(canonical_val) = options.get("canonical") {
            match canonical_val {
                PhpMixed::Bool(b) => {
                    canonical = *b;
                }
                _ => {
                    return Err(InvalidArgumentException {
                        message: format!(
                            r#""canonical" key for repository {} should be a boolean"#,
                            repo.get_repo_name()
                        ),
                        code: 0,
                    }
                    .into());
                }
            }
        }

        Ok(Self {
            only,
            exclude,
            canonical,
            repo,
        })
    }

    pub fn get_repository(&self) -> RepositoryInterfaceHandle {
        self.repo.clone()
    }

    fn is_allowed(&self, name: &str) -> bool {
        if self.only.is_none() && self.exclude.is_none() {
            return true;
        }

        if let Some(only) = &self.only {
            return Preg::is_match(only, name);
        }

        if self.exclude.is_none() {
            return true;
        }

        !Preg::is_match(self.exclude.as_ref().unwrap(), name)
    }
}

impl RepositoryInterface for FilterRepository {
    fn count(&self) -> anyhow::Result<usize> {
        if self.repo.count()? > 0 {
            Ok(self
                .repo
                .get_packages()?
                .iter()
                .filter(|p| self.is_allowed(&p.get_name()))
                .count())
        } else {
            Ok(0)
        }
    }

    fn has_package(&self, package: PackageInterfaceHandle) -> bool {
        self.repo.has_package(package)
    }

    fn find_package(
        &mut self,
        name: &str,
        constraint: FindPackageConstraint,
    ) -> anyhow::Result<Option<BasePackageHandle>> {
        if !self.is_allowed(name) {
            return Ok(None);
        }

        self.repo.find_package(name, constraint)
    }

    fn find_packages(
        &mut self,
        name: &str,
        constraint: Option<FindPackageConstraint>,
    ) -> anyhow::Result<Vec<BasePackageHandle>> {
        if !self.is_allowed(name) {
            return Ok(Vec::new());
        }

        self.repo.find_packages(name, constraint)
    }

    fn load_packages(
        &mut self,
        mut package_name_map: IndexMap<String, Option<AnyConstraint>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, PackageInterfaceHandle>>,
    ) -> anyhow::Result<LoadPackagesResult> {
        package_name_map.retain(|name, _| self.is_allowed(name));

        if package_name_map.is_empty() {
            return Ok(LoadPackagesResult {
                names_found: Vec::new(),
                packages: IndexMap::new(),
            });
        }

        let mut result = self.repo.load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        )?;
        if !self.canonical {
            result.names_found = Vec::new();
        }

        Ok(result)
    }

    fn search(
        &mut self,
        query: String,
        mode: i64,
        r#type: Option<String>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        let mut result = Vec::new();

        for package in self.repo.search(query, mode, r#type)? {
            if self.is_allowed(&package.name) {
                result.push(package);
            }
        }

        Ok(result)
    }

    fn get_packages(&mut self) -> anyhow::Result<Vec<BasePackageHandle>> {
        let mut result = Vec::new();
        for package in self.repo.get_packages()? {
            if self.is_allowed(&package.get_name()) {
                result.push(package);
            }
        }

        Ok(result)
    }

    fn get_providers(
        &mut self,
        package_name: String,
    ) -> anyhow::Result<IndexMap<String, ProviderInfo>> {
        let mut result = IndexMap::new();
        for (name, provider) in self.repo.get_providers(package_name)? {
            if self.is_allowed(&provider.name) {
                result.insert(name, provider);
            }
        }

        Ok(result)
    }

    fn get_repo_name(&self) -> String {
        self.repo.get_repo_name()
    }

    fn as_advisory_provider(&self) -> Option<&dyn AdvisoryProviderInterface> {
        // FilterRepository is itself an advisory provider (it filters), but only meaningfully so
        // when the wrapped repository provides advisories.
        if self.repo.borrow().as_advisory_provider().is_some() {
            Some(self)
        } else {
            None
        }
    }

    fn as_advisory_provider_mut(&mut self) -> Option<&mut dyn AdvisoryProviderInterface> {
        if self.repo.borrow().as_advisory_provider().is_some() {
            Some(self)
        } else {
            None
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl AdvisoryProviderInterface for FilterRepository {
    fn has_security_advisories(&mut self) -> anyhow::Result<bool> {
        let mut repo = self.repo.borrow_mut();
        if let Some(advisory_repo) = repo.as_advisory_provider_mut() {
            advisory_repo.has_security_advisories()
        } else {
            Ok(false)
        }
    }

    fn get_security_advisories(
        &mut self,
        mut package_constraint_map: IndexMap<String, AnyConstraint>,
        allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult> {
        package_constraint_map.retain(|name, _| self.is_allowed(name));
        let mut repo = self.repo.borrow_mut();
        if let Some(advisory_repo) = repo.as_advisory_provider_mut() {
            advisory_repo.get_security_advisories(package_constraint_map, allow_partial_advisories)
        } else {
            Ok(SecurityAdvisoryResult {
                names_found: Vec::new(),
                advisories: IndexMap::new(),
            })
        }
    }
}
