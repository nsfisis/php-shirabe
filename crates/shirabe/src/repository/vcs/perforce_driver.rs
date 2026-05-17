//! ref: composer/src/Composer/Repository/Vcs/PerforceDriver.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{BadMethodCallException, PhpMixed, RuntimeException};

use crate::cache::Cache;
use crate::config::Config;
use crate::io::io_interface::IOInterface;
use crate::repository::vcs::vcs_driver::VcsDriverBase;
use crate::util::http::response::Response;
use crate::util::perforce::Perforce;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct PerforceDriver {
    inner: VcsDriverBase,
    pub(crate) depot: String,
    pub(crate) branch: String,
    pub(crate) perforce: Option<Perforce>,
}

impl PerforceDriver {
    pub fn initialize(&mut self) -> anyhow::Result<()> {
        self.depot = self
            .inner
            .repo_config
            .get("depot")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        self.branch = String::new();
        if let Some(branch) = self
            .inner
            .repo_config
            .get("branch")
            .and_then(|v| v.as_string())
        {
            if !branch.is_empty() {
                self.branch = branch.to_string();
            }
        }

        let repo_config = self.inner.repo_config.clone();
        self.init_perforce(&repo_config)?;
        self.perforce.as_mut().unwrap().p4_login()?;
        self.perforce.as_mut().unwrap().check_stream()?;
        self.perforce.as_mut().unwrap().write_p4_client_spec()?;
        self.perforce.as_mut().unwrap().connect_client()?;

        Ok(())
    }

    fn init_perforce(&mut self, repo_config: &IndexMap<String, PhpMixed>) -> anyhow::Result<()> {
        if self.perforce.is_some() {
            return Ok(());
        }

        let cache_vcs_dir = self
            .inner
            .config
            .get("cache-vcs-dir")
            .as_string()
            .unwrap_or("")
            .to_string();
        if !Cache::is_usable(&cache_vcs_dir) {
            return Err(RuntimeException {
                message: "PerforceDriver requires a usable cache directory, and it looks like you set it to be disabled".to_string(),
                code: 0,
            }.into());
        }

        let repo_dir = format!("{}/{}", cache_vcs_dir, self.depot);
        self.perforce = Some(Perforce::create(
            repo_config,
            &self.inner.url,
            &repo_dir,
            &self.inner.process,
            self.inner.io.as_ref(),
        )?);

        Ok(())
    }

    pub fn get_file_content(&self, file: &str, identifier: &str) -> anyhow::Result<Option<String>> {
        self.perforce
            .as_ref()
            .unwrap()
            .get_file_content(file, identifier)
    }

    pub fn get_change_date(
        &self,
        _identifier: &str,
    ) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
        Ok(None)
    }

    pub fn get_root_identifier(&self) -> &str {
        &self.branch
    }

    pub fn get_branches(&self) -> anyhow::Result<IndexMap<String, String>> {
        self.perforce.as_ref().unwrap().get_branches()
    }

    pub fn get_tags(&self) -> anyhow::Result<IndexMap<String, String>> {
        self.perforce.as_ref().unwrap().get_tags()
    }

    pub fn get_dist(&self, _identifier: &str) -> Option<IndexMap<String, PhpMixed>> {
        None
    }

    pub fn get_source(&self, identifier: &str) -> IndexMap<String, PhpMixed> {
        let mut source = IndexMap::new();
        source.insert("type".to_string(), PhpMixed::String("perforce".to_string()));
        source.insert(
            "url".to_string(),
            self.inner
                .repo_config
                .get("url")
                .cloned()
                .unwrap_or(PhpMixed::Null),
        );
        source.insert(
            "reference".to_string(),
            PhpMixed::String(identifier.to_string()),
        );
        source.insert(
            "p4user".to_string(),
            PhpMixed::String(self.perforce.as_ref().unwrap().get_user().to_string()),
        );
        source
    }

    pub fn get_url(&self) -> &str {
        &self.inner.url
    }

    pub fn has_composer_file(&self, identifier: &str) -> bool {
        let path = format!("//{}/{}", self.depot, identifier);
        self.perforce
            .as_ref()
            .unwrap()
            .get_composer_information(&path)
            .map_or(false, |info| !info.is_empty())
    }

    pub fn get_contents(&self, _url: &str) -> anyhow::Result<Response> {
        Err(BadMethodCallException {
            message: "Not implemented/used in PerforceDriver".to_string(),
            code: 0,
        }
        .into())
    }

    pub fn supports(io: &dyn IOInterface, config: &Config, url: &str, deep: bool) -> bool {
        if deep || Preg::is_match(r"#\b(perforce|p4)\b#i", url).unwrap_or(false) {
            return Perforce::check_server_exists(url, &ProcessExecutor::new(io));
        }
        false
    }

    pub fn cleanup(&mut self) -> anyhow::Result<()> {
        self.perforce.as_mut().unwrap().cleanup_client_spec()?;
        self.perforce = None;
        Ok(())
    }

    pub fn get_depot(&self) -> &str {
        &self.depot
    }

    pub fn get_branch(&self) -> &str {
        &self.branch
    }
}
