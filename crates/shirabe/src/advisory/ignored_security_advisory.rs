//! ref: composer/src/Composer/Advisory/IgnoredSecurityAdvisory.php

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;
use crate::advisory::security_advisory::SecurityAdvisory;

#[derive(Debug)]
pub struct IgnoredSecurityAdvisory {
    inner: SecurityAdvisory,
    pub ignore_reason: Option<String>,
}

impl IgnoredSecurityAdvisory {
    pub fn new(
        package_name: String,
        advisory_id: String,
        affected_versions: Box<dyn ConstraintInterface>,
        title: String,
        sources: Vec<IndexMap<String, String>>,
        reported_at: DateTime<Utc>,
        cve: Option<String>,
        link: Option<String>,
        ignore_reason: Option<String>,
        severity: Option<String>,
    ) -> Self {
        let inner = SecurityAdvisory::new(package_name, advisory_id, affected_versions, title, sources, reported_at, cve, link, severity);
        Self {
            inner,
            ignore_reason,
        }
    }

    pub fn json_serialize(&self) -> PhpMixed {
        let mut data = self.inner.json_serialize();
        if self.ignore_reason.is_none() {
            if let PhpMixed::Array(ref mut map) = data {
                map.remove("ignoreReason");
            }
        }
        data
    }
}
