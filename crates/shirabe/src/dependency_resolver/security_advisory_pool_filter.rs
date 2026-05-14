//! ref: composer/src/Composer/DependencyResolver/SecurityAdvisoryPoolFilter.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::constraint::Constraint;
use crate::advisory::audit_config::AuditConfig;
use crate::advisory::auditor::Auditor;
use crate::dependency_resolver::pool::Pool;
use crate::dependency_resolver::request::Request;
use crate::package::package_interface::PackageInterface;
use crate::repository::platform_repository::PlatformRepository;
use crate::repository::repository_interface::RepositoryInterface;
use crate::repository::repository_set::RepositorySet;

#[derive(Debug)]
pub struct SecurityAdvisoryPoolFilter {
    auditor: Auditor,
    audit_config: AuditConfig,
}

impl SecurityAdvisoryPoolFilter {
    pub fn new(auditor: Auditor, audit_config: AuditConfig) -> Self {
        Self { auditor, audit_config }
    }

    pub fn filter(&self, pool: Pool, repositories: Vec<Box<dyn RepositoryInterface>>, request: &Request) -> Pool {
        if !self.audit_config.block_insecure {
            return pool;
        }

        let mut repo_set = RepositorySet::new();
        for repo in &repositories {
            repo_set.add_repository(repo.as_ref());
        }

        let mut packages_for_advisories: Vec<Box<dyn PackageInterface>> = vec![];
        for package in pool.get_packages() {
            if !package.is_root() && !PlatformRepository::is_platform_package(package.get_name()) && !request.is_locked_package(package.as_ref()) {
                packages_for_advisories.push(package);
            }
        }

        // all_advisories: ['advisories' => array<string, array<PartialSecurityAdvisory|SecurityAdvisory>>, ...]
        let mut all_advisories: IndexMap<String, PhpMixed> = repo_set.get_matching_security_advisories(&packages_for_advisories, true, true);
        if self.auditor.needs_complete_advisory_load(&all_advisories["advisories"], &self.audit_config.ignore_list_for_blocking) {
            all_advisories = repo_set.get_matching_security_advisories(&packages_for_advisories, false, true);
        }

        // advisory_map: array<string, array<PartialSecurityAdvisory|SecurityAdvisory>>
        let advisory_map: IndexMap<String, Vec<PhpMixed>> = self.auditor.process_advisories(
            &all_advisories["advisories"],
            &self.audit_config.ignore_list_for_blocking,
            &self.audit_config.ignore_severity_for_blocking,
        )["advisories"].clone().into();

        let mut packages: Vec<Box<dyn PackageInterface>> = vec![];
        // security_removed_versions: array<string, array<string, array<PartialSecurityAdvisory|SecurityAdvisory>>>
        let mut security_removed_versions: IndexMap<String, IndexMap<String, Vec<PhpMixed>>> = IndexMap::new();
        // abandoned_removed_versions: array<string, array<string, string>>
        let mut abandoned_removed_versions: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
        for package in pool.get_packages() {
            if self.audit_config.block_abandoned && !self.auditor.filter_abandoned_packages(vec![package.as_ref()], &self.audit_config.ignore_abandoned_for_blocking).is_empty() {
                for package_name in package.get_names(false) {
                    abandoned_removed_versions
                        .entry(package_name)
                        .or_default()
                        .insert(package.get_version().to_string(), package.get_pretty_version().to_string());
                }
                continue;
            }

            let matching_advisories = self.get_matching_advisories(package.as_ref(), &advisory_map);
            if !matching_advisories.is_empty() {
                for package_name in package.get_names(false) {
                    security_removed_versions
                        .entry(package_name)
                        .or_default()
                        .insert(package.get_version().to_string(), matching_advisories.clone());
                }
                continue;
            }

            packages.push(package);
        }

        Pool::new(packages, pool.get_unacceptable_fixed_or_locked_packages(), pool.get_all_removed_versions(), pool.get_all_removed_versions_by_package(), security_removed_versions, abandoned_removed_versions)
    }

    fn get_matching_advisories(&self, package: &dyn PackageInterface, advisory_map: &IndexMap<String, Vec<PhpMixed>>) -> Vec<PhpMixed> {
        if package.is_dev() {
            return vec![];
        }

        let mut matching_advisories: Vec<PhpMixed> = vec![];
        for package_name in package.get_names(false) {
            if !advisory_map.contains_key(&package_name) {
                continue;
            }

            let package_constraint = Constraint::new("==", package.get_version());
            for advisory in &advisory_map[&package_name] {
                // advisory is PartialSecurityAdvisory or SecurityAdvisory; both have affected_versions: Box<dyn ConstraintInterface>
                if advisory.affected_versions().matches(&package_constraint) {
                    matching_advisories.push(advisory.clone());
                }
            }
        }

        matching_advisories
    }
}
