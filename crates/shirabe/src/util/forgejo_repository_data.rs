//! ref: composer/src/Composer/Util/ForgejoRepositoryData.php sha1:85eba081222dc3953261a55bb8a32c9f16490d49

use bstr::{B, BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, TypeError, bconcat};

#[derive(Debug)]
pub struct ForgejoRepositoryData {
    pub html_url: BString,
    pub ssh_url: BString,
    pub http_clone_url: BString,
    pub is_private: bool,
    pub default_branch: BString,
    pub has_issues: bool,
    pub is_archived: bool,
}

impl ForgejoRepositoryData {
    pub fn new(
        html_url: BString,
        http_clone_url: BString,
        ssh_url: BString,
        is_private: bool,
        default_branch: BString,
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

    pub fn from_remote_data(data: &IndexMap<BString, PhpMixed>) -> anyhow::Result<Self> {
        let get_string = |key: &BStr| {
            data.get(B(key))
                .and_then(|v| v.as_string())
                .map(BStr::to_owned)
                .ok_or_else(|| {
                    anyhow::Error::from(TypeError::new(bconcat!(
                        "missing or invalid string field: ",
                        key
                    )))
                })
        };
        let get_bool = |key: &BStr| {
            data.get(B(key)).and_then(|v| v.as_bool()).ok_or_else(|| {
                anyhow::Error::from(TypeError::new(bconcat!(
                    "missing or invalid bool field: ",
                    key
                )))
            })
        };
        Ok(Self::new(
            get_string(BStr::new("html_url"))?,
            get_string(BStr::new("clone_url"))?,
            get_string(BStr::new("ssh_url"))?,
            get_bool(BStr::new("private"))?,
            get_string(BStr::new("default_branch"))?,
            get_bool(BStr::new("has_issues"))?,
            get_bool(BStr::new("archived"))?,
        ))
    }
}
