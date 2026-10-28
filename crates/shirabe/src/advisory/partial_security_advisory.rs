//! ref: composer/src/Composer/Advisory/PartialSecurityAdvisory.php

use crate::advisory::{AnySecurityAdvisory, SecurityAdvisory};
use crate::package::version::VersionParser;
use bstr::{B, BString, ByteSlice};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, date_create, php_regex, preg_replace};
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};

fn serialize_constraint<S: serde::Serializer>(
    c: &AnyConstraint,
    serializer: S,
) -> anyhow::Result<S::Ok, S::Error> {
    serializer.serialize_str(&c.get_pretty_string())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialSecurityAdvisory {
    pub advisory_id: String,
    pub package_name: String,
    #[serde(serialize_with = "serialize_constraint")]
    pub affected_versions: AnyConstraint,
}

impl PartialSecurityAdvisory {
    pub fn create(
        package_name: &str,
        data: &IndexMap<BString, PhpMixed>,
        parser: &VersionParser,
    ) -> anyhow::Result<AnySecurityAdvisory> {
        let affected_versions_str = data[B("affectedVersions")].as_string().unwrap_or_default();

        let constraint: AnyConstraint =
            match parser.parse_constraints(&affected_versions_str.to_str_lossy()) {
                Ok(c) => c,
                Err(_) => {
                    let affected_version = preg_replace(
                        php_regex!(r"{(^[>=<^~]*[\d.]+).*}"),
                        "$1",
                        &affected_versions_str.to_str_lossy(),
                    );
                    match parser.parse_constraints(&affected_version) {
                        Ok(c) => c,
                        Err(_) => SimpleConstraint::new(
                            "==".to_string(),
                            "0.0.0-invalid-version".to_string(),
                            None,
                        )
                        .into(),
                    }
                }
            };

        let has_full_data = data.contains_key(B("title"))
            && data.contains_key(B("sources"))
            && data.contains_key(B("reportedAt"));

        if has_full_data {
            let reported_at: DateTime<Utc> = date_create(
                &data[B("reportedAt")]
                    .as_string()
                    .unwrap_or_default()
                    .to_str_lossy(),
            )
            .unwrap_or_default();
            let sources: Vec<IndexMap<String, String>> = data[B("sources")]
                .as_list()
                .map(|list| {
                    list.iter()
                        .filter_map(|item| item.as_array())
                        .map(|arr| {
                            arr.iter()
                                // TODO(bytes): the keys are Strings, so non-UTF-8 bytes are replaced.
                                .filter_map(|(k, v)| {
                                    v.as_string()
                                        .map(|s| (k.to_str_lossy().into_owned(), s.to_string()))
                                })
                                .collect()
                        })
                        .collect()
                })
                .unwrap_or_default();
            let advisory = SecurityAdvisory::new(
                package_name.to_string(),
                data[B("advisoryId")]
                    .as_string()
                    .unwrap_or_default()
                    .to_string(),
                constraint,
                data[B("title")].as_string().unwrap_or_default().to_string(),
                sources,
                reported_at,
                data.get(B("cve"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string()),
                data.get(B("link"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string()),
                data.get(B("severity"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string()),
            );
            return Ok(AnySecurityAdvisory::Full(advisory));
        }

        Ok(AnySecurityAdvisory::Partial(Self {
            advisory_id: data[B("advisoryId")]
                .as_string()
                .unwrap_or_default()
                .to_string(),
            package_name: package_name.to_string(),
            affected_versions: constraint,
        }))
    }

    pub fn new(
        package_name: String,
        advisory_id: String,
        affected_versions: AnyConstraint,
    ) -> Self {
        Self {
            advisory_id,
            package_name,
            affected_versions,
        }
    }
}
