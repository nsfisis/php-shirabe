//! ref: composer/src/Composer/Advisory/AuditConfig.php

use indexmap::IndexMap;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed};

use crate::advisory::auditor::Auditor;
use crate::config::Config;

#[derive(Debug)]
pub struct AuditConfig {
    pub audit: bool,
    pub audit_format: String,
    pub audit_abandoned: String,
    pub block_insecure: bool,
    pub block_abandoned: bool,
    pub ignore_unreachable: bool,
    pub ignore_list_for_audit: IndexMap<String, Option<String>>,
    pub ignore_list_for_blocking: IndexMap<String, Option<String>>,
    pub ignore_severity_for_audit: IndexMap<String, Option<String>>,
    pub ignore_severity_for_blocking: IndexMap<String, Option<String>>,
    pub ignore_abandoned_for_audit: IndexMap<String, Option<String>>,
    pub ignore_abandoned_for_blocking: IndexMap<String, Option<String>>,
}

impl AuditConfig {
    pub fn new(
        audit: bool,
        audit_format: String,
        audit_abandoned: String,
        block_insecure: bool,
        block_abandoned: bool,
        ignore_unreachable: bool,
        ignore_list_for_audit: IndexMap<String, Option<String>>,
        ignore_list_for_blocking: IndexMap<String, Option<String>>,
        ignore_severity_for_audit: IndexMap<String, Option<String>>,
        ignore_severity_for_blocking: IndexMap<String, Option<String>>,
        ignore_abandoned_for_audit: IndexMap<String, Option<String>>,
        ignore_abandoned_for_blocking: IndexMap<String, Option<String>>,
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
        IndexMap<String, Option<String>>,
        IndexMap<String, Option<String>>,
    )> {
        let mut for_audit: IndexMap<String, Option<String>> = IndexMap::new();
        let mut for_block: IndexMap<String, Option<String>> = IndexMap::new();

        let entries = match config {
            PhpMixed::Array(arr) => arr,
            PhpMixed::List(list) => {
                for value in list {
                    if let Some(id) = value.as_string() {
                        for_audit.insert(id.to_string(), None);
                        for_block.insert(id.to_string(), None);
                    }
                }
                return Ok((for_audit, for_block));
            }
            _ => return Ok((for_audit, for_block)),
        };

        for (key, value) in entries {
            let (id, apply, reason) = match value.as_ref() {
                PhpMixed::String(reason_str) => {
                    (key.clone(), "all".to_string(), Some(reason_str.clone()))
                }
                PhpMixed::Array(detail) => {
                    let apply = detail
                        .get("apply")
                        .and_then(|v| v.as_string())
                        .unwrap_or("all")
                        .to_string();
                    let reason = detail
                        .get("reason")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string());

                    if !["audit", "block", "all"].contains(&apply.as_str()) {
                        return Err(InvalidArgumentException {
                            message: format!(
                                "Invalid 'apply' value for '{}': {}. Expected 'audit', 'block', or 'all'.",
                                key, apply
                            ),
                            code: 0,
                        }.into());
                    }

                    (key.clone(), apply, reason)
                }
                PhpMixed::Null => (key.clone(), "all".to_string(), None),
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

    pub fn from_config(config: &Config, audit: bool, audit_format: &str) -> anyhow::Result<Self> {
        let audit_config_raw = config.get("audit");
        let audit_config = audit_config_raw.as_array();

        let empty_array = PhpMixed::Array(IndexMap::new());

        let ignore_raw = audit_config
            .and_then(|m| m.get("ignore"))
            .map(|v| *v.clone())
            .unwrap_or_else(|| empty_array.clone());
        let (ignore_list_for_audit, ignore_list_for_blocking) =
            Self::parse_ignore_with_apply(&ignore_raw)?;

        let ignore_abandoned_raw = audit_config
            .and_then(|m| m.get("ignore-abandoned"))
            .map(|v| *v.clone())
            .unwrap_or_else(|| empty_array.clone());
        let (ignore_abandoned_for_audit, ignore_abandoned_for_blocking) =
            Self::parse_ignore_with_apply(&ignore_abandoned_raw)?;

        let ignore_severity_raw = audit_config
            .and_then(|m| m.get("ignore-severity"))
            .map(|v| *v.clone())
            .unwrap_or_else(|| empty_array.clone());
        let (ignore_severity_for_audit, ignore_severity_for_blocking) =
            Self::parse_ignore_with_apply(&ignore_severity_raw)?;

        let audit_abandoned = audit_config
            .and_then(|m| m.get("abandoned"))
            .and_then(|v| v.as_string())
            .unwrap_or(Auditor::ABANDONED_FAIL)
            .to_string();

        let block_insecure = audit_config
            .and_then(|m| m.get("block-insecure"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let block_abandoned = audit_config
            .and_then(|m| m.get("block-abandoned"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let ignore_unreachable = audit_config
            .and_then(|m| m.get("ignore-unreachable"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(Self::new(
            audit,
            audit_format.to_string(),
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
