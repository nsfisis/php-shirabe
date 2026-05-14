//! ref: composer/src/Composer/Advisory/SecurityAdvisory.php

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

use crate::advisory::ignored_security_advisory::IgnoredSecurityAdvisory;
use crate::advisory::partial_security_advisory::PartialSecurityAdvisory;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityAdvisory {
    #[serde(flatten)]
    inner: PartialSecurityAdvisory,
    pub title: String,
    pub cve: Option<String>,
    pub link: Option<String>,
    pub reported_at: DateTime<Utc>,
    pub sources: Vec<IndexMap<String, String>>,
    pub severity: Option<String>,
}

impl SecurityAdvisory {
    pub fn new(
        package_name: String,
        advisory_id: String,
        affected_versions: Box<dyn ConstraintInterface>,
        title: String,
        sources: Vec<IndexMap<String, String>>,
        reported_at: DateTime<Utc>,
        cve: Option<String>,
        link: Option<String>,
        severity: Option<String>,
    ) -> Self {
        let inner = PartialSecurityAdvisory::new(package_name, advisory_id, affected_versions);
        Self {
            inner,
            title,
            sources,
            reported_at,
            cve,
            link,
            severity,
        }
    }

    pub fn to_ignored_advisory(&self, ignore_reason: Option<String>) -> IgnoredSecurityAdvisory {
        IgnoredSecurityAdvisory::new(
            self.inner.package_name.clone(),
            self.inner.advisory_id.clone(),
            // TODO: Phase B - handle shared ownership of affected_versions
            self.inner.affected_versions.clone(),
            self.title.clone(),
            self.sources.clone(),
            self.reported_at,
            self.cve.clone(),
            self.link.clone(),
            ignore_reason,
            self.severity.clone(),
        )
    }
}
