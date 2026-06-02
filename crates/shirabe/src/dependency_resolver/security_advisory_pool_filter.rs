//! ref: composer/src/Composer/DependencyResolver/SecurityAdvisoryPoolFilter.php

use crate::advisory::AuditConfig;
use crate::advisory::Auditor;
use crate::advisory::PartialOrFullSecurityAdvisory;
use crate::dependency_resolver::Pool;
use crate::dependency_resolver::Request;
use crate::package::BasePackageHandle;
use crate::repository::PlatformRepository;
use crate::repository::RepositoryInterfaceHandle;
use crate::repository::RepositorySet;
use indexmap::IndexMap;
use shirabe_semver::constraint::SimpleConstraint;

#[derive(Debug)]
pub struct SecurityAdvisoryPoolFilter {
    auditor: Auditor,
    audit_config: AuditConfig,
}

impl SecurityAdvisoryPoolFilter {
    pub fn new(auditor: Auditor, audit_config: AuditConfig) -> Self {
        Self {
            auditor,
            audit_config,
        }
    }

    pub fn filter(
        &self,
        pool: Pool,
        repositories: Vec<RepositoryInterfaceHandle>,
        request: &Request,
    ) -> anyhow::Result<Pool> {
        if !self.audit_config.block_insecure {
            return Ok(pool);
        }

        let mut repo_set = RepositorySet::new(
            "stable",
            IndexMap::new(),
            vec![],
            IndexMap::new(),
            IndexMap::new(),
            IndexMap::new(),
        );
        for repo in repositories {
            repo_set.add_repository(repo)?;
        }

        let mut packages_for_advisories: Vec<BasePackageHandle> = vec![];
        for package in pool.get_packages() {
            if package.as_root().is_none()
                && !PlatformRepository::is_platform_package(&package.get_name())
                && !request.is_locked_package(package.clone())
            {
                packages_for_advisories.push(package.clone());
            }
        }

        let mut all_advisories = repo_set.get_matching_security_advisories(
            packages_for_advisories.clone(),
            true,
            true,
        )?;
        if self.auditor.needs_complete_advisory_load(
            &all_advisories.advisories,
            &self.audit_config.ignore_list_for_blocking,
        ) {
            all_advisories = repo_set.get_matching_security_advisories(
                packages_for_advisories.clone(),
                false,
                true,
            )?;
        }

        let advisory_map = self
            .auditor
            .process_advisories(
                all_advisories.advisories,
                &self.audit_config.ignore_list_for_blocking,
                &self.audit_config.ignore_severity_for_blocking,
            )
            .advisories;

        let mut packages: Vec<BasePackageHandle> = vec![];
        let mut security_removed_versions: IndexMap<
            String,
            IndexMap<String, Vec<PartialOrFullSecurityAdvisory>>,
        > = IndexMap::new();
        let mut abandoned_removed_versions: IndexMap<String, IndexMap<String, String>> =
            IndexMap::new();
        for package in pool.get_packages() {
            if self.audit_config.block_abandoned
                && self
                    .auditor
                    .filter_abandoned_packages(
                        &[package.clone()],
                        &self.audit_config.ignore_abandoned_for_blocking,
                    )?
                    .len()
                    != 0
            {
                for package_name in package.get_names(false) {
                    abandoned_removed_versions
                        .entry(package_name)
                        .or_default()
                        .insert(
                            package.get_version().to_string(),
                            package.get_pretty_version().to_string(),
                        );
                }
                continue;
            }

            let matching_advisories = self.get_matching_advisories(package.clone(), &advisory_map);
            if matching_advisories.len() > 0 {
                for package_name in package.get_names(false) {
                    security_removed_versions
                        .entry(package_name)
                        .or_default()
                        .insert(
                            package.get_version().to_string(),
                            matching_advisories.clone(),
                        );
                }

                continue;
            }

            packages.push(package.clone());
        }

        Ok(Pool::new(
            packages,
            pool.get_unacceptable_fixed_or_locked_packages().clone(),
            pool.get_all_removed_versions().clone(),
            pool.get_all_removed_versions_by_package().clone(),
            security_removed_versions,
            abandoned_removed_versions,
        ))
    }

    fn get_matching_advisories(
        &self,
        package: BasePackageHandle,
        advisory_map: &IndexMap<String, Vec<PartialOrFullSecurityAdvisory>>,
    ) -> Vec<PartialOrFullSecurityAdvisory> {
        if package.is_dev() {
            return vec![];
        }

        let mut matching_advisories: Vec<PartialOrFullSecurityAdvisory> = vec![];
        for package_name in package.get_names(false) {
            if !advisory_map.contains_key(&package_name) {
                continue;
            }

            let package_constraint =
                SimpleConstraint::new("==".to_string(), package.get_version().to_string(), None)
                    .into();
            for advisory in &advisory_map[&package_name] {
                if advisory.affected_versions().matches(&package_constraint) {
                    matching_advisories.push(advisory.clone());
                }
            }
        }

        matching_advisories
    }
}
