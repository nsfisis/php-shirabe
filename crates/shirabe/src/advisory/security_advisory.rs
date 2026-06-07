//! ref: composer/src/Composer/Advisory/SecurityAdvisory.php

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;

use crate::advisory::IgnoredSecurityAdvisory;
use crate::advisory::PartialSecurityAdvisory;

#[derive(Debug, Clone, serde::Serialize)]
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
        affected_versions: AnyConstraint,
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

    pub fn advisory_id(&self) -> &str {
        &self.inner.advisory_id
    }

    pub fn package_name(&self) -> &str {
        &self.inner.package_name
    }

    pub fn affected_versions(&self) -> &AnyConstraint {
        &self.inner.affected_versions
    }

    pub fn to_ignored_advisory(&self, ignore_reason: Option<String>) -> IgnoredSecurityAdvisory {
        IgnoredSecurityAdvisory::new(
            self.inner.package_name.clone(),
            self.inner.advisory_id.clone(),
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
