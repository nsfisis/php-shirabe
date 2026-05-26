//! ref: composer/src/Composer/DependencyResolver/SecurityAdvisoryPoolFilter.php

use crate::advisory::AuditConfig;
use crate::advisory::Auditor;
use crate::advisory::PartialSecurityAdvisory;
use crate::dependency_resolver::Pool;
use crate::dependency_resolver::Request;
use crate::package::BasePackageHandle;
use crate::repository::RepositoryInterface;
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;
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
        repositories: Vec<Box<dyn RepositoryInterface>>,
        request: &Request,
    ) -> Pool {
        // TODO(phase-c): port the filter() body. Blockers:
        //   * RepositorySet::new takes 6 args; ConfigSourceInterface refactor pending
        //   * pool.get_packages() yields BasePackageHandle; widen to PackageInterfaceHandle
        //     (via .into()) where the audit/repo APIs expect PackageInterface.
        //   * Pool::new requires owned Vecs; clone the handles out of the existing pool.
        //   * advisory map element type mismatch (PhpMixed vs PartialSecurityAdvisory).
        let _ = (
            pool,
            repositories,
            request,
            &self.auditor,
            &self.audit_config,
        );
        todo!("port SecurityAdvisoryPoolFilter::filter")
    }

    /// @param array<string, array<PartialSecurityAdvisory|SecurityAdvisory>> $advisoryMap
    /// @return list<PartialSecurityAdvisory|SecurityAdvisory>
    fn get_matching_advisories(
        &self,
        package: BasePackageHandle,
        advisory_map: &IndexMap<String, Vec<PartialSecurityAdvisory>>,
    ) -> Vec<PartialSecurityAdvisory> {
        if package.is_dev() {
            return vec![];
        }

        let mut matching_advisories: Vec<PartialSecurityAdvisory> = vec![];
        for package_name in package.get_names(false) {
            if !advisory_map.contains_key(&package_name) {
                continue;
            }

            let package_constraint =
                SimpleConstraint::new("==".to_string(), package.get_version().to_string(), None)
                    .into();
            for advisory in &advisory_map[&package_name] {
                // advisory is PartialSecurityAdvisory or SecurityAdvisory; both have affected_versions: Box<dyn ConstraintInterface>
                if advisory.affected_versions.matches(&package_constraint) {
                    // TODO(phase-b): PartialSecurityAdvisory is not Clone; replace with Rc when sharing is needed
                    matching_advisories.push(todo!("clone PartialSecurityAdvisory"));
                }
            }
        }

        matching_advisories
    }
}
