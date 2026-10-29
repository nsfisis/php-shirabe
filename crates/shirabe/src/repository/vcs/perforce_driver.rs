//! ref: composer/src/Composer/Repository/Vcs/PerforceDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::io::IOInterface;
use crate::repository::vcs::VcsDriverBase;
use crate::util::http::Response;
use crate::util::{Perforce, PerforceInterface, ProcessExecutor};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    BadMethodCallException, PhpMixed, RuntimeException, bconcat, path_from_bytes, php_regex,
    preg_is_match,
};

#[derive(Debug)]
pub struct PerforceDriver {
    inner: VcsDriverBase,
    depot: BString,
    branch: BString,
    perforce: Option<Box<dyn PerforceInterface>>,
}

impl PerforceDriver {
    pub fn new(
        repo_config: IndexMap<BString, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
        process: std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    ) -> Self {
        Self {
            inner: VcsDriverBase::new(repo_config, io, config, http_downloader, process),
            depot: BString::default(),
            branch: BString::default(),
            perforce: None,
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<()> {
        self.depot = BString::from(
            self.inner
                .repo_config
                .get(B("depot"))
                .and_then(|v| v.as_string())
                .unwrap_or_default(),
        );
        self.branch = BString::default();
        if let Some(branch) = self
            .inner
            .repo_config
            .get(B("branch"))
            .and_then(|v| v.as_string())
            && !branch.is_empty()
        {
            self.branch = BString::from(branch);
        }

        let repo_config = self.inner.repo_config.clone();
        self.init_perforce(&repo_config)?;
        self.perforce.as_mut().unwrap().p4_login()?;
        self.perforce.as_mut().unwrap().check_stream();
        self.perforce.as_mut().unwrap().write_p4_client_spec()?;
        self.perforce.as_mut().unwrap().connect_client()?;

        Ok(())
    }

    fn init_perforce(&mut self, repo_config: &IndexMap<BString, PhpMixed>) -> anyhow::Result<()> {
        if self.perforce.is_some() {
            return Ok(());
        }

        let cache_vcs_dir = BString::from(
            self.inner
                .config
                .borrow_mut()
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or_default(),
        );
        if !Cache::is_usable(path_from_bytes(&cache_vcs_dir)) {
            return Err(RuntimeException::new("PerforceDriver requires a usable cache directory, and it looks like you set it to be disabled".to_string()).into());
        }

        let repo_dir = path_from_bytes(&bconcat!(cache_vcs_dir, "/", self.depot)).to_path_buf();
        self.perforce = Some(Box::new(Perforce::create(
            repo_config.clone(),
            self.inner.url.clone(),
            repo_dir,
            self.inner.process.clone(),
            self.inner.io.clone(),
        )));

        Ok(())
    }

    /// For testing only. Rust equivalent of the reflection-based
    /// `overrideDriverInternalPerforce` helper in PerforceDriverTest.
    pub fn __override_perforce(&mut self, perforce: Box<dyn PerforceInterface>) {
        self.perforce = Some(perforce);
    }

    pub fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        Ok(self
            .perforce
            .as_mut()
            .unwrap()
            .get_file_content(file, identifier))
    }

    pub fn get_change_date(
        &self,
        _identifier: &BStr,
    ) -> anyhow::Result<Option<chrono::DateTime<chrono::FixedOffset>>> {
        Ok(None)
    }

    pub fn get_root_identifier(&self) -> &BStr {
        self.branch.as_bstr()
    }

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        Ok(self.perforce.as_mut().unwrap().get_branches())
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        Ok(self.perforce.as_mut().unwrap().get_tags())
    }

    pub fn get_dist(&self, _identifier: &BStr) -> Option<IndexMap<BString, PhpMixed>> {
        None
    }

    pub fn get_source(&self, identifier: &BStr) -> IndexMap<BString, PhpMixed> {
        let mut source = IndexMap::new();
        source.insert("type".into(), PhpMixed::string("perforce".to_string()));
        source.insert(
            "url".into(),
            self.inner
                .repo_config
                .get(B("url"))
                .cloned()
                .unwrap_or(PhpMixed::Null),
        );
        source.insert(
            "reference".into(),
            PhpMixed::string(BString::from(identifier)),
        );
        source.insert(
            "p4user".into(),
            PhpMixed::string(
                self.perforce
                    .as_ref()
                    .unwrap()
                    .get_user()
                    .unwrap_or_default(),
            ),
        );
        source
    }

    pub fn get_url(&self) -> &BStr {
        self.inner.url.as_bstr()
    }

    pub fn has_composer_file(&mut self, identifier: &BStr) -> bool {
        let path = bconcat!("//", self.depot, "/", identifier);
        self.perforce
            .as_mut()
            .unwrap()
            .get_composer_information(path.as_bstr())
            .is_ok_and(|info| info.is_some_and(|i| !i.is_empty()))
    }

    pub fn get_contents(&self, _url: &str) -> anyhow::Result<Response> {
        Err(
            BadMethodCallException::new("Not implemented/used in PerforceDriver".to_string())
                .into(),
        )
    }

    pub fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        _config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &BStr,
        deep: bool,
    ) -> anyhow::Result<bool> {
        if deep || preg_is_match(php_regex!(r"#\b(perforce|p4)\b#i"), url) {
            return Perforce::check_server_exists(url, &mut ProcessExecutor::new(Some(io)));
        }
        Ok(false)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        if let Some(cached) = self.inner.read_cached_composer(identifier)? {
            return Ok(cached);
        }

        let file_content = self.get_file_content(bstr::BStr::new("composer.json"), identifier)?;
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

    pub fn get_depot(&self) -> &BStr {
        self.depot.as_bstr()
    }

    pub fn get_branch(&self) -> &BStr {
        self.branch.as_bstr()
    }
}

impl crate::repository::vcs::VcsDriverInterface for PerforceDriver {
    fn initialize(&mut self) -> anyhow::Result<()> {
        PerforceDriver::initialize(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        PerforceDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        PerforceDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<chrono::DateTime<chrono::FixedOffset>>> {
        PerforceDriver::get_change_date(self, identifier)
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<BString> {
        Ok(PerforceDriver::get_root_identifier(self).to_owned())
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        PerforceDriver::get_branches(self)
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        PerforceDriver::get_tags(self)
    }

    fn get_dist(&self, identifier: &BStr) -> Option<IndexMap<BString, BString>> {
        PerforceDriver::get_dist(self, identifier).map(|m| {
            m.into_iter()
                .map(|(k, v)| (k.clone(), BString::from(v.as_string().unwrap_or_default())))
                .collect()
        })
    }

    fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        PerforceDriver::get_source(self, identifier)
            .into_iter()
            .map(|(k, v)| (k.clone(), BString::from(v.as_string().unwrap_or_default())))
            .collect()
    }

    fn get_url(&self) -> BString {
        bstr::BString::from(PerforceDriver::get_url(self))
    }

    fn has_composer_file(&mut self, identifier: &BStr) -> anyhow::Result<bool> {
        Ok(PerforceDriver::has_composer_file(self, identifier))
    }

    fn cleanup(&mut self) -> anyhow::Result<()> {
        PerforceDriver::cleanup(self)
    }

    fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &BStr,
        deep: bool,
    ) -> anyhow::Result<bool> {
        PerforceDriver::supports(io, config, url, deep)
    }
}
