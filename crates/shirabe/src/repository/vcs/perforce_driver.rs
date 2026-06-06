//! ref: composer/src/Composer/Repository/Vcs/PerforceDriver.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{BadMethodCallException, PhpMixed, RuntimeException};

use crate::cache::Cache;
use crate::config::Config;
use crate::io::IOInterface;
use crate::repository::vcs::VcsDriverBase;
use crate::util::Perforce;
use crate::util::ProcessExecutor;
use crate::util::http::Response;

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
        self.perforce.as_mut().unwrap().check_stream();
        self.perforce.as_mut().unwrap().write_p4_client_spec()?;
        self.perforce.as_mut().unwrap().connect_client();

        Ok(())
    }

    fn init_perforce(&mut self, repo_config: &IndexMap<String, PhpMixed>) -> anyhow::Result<()> {
        if self.perforce.is_some() {
            return Ok(());
        }

        let cache_vcs_dir = self
            .inner
            .config
            .borrow_mut()
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
            repo_config.clone(),
            self.inner.url.clone(),
            repo_dir,
            self.inner.process.clone(),
            self.inner.io.clone(),
        ));

        Ok(())
    }

    pub fn get_file_content(
        &mut self,
        file: &str,
        identifier: &str,
    ) -> anyhow::Result<Option<String>> {
        Ok(self
            .perforce
            .as_mut()
            .unwrap()
            .get_file_content(file, identifier))
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

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        Ok(self.perforce.as_mut().unwrap().get_branches())
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        Ok(self.perforce.as_mut().unwrap().get_tags())
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
            PhpMixed::String(
                self.perforce
                    .as_ref()
                    .unwrap()
                    .get_user()
                    .unwrap_or_default(),
            ),
        );
        source
    }

    pub fn get_url(&self) -> &str {
        &self.inner.url
    }

    pub fn has_composer_file(&mut self, identifier: &str) -> bool {
        let path = format!("//{}/{}", self.depot, identifier);
        self.perforce
            .as_mut()
            .unwrap()
            .get_composer_information(&path)
            .map_or(false, |info| info.map_or(false, |i| !i.is_empty()))
    }

    pub fn get_contents(&self, _url: &str) -> anyhow::Result<Response> {
        Err(BadMethodCallException {
            message: "Not implemented/used in PerforceDriver".to_string(),
            code: 0,
        }
        .into())
    }

    pub fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        _config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool> {
        if deep || Preg::is_match(r"#\b(perforce|p4)\b#i", url).unwrap_or(false) {
            return Ok(Perforce::check_server_exists(
                url,
                &mut ProcessExecutor::new(Some(io)),
            ));
        }
        Ok(false)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        if let Some(cached) = self.inner.read_cached_composer(identifier)? {
            return Ok(cached);
        }

        let file_content = self.get_file_content("composer.json", identifier)?;
        let composer =
            VcsDriverBase::finish_base_composer_information(identifier, file_content, || {
                self.get_change_date(identifier)
            })?;

        self.inner.write_cached_composer(identifier, composer)
    }

    pub fn cleanup(&mut self) -> anyhow::Result<()> {
        self.perforce.as_mut().unwrap().cleanup_client_spec();
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

impl crate::repository::vcs::VcsDriverInterface for PerforceDriver {
    fn initialize(&mut self) -> anyhow::Result<()> {
        PerforceDriver::initialize(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        PerforceDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(&mut self, file: &str, identifier: &str) -> anyhow::Result<Option<String>> {
        PerforceDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
        PerforceDriver::get_change_date(self, identifier)
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<String> {
        Ok(PerforceDriver::get_root_identifier(self).to_string())
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        PerforceDriver::get_branches(self)
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        PerforceDriver::get_tags(self)
    }

    fn get_dist(&self, identifier: &str) -> anyhow::Result<Option<IndexMap<String, String>>> {
        Ok(PerforceDriver::get_dist(self, identifier).map(|m| {
            m.into_iter()
                .map(|(k, v)| (k, v.as_string().unwrap_or("").to_string()))
                .collect()
        }))
    }

    fn get_source(&self, identifier: &str) -> anyhow::Result<IndexMap<String, String>> {
        Ok(PerforceDriver::get_source(self, identifier)
            .into_iter()
            .map(|(k, v)| (k, v.as_string().unwrap_or("").to_string()))
            .collect())
    }

    fn get_url(&self) -> String {
        PerforceDriver::get_url(self).to_string()
    }

    fn has_composer_file(&mut self, identifier: &str) -> anyhow::Result<bool> {
        Ok(PerforceDriver::has_composer_file(self, identifier))
    }

    fn cleanup(&mut self) -> anyhow::Result<()> {
        PerforceDriver::cleanup(self)
    }

    fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool> {
        PerforceDriver::supports(io, config, url, deep)
    }
}
