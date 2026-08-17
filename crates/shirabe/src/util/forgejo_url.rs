//! ref: composer/src/Composer/Util/ForgejoUrl.php

use shirabe_pcre::Preg;
use shirabe_php_shim::InvalidArgumentException;

#[derive(Debug)]
pub struct ForgejoUrl {
    pub owner: String,
    pub repository: String,
    pub origin_url: String,
    pub api_url: String,
}

impl ForgejoUrl {
    pub const URL_REGEX: &'static str =
        r"{^(?:(?:https?|git)://([^/]+)/|git@([^:]+):/?)([^/]+)/([^/]+?)(?:\.git|/)?$}";

    fn new(owner: String, repository: String, origin_url: String, api_url: String) -> Self {
        Self {
            owner,
            repository,
            origin_url,
            api_url,
        }
    }

    pub fn create(repo_url: &str) -> anyhow::Result<Self> {
        match Self::try_from(Some(repo_url)) {
            Some(url) => Ok(url),
            None => Err(InvalidArgumentException::new(format!(
                "This is not a valid Forgejo URL: {}",
                repo_url
            ))
            .into()),
        }
    }

    pub fn try_from(repo_url: Option<&str>) -> Option<Self> {
        let repo_url = repo_url?;
        let matches = Preg::match3(Self::URL_REGEX, repo_url)?;
        use shirabe_pcre::CaptureKey;
        let m: Vec<String> = (0..5)
            .map(|i| {
                matches
                    .get(&CaptureKey::ByIndex(i))
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();

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
            origin_url,
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
