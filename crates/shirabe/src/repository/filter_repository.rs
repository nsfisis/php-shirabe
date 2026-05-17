//! ref: composer/src/Composer/Repository/FilterRepository.php

use crate::package::base_package::BasePackage;
use crate::package::package_interface::PackageInterface;
use crate::repository::advisory_provider_interface::{
    AdvisoryProviderInterface, SecurityAdvisoryResult,
};
use crate::repository::repository_interface::{
    FindPackageConstraint, LoadPackagesResult, ProviderInfo, RepositoryInterface, SearchResult,
};
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed};
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

#[derive(Debug)]
pub struct FilterRepository {
    only: Option<String>,
    exclude: Option<String>,
    canonical: bool,
    repo: Box<dyn RepositoryInterface>,
}

impl FilterRepository {
    pub fn new(
        repo: Box<dyn RepositoryInterface>,
        options: IndexMap<String, PhpMixed>,
    ) -> Result<Self> {
        let mut only: Option<String> = None;
        let mut exclude: Option<String> = None;
        let mut canonical = true;

        if let Some(only_val) = options.get("only") {
            match only_val {
                PhpMixed::List(list) => {
                    let names: Vec<String> = list
                        .iter()
                        .filter_map(|v| {
                            if let PhpMixed::String(s) = v.as_ref() {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    only = Some(BasePackage::package_names_to_regexp(&names));
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
                            if let PhpMixed::String(s) = v.as_ref() {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    exclude = Some(BasePackage::package_names_to_regexp(&names));
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

    pub fn get_repository(&self) -> &dyn RepositoryInterface {
        self.repo.as_ref()
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

impl shirabe_php_shim::Countable for FilterRepository {
    fn count(&self) -> i64 {
        if self.repo.count() > 0 {
            self.get_packages().len() as i64
        } else {
            0
        }
    }
}

impl RepositoryInterface for FilterRepository {
    fn has_package(&self, package: &dyn PackageInterface) -> bool {
        self.repo.has_package(package)
    }

    fn find_package(
        &self,
        name: String,
        constraint: FindPackageConstraint,
    ) -> Option<Box<dyn BasePackage>> {
        if !self.is_allowed(&name) {
            return None;
        }

        self.repo.find_package(name, constraint)
    }

    fn find_packages(
        &self,
        name: String,
        constraint: Option<FindPackageConstraint>,
    ) -> Vec<Box<dyn BasePackage>> {
        if !self.is_allowed(&name) {
            return Vec::new();
        }

        self.repo.find_packages(name, constraint)
    }

    fn load_packages(
        &self,
        mut package_name_map: IndexMap<String, Option<Box<dyn ConstraintInterface>>>,
        acceptable_stabilities: IndexMap<String, i64>,
        stability_flags: IndexMap<String, i64>,
        already_loaded: IndexMap<String, IndexMap<String, Box<dyn PackageInterface>>>,
    ) -> LoadPackagesResult {
        package_name_map.retain(|name, _| self.is_allowed(name));

        if package_name_map.is_empty() {
            return LoadPackagesResult {
                names_found: Vec::new(),
                packages: Vec::new(),
            };
        }

        let mut result = self.repo.load_packages(
            package_name_map,
            acceptable_stabilities,
            stability_flags,
            already_loaded,
        );
        if !self.canonical {
            result.names_found = Vec::new();
        }

        result
    }

    fn search(&self, query: String, mode: i64, r#type: Option<String>) -> Vec<SearchResult> {
        let mut result = Vec::new();

        for package in self.repo.search(query, mode, r#type) {
            if self.is_allowed(&package.name) {
                result.push(package);
            }
        }

        result
    }

    fn get_packages(&self) -> Vec<Box<dyn BasePackage>> {
        let mut result = Vec::new();
        for package in self.repo.get_packages() {
            if self.is_allowed(package.get_name()) {
                result.push(package);
            }
        }

        result
    }

    fn get_providers(&self, package_name: String) -> IndexMap<String, ProviderInfo> {
        let mut result = IndexMap::new();
        for (name, provider) in self.repo.get_providers(package_name) {
            if self.is_allowed(&provider.name) {
                result.insert(name, provider);
            }
        }

        result
    }

    fn get_repo_name(&self) -> String {
        self.repo.get_repo_name()
    }

    fn as_advisory_provider(&self) -> Option<&dyn AdvisoryProviderInterface> {
        self.repo.as_advisory_provider()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl AdvisoryProviderInterface for FilterRepository {
    fn has_security_advisories(&self) -> bool {
        if let Some(advisory_repo) = self.repo.as_advisory_provider() {
            advisory_repo.has_security_advisories()
        } else {
            false
        }
    }

    fn get_security_advisories(
        &self,
        mut package_constraint_map: IndexMap<String, Box<dyn ConstraintInterface>>,
        allow_partial_advisories: bool,
    ) -> anyhow::Result<SecurityAdvisoryResult> {
        if let Some(advisory_repo) = self.repo.as_advisory_provider() {
            package_constraint_map.retain(|name, _| self.is_allowed(name));
            advisory_repo.get_security_advisories(package_constraint_map, allow_partial_advisories)
        } else {
            Ok(SecurityAdvisoryResult {
                names_found: Vec::new(),
                advisories: IndexMap::new(),
            })
        }
    }
}
