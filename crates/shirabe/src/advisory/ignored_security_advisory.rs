//! ref: composer/src/Composer/Advisory/IgnoredSecurityAdvisory.php

use crate::advisory::SecurityAdvisory;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredSecurityAdvisory {
    #[serde(flatten)]
    inner: SecurityAdvisory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_reason: Option<String>,
}

impl IgnoredSecurityAdvisory {
    pub fn new(
        package_name: String,
        advisory_id: String,
        affected_versions: AnyConstraint,
        title: String,
        sources: Vec<IndexMap<String, String>>,
        reported_at: DateTime<Utc>,
        cve: Option<String>,
        link: Option<String>,
        ignore_reason: Option<String>,
        severity: Option<String>,
    ) -> Self {
        let inner = SecurityAdvisory::new(
            package_name,
            advisory_id,
            affected_versions,
            title,
            sources,
            reported_at,
            cve,
            link,
            severity,
        );
        Self {
            inner,
            ignore_reason,
        }
    }
}
