//! ref: composer/src/Composer/Advisory/PartialSecurityAdvisory.php

use crate::advisory::PartialOrFullSecurityAdvisory;
use crate::advisory::SecurityAdvisory;
use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{PhpMixed, UnexpectedValueException};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;
use shirabe_semver::version_parser::VersionParser;

fn serialize_constraint<S: serde::Serializer>(
    c: &AnyConstraint,
    serializer: S,
) -> Result<S::Ok, S::Error> {
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
        data: &IndexMap<String, PhpMixed>,
        parser: &VersionParser,
    ) -> Result<PartialOrFullSecurityAdvisory> {
        let affected_versions_str = data["affectedVersions"].as_string().unwrap_or("");

        let constraint: AnyConstraint = match parser.parse_constraints(affected_versions_str) {
            Ok(c) => c,
            Err(_) => {
                let affected_version =
                    Preg::replace(r"(^[>=<^~]*[\d.]+).*", "$1", affected_versions_str);
                match parser.parse_constraints(affected_version.as_deref().unwrap_or("")) {
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

        let has_full_data = data.contains_key("title")
            && data.contains_key("sources")
            && data.contains_key("reportedAt");

        if has_full_data {
            let reported_at: DateTime<Utc> = Utc
                .datetime_from_str(
                    data["reportedAt"].as_string().unwrap_or(""),
                    "%Y-%m-%dT%H:%M:%S+00:00",
                )
                .unwrap_or_default();
            let sources: Vec<IndexMap<String, String>> = data["sources"]
                .as_list()
                .map(|list| {
                    list.iter()
                        .filter_map(|item| item.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|(k, v)| {
                                    v.as_string().map(|s| (k.clone(), s.to_string()))
                                })
                                .collect()
                        })
                        .collect()
                })
                .unwrap_or_default();
            let advisory = SecurityAdvisory::new(
                package_name.to_string(),
                data["advisoryId"].as_string().unwrap_or("").to_string(),
                constraint,
                data["title"].as_string().unwrap_or("").to_string(),
                sources,
                reported_at,
                data.get("cve")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string()),
                data.get("link")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string()),
                data.get("severity")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string()),
            );
            return Ok(PartialOrFullSecurityAdvisory::Full(advisory));
        }

        Ok(PartialOrFullSecurityAdvisory::Partial(Self {
            advisory_id: data["advisoryId"].as_string().unwrap_or("").to_string(),
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
