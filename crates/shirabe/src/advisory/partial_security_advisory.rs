//! ref: composer/src/Composer/Advisory/PartialSecurityAdvisory.php

use crate::advisory::{AnySecurityAdvisory, SecurityAdvisory};
use crate::package::version::VersionParser;
use bstr::{B, BStr, BString};
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, date_create, json_bytes, php_regex, preg_replace};
use shirabe_semver::constraint::{AnyConstraint, SimpleConstraint};

fn serialize_constraint<S: serde::Serializer>(
    c: &AnyConstraint,
    serializer: S,
) -> anyhow::Result<S::Ok, S::Error> {
    json_bytes::str(&c.get_pretty_string(), serializer)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartialSecurityAdvisory {
    #[serde(serialize_with = "shirabe_php_shim::json_bytes::str")]
    pub advisory_id: BString,
    #[serde(serialize_with = "shirabe_php_shim::json_bytes::str")]
    pub package_name: BString,
    #[serde(serialize_with = "serialize_constraint")]
    pub affected_versions: AnyConstraint,
}

impl PartialSecurityAdvisory {
    pub fn create(
        package_name: &BStr,
        data: &IndexMap<BString, PhpMixed>,
        parser: &VersionParser,
    ) -> anyhow::Result<AnySecurityAdvisory> {
        let affected_versions_str = data[B("affectedVersions")].as_string().unwrap_or_default();

        let constraint: AnyConstraint = match parser.parse_constraints(affected_versions_str) {
            Ok(c) => c,
            Err(_) => {
                let affected_version = BString::from(
                    &preg_replace(
                        php_regex!(r"{(^[>=<^~]*[\d.]+).*}"),
                        "$1",
                        affected_versions_str,
                    )[..],
                );
                match parser.parse_constraints(&affected_version) {
                    Ok(c) => c,
                    Err(_) => {
                        SimpleConstraint::new("==", "0.0.0-invalid-version".to_string(), None)
                            .into()
                    }
                }
            }
        };

        let has_full_data = data.contains_key(B("title"))
            && data.contains_key(B("sources"))
            && data.contains_key(B("reportedAt"));

        if has_full_data {
            let reported_at: DateTime<Utc> =
                date_create(data[B("reportedAt")].as_string().unwrap_or_default())
                    .unwrap_or_default();
            let sources: Vec<IndexMap<BString, BString>> = data[B("sources")]
                .as_list()
                .map(|list| {
                    list.iter()
                        .filter_map(|item| item.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|(k, v)| {
                                    v.as_string().map(|s| (k.clone(), BString::from(s)))
                                })
                                .collect()
                        })
                        .collect()
                })
                .unwrap_or_default();
            let advisory = SecurityAdvisory::new(
                bstr::BString::from(package_name),
                BString::from(data[B("advisoryId")].as_string().unwrap_or_default()),
                constraint,
                BString::from(data[B("title")].as_string().unwrap_or_default()),
                sources,
                reported_at,
                data.get(B("cve"))
                    .and_then(|v| v.as_string())
                    .map(BString::from),
                data.get(B("link"))
                    .and_then(|v| v.as_string())
                    .map(BString::from),
                data.get(B("severity"))
                    .and_then(|v| v.as_string())
                    .map(BString::from),
            );
            return Ok(AnySecurityAdvisory::Full(advisory));
        }

        Ok(AnySecurityAdvisory::Partial(Self {
            advisory_id: BString::from(data[B("advisoryId")].as_string().unwrap_or_default()),
            package_name: bstr::BString::from(package_name),
            affected_versions: constraint,
        }))
    }

    pub fn new(
        package_name: BString,
        advisory_id: BString,
        affected_versions: AnyConstraint,
    ) -> Self {
        Self {
            advisory_id,
            package_name,
            affected_versions,
        }
    }
}
