//! ref: composer/src/Composer/Advisory/PartialSecurityAdvisory.php

use crate::advisory::security_advisory::SecurityAdvisory;
use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{PhpMixed, UnexpectedValueException};
use shirabe_semver::constraint::constraint::Constraint;
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;
use shirabe_semver::version_parser::VersionParser;

fn serialize_constraint<S: serde::Serializer>(
    c: &Box<dyn ConstraintInterface>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&c.get_pretty_string())
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialSecurityAdvisory {
    pub advisory_id: String,
    pub package_name: String,
    #[serde(serialize_with = "serialize_constraint")]
    pub affected_versions: Box<dyn ConstraintInterface>,
}

impl PartialSecurityAdvisory {
    pub fn create(
        package_name: &str,
        data: &IndexMap<String, PhpMixed>,
        parser: &VersionParser,
    ) -> Result<Box<dyn std::any::Any>> {
        let affected_versions_str = data["affectedVersions"].as_string().unwrap_or("");

        let constraint: Box<dyn ConstraintInterface> =
            match parser.parse_constraints(affected_versions_str) {
                Ok(c) => c,
                Err(_) => {
                    let affected_version =
                        Preg::replace(r"(^[>=<^~]*[\d.]+).*", "$1", affected_versions_str);
                    match parser.parse_constraints(&affected_version) {
                        Ok(c) => c,
                        Err(_) => Box::new(Constraint::new("==", "0.0.0-invalid-version")),
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
            let advisory = SecurityAdvisory::new(
                package_name.to_string(),
                data["advisoryId"].as_string().unwrap_or("").to_string(),
                constraint,
                data["title"].as_string().unwrap_or("").to_string(),
                data["sources"].clone(),
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
            return Ok(Box::new(advisory));
        }

        Ok(Box::new(Self {
            advisory_id: data["advisoryId"].as_string().unwrap_or("").to_string(),
            package_name: package_name.to_string(),
            affected_versions: constraint,
        }))
    }

    pub fn new(
        package_name: String,
        advisory_id: String,
        affected_versions: Box<dyn ConstraintInterface>,
    ) -> Self {
        Self {
            advisory_id,
            package_name,
            affected_versions,
        }
    }
}
