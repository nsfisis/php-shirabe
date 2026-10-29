//! ref: composer/src/Composer/Util/ForgejoUrl.php

use bstr::{BStr, BString};
use shirabe_php_shim::{InvalidArgumentException, bconcat, preg_match, strtolower};

#[derive(Debug)]
pub struct ForgejoUrl {
    pub owner: BString,
    pub repository: BString,
    pub origin_url: BString,
    pub api_url: BString,
}

impl ForgejoUrl {
    pub const URL_REGEX: &'static str =
        r"{^(?:(?:https?|git)://([^/]+)/|git@([^:]+):/?)([^/]+)/([^/]+?)(?:\.git|/)?$}";

    fn new(owner: BString, repository: BString, origin_url: BString, api_url: BString) -> Self {
        Self {
            owner,
            repository,
            origin_url,
            api_url,
        }
    }

    pub fn create(repo_url: &BStr) -> anyhow::Result<Self> {
        match Self::try_from(Some(repo_url)) {
            Some(url) => Ok(url),
            None => Err(InvalidArgumentException::new(format!(
                "This is not a valid Forgejo URL: {}",
                repo_url.display_lossy()
            ))
            .into()),
        }
    }

    pub fn try_from(repo_url: Option<&BStr>) -> Option<Self> {
        let repo_url = repo_url?;
        let matches = preg_match(Self::URL_REGEX, repo_url)?;

        let origin_url = strtolower(
            matches
                .get(1)
                .or_else(|| matches.get(2))
                .unwrap_or_default(),
        );
        let api_base = bconcat!(origin_url, "/api/v1");
        let owner = BString::from(matches.get(3).unwrap_or_default());
        let repository = BString::from(matches.get(4).unwrap_or_default());
        let api_url = bconcat!("https://", api_base, "/repos/", owner, "/", repository);

        Some(Self::new(owner, repository, origin_url, api_url))
    }

    pub fn generate_ssh_url(&self) -> BString {
        bconcat!(
            "git@",
            self.origin_url,
            ":",
            self.owner,
            "/",
            self.repository,
            ".git"
        )
    }
}
