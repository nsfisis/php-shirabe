//! ref: composer/src/Composer/Advisory/IgnoredSecurityAdvisory.php

use crate::advisory::SecurityAdvisory;
use bstr::BString;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_semver::constraint::AnyConstraint;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredSecurityAdvisory {
    #[serde(flatten)]
    inner: SecurityAdvisory,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "shirabe_php_shim::json_bytes::option_str"
    )]
    pub ignore_reason: Option<BString>,
}

impl IgnoredSecurityAdvisory {
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn new(
        package_name: BString,
        advisory_id: BString,
        affected_versions: AnyConstraint,
        title: BString,
        sources: Vec<IndexMap<BString, BString>>,
        reported_at: DateTime<Utc>,
        cve: Option<BString>,
        link: Option<BString>,
        ignore_reason: Option<BString>,
        severity: Option<BString>,
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

    pub fn as_security_advisory(&self) -> &SecurityAdvisory {
        &self.inner
    }
}
