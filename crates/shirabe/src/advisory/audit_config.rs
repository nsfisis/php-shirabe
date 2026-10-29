//! ref: composer/src/Composer/Advisory/AuditConfig.php

use crate::advisory::Auditor;
use crate::config::Config;
use bstr::{B, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, canonical_int_key, legacy_lossy};

#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub audit: bool,
    pub audit_format: BString,
    pub audit_abandoned: BString,
    pub block_insecure: bool,
    pub block_abandoned: bool,
    pub ignore_unreachable: bool,
    pub ignore_list_for_audit: IndexMap<BString, Option<BString>>,
    pub ignore_list_for_blocking: IndexMap<BString, Option<BString>>,
    pub ignore_severity_for_audit: IndexMap<BString, Option<BString>>,
    pub ignore_severity_for_blocking: IndexMap<BString, Option<BString>>,
    pub ignore_abandoned_for_audit: IndexMap<BString, Option<BString>>,
    pub ignore_abandoned_for_blocking: IndexMap<BString, Option<BString>>,
}

impl AuditConfig {
    #[allow(clippy::too_many_arguments, reason = "to keep PHP signature")]
    pub fn new(
        audit: bool,
        audit_format: BString,
        audit_abandoned: BString,
        block_insecure: bool,
        block_abandoned: bool,
        ignore_unreachable: bool,
        ignore_list_for_audit: IndexMap<BString, Option<BString>>,
        ignore_list_for_blocking: IndexMap<BString, Option<BString>>,
        ignore_severity_for_audit: IndexMap<BString, Option<BString>>,
        ignore_severity_for_blocking: IndexMap<BString, Option<BString>>,
        ignore_abandoned_for_audit: IndexMap<BString, Option<BString>>,
        ignore_abandoned_for_blocking: IndexMap<BString, Option<BString>>,
    ) -> Self {
        Self {
            audit,
            audit_format,
            audit_abandoned,
            block_insecure,
            block_abandoned,
            ignore_unreachable,
            ignore_list_for_audit,
            ignore_list_for_blocking,
            ignore_severity_for_audit,
            ignore_severity_for_blocking,
            ignore_abandoned_for_audit,
            ignore_abandoned_for_blocking,
        }
    }

    /// Parse ignore configuration supporting both simple and detailed formats with apply scopes.
    ///
    /// Simple format: ['CVE-123', 'CVE-456'] or ['CVE-123' => 'reason']
    /// Detailed format: ['CVE-123' => ['apply' => 'audit|block|all', 'reason' => '...']]
    fn parse_ignore_with_apply(
        config: &PhpMixed,
    ) -> anyhow::Result<(
        IndexMap<BString, Option<BString>>,
        IndexMap<BString, Option<BString>>,
    )> {
        let mut for_audit: IndexMap<BString, Option<BString>> = IndexMap::new();
        let mut for_block: IndexMap<BString, Option<BString>> = IndexMap::new();

        let entries = match config {
            PhpMixed::Array(arr) => arr,
            PhpMixed::List(list) => {
                for value in list {
                    if let Some(id) = value.as_string() {
                        for_audit.insert(BString::from(id), None);
                        for_block.insert(BString::from(id), None);
                    }
                }
                return Ok((for_audit, for_block));
            }
            _ => return Ok((for_audit, for_block)),
        };

        for (key, value) in entries {
            let (id, apply, reason) = match value {
                PhpMixed::String(reason_str) => {
                    // TODO(type-model): PHP's `array` type must be modeled more precisely. This is
                    // escape hatch.
                    if canonical_int_key(key).is_some() {
                        (reason_str.clone(), BString::from("all"), None)
                    } else {
                        (key.clone(), BString::from("all"), Some(reason_str.clone()))
                    }
                }
                PhpMixed::Array(detail) => {
                    let apply = BString::from(
                        detail
                            .get(B("apply"))
                            .and_then(|v| v.as_string())
                            .unwrap_or("all".into()),
                    );
                    let reason = detail
                        .get(B("reason"))
                        .and_then(|v| v.as_string())
                        .map(BString::from);

                    if !["audit", "block", "all"].iter().any(|a| apply == *a) {
                        return Err(InvalidArgumentException::new(format!(
                            "Invalid 'apply' value for '{}': {}. Expected 'audit', 'block', or 'all'.",
                            key.display_lossy(), apply.display_lossy()
                        )).into());
                    }

                    (key.clone(), apply, reason)
                }
                PhpMixed::Null => (key.clone(), BString::from("all"), None),
                _ => continue,
            };

            if apply == "audit" || apply == "all" {
                for_audit.insert(id.clone(), reason.clone());
            }
            if apply == "block" || apply == "all" {
                for_block.insert(id, reason);
            }
        }

        Ok((for_audit, for_block))
    }

    pub fn from_config(
        config: &mut Config,
        audit: bool,
        audit_format: &str,
    ) -> anyhow::Result<Self> {
        let audit_config_raw = config.get(&legacy_lossy(b"audit".as_slice()));
        let audit_config = audit_config_raw.as_array();

        let empty_array = PhpMixed::Array(IndexMap::new());

        let ignore_raw = audit_config
            .and_then(|m| m.get(B("ignore")))
            .cloned()
            .unwrap_or_else(|| empty_array.clone());
        let (ignore_list_for_audit, ignore_list_for_blocking) =
            Self::parse_ignore_with_apply(&ignore_raw)?;

        let ignore_abandoned_raw = audit_config
            .and_then(|m| m.get(B("ignore-abandoned")))
            .cloned()
            .unwrap_or_else(|| empty_array.clone());
        let (ignore_abandoned_for_audit, ignore_abandoned_for_blocking) =
            Self::parse_ignore_with_apply(&ignore_abandoned_raw)?;

        let ignore_severity_raw = audit_config
            .and_then(|m| m.get(B("ignore-severity")))
            .cloned()
            .unwrap_or_else(|| empty_array.clone());
        let (ignore_severity_for_audit, ignore_severity_for_blocking) =
            Self::parse_ignore_with_apply(&ignore_severity_raw)?;

        let audit_abandoned = BString::from(
            audit_config
                .and_then(|m| m.get(B("abandoned")))
                .and_then(|v| v.as_string())
                .unwrap_or(Auditor::ABANDONED_FAIL.into()),
        );

        let block_insecure = audit_config
            .and_then(|m| m.get(B("block-insecure")))
            .and_then(|v| v.as_opt())
            .map(|v| v.to_bool())
            .unwrap_or(true);

        let block_abandoned = audit_config
            .and_then(|m| m.get(B("block-abandoned")))
            .and_then(|v| v.as_opt())
            .map(|v| v.to_bool())
            .unwrap_or(false);

        let ignore_unreachable = audit_config
            .and_then(|m| m.get(B("ignore-unreachable")))
            .and_then(|v| v.as_opt())
            .map(|v| v.to_bool())
            .unwrap_or(false);

        Ok(Self::new(
            audit,
            bstr::BString::from(audit_format.to_string()),
            audit_abandoned,
            block_insecure,
            block_abandoned,
            ignore_unreachable,
            ignore_list_for_audit,
            ignore_list_for_blocking,
            ignore_severity_for_audit,
            ignore_severity_for_blocking,
            ignore_abandoned_for_audit,
            ignore_abandoned_for_blocking,
        ))
    }
}
