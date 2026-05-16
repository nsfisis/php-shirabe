//! ref: composer/src/Composer/Util/ForgejoUrl.php

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::InvalidArgumentException;

pub struct ForgejoUrl {
    pub owner: String,
    pub repository: String,
    pub origin_url: String,
    pub api_url: String,
}

impl ForgejoUrl {
    pub const URL_REGEX: &'static str =
        r"^(?:(?:https?|git)://([^/]+)/|git@([^:]+):/?)([^/]+)/([^/]+?)(?:\.git|/)?$";

    fn new(owner: String, repository: String, origin_url: String, api_url: String) -> Self {
        Self {
            owner,
            repository,
            origin_url,
            api_url,
        }
    }

    pub fn create(repo_url: &str) -> Result<Self> {
        match Self::try_from(Some(repo_url)) {
            Some(url) => Ok(url),
            None => Err(InvalidArgumentException {
                message: format!("This is not a valid Forgejo URL: {}", repo_url),
                code: 0,
            }
            .into()),
        }
    }

    pub fn try_from(repo_url: Option<&str>) -> Option<Self> {
        let repo_url = repo_url?;
        let m = Preg::match_(Self::URL_REGEX, repo_url)?;

        let origin_url = if !m[1].is_empty() {
            m[1].clone()
        } else {
            m[2].clone()
        }
        .to_lowercase();
        let api_base = format!("{}/api/v1", origin_url);

        Some(Self::new(
            m[3].clone(),
            m[4].clone(),
            origin_url.clone(),
            format!("https://{}/repos/{}/{}", api_base, m[3], m[4]),
        ))
    }

    pub fn generate_ssh_url(&self) -> String {
        format!(
            "git@{}:{}/{}.git",
            self.origin_url, self.owner, self.repository
        )
    }
}
