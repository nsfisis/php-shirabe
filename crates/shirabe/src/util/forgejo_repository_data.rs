//! ref: composer/src/Composer/Util/ForgejoRepositoryData.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct ForgejoRepositoryData {
    pub html_url: String,
    pub ssh_url: String,
    pub http_clone_url: String,
    pub is_private: bool,
    pub default_branch: String,
    pub has_issues: bool,
    pub is_archived: bool,
}

impl ForgejoRepositoryData {
    pub fn new(
        html_url: String,
        http_clone_url: String,
        ssh_url: String,
        is_private: bool,
        default_branch: String,
        has_issues: bool,
        is_archived: bool,
    ) -> Self {
        Self {
            html_url,
            http_clone_url,
            ssh_url,
            is_private,
            default_branch,
            has_issues,
            is_archived,
        }
    }

    pub fn from_remote_data(data: &IndexMap<String, PhpMixed>) -> anyhow::Result<Self> {
        let get_string = |key: &str| {
            data.get(key)
                .and_then(|v| v.as_string())
                .map(|s| s.to_owned())
                .ok_or_else(|| anyhow::anyhow!("missing or invalid string field: {key}"))
        };
        let get_bool = |key: &str| {
            data.get(key)
                .and_then(|v| v.as_bool())
                .ok_or_else(|| anyhow::anyhow!("missing or invalid bool field: {key}"))
        };
        Ok(Self::new(
            get_string("html_url")?,
            get_string("clone_url")?,
            get_string("ssh_url")?,
            get_bool("private")?,
            get_string("default_branch")?,
            get_bool("has_issues")?,
            get_bool("archived")?,
        ))
    }
}
