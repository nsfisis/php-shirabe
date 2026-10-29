//! ref: composer/src/Composer/Advisory/SecurityAdvisory.php

use crate::advisory::{IgnoredSecurityAdvisory, PartialSecurityAdvisory};
use bstr::{BStr, BString, ByteSlice};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::DATE_RFC3339;
use shirabe_semver::constraint::AnyConstraint;

fn serialize_date_rfc3339<S: serde::Serializer>(
    dt: &DateTime<Utc>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&dt.format(DATE_RFC3339).to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityAdvisory {
    #[serde(flatten)]
    inner: PartialSecurityAdvisory,
    #[serde(serialize_with = "shirabe_php_shim::json_bytes::str")]
    pub title: BString,
    #[serde(serialize_with = "shirabe_php_shim::json_bytes::option_str")]
    pub cve: Option<BString>,
    #[serde(serialize_with = "shirabe_php_shim::json_bytes::option_str")]
    pub link: Option<BString>,
    #[serde(serialize_with = "serialize_date_rfc3339")]
    pub reported_at: DateTime<Utc>,
    #[serde(serialize_with = "shirabe_php_shim::json_bytes::str_map_vec")]
    pub sources: Vec<IndexMap<BString, BString>>,
    #[serde(serialize_with = "shirabe_php_shim::json_bytes::option_str")]
    pub severity: Option<BString>,
}

impl SecurityAdvisory {
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
        severity: Option<BString>,
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

    pub fn advisory_id(&self) -> &BStr {
        self.inner.advisory_id.as_bstr()
    }

    pub fn package_name(&self) -> &BStr {
        self.inner.package_name.as_bstr()
    }

    pub fn affected_versions(&self) -> &AnyConstraint {
        &self.inner.affected_versions
    }

    pub fn to_ignored_advisory(&self, ignore_reason: Option<BString>) -> IgnoredSecurityAdvisory {
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
