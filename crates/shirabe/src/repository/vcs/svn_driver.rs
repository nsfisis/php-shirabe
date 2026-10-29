//! ref: composer/src/Composer/Repository/Vcs/SvnDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::IOInterface;
use crate::json::{JsonEncodeOptions, JsonFile};
use crate::repository::vcs::VcsDriverBase;
use crate::util::{Filesystem, ProcessExecutor, Svn as SvnUtil, Url};
use bstr::{B, BStr, BString, ByteSlice};
use chrono::{DateTime, FixedOffset, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, PhpMixed, RuntimeException, bconcat, date_create, legacy_lossy, path_from_bytes,
    php_regex, preg_is_match, preg_match, preg_replace, rtrim2, stripos, strrpos, strtr, substr,
    trim, trim2,
};

#[derive(Debug)]
pub struct SvnDriver {
    inner: VcsDriverBase,
    base_url: BString,
    tags: Option<IndexMap<BString, BString>>,
    branches: Option<IndexMap<BString, BString>>,
    root_identifier: Option<BString>,

    trunk_path: Option<BString>,
    branches_path: BString,
    tags_path: BString,
    package_path: BString,
    cache_credentials: bool,

    util: Option<SvnUtil>,
}

impl SvnDriver {
    pub fn new(
        repo_config: IndexMap<BString, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
        process: std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    ) -> Self {
        Self {
            inner: VcsDriverBase::new(repo_config, io, config, http_downloader, process),
            base_url: BString::default(),
            tags: None,
            branches: None,
            root_identifier: None,
            trunk_path: Some(bstr::BString::from("trunk".to_string())),
            branches_path: bstr::BString::from("branches".to_string()),
            tags_path: bstr::BString::from("tags".to_string()),
            package_path: BString::default(),
            cache_credentials: true,
            util: None,
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<()> {
        let normalized = Self::normalize_url(self.inner.url.as_bstr());
        self.inner.url = rtrim2(&normalized, "/");
        self.base_url = self.inner.url.clone();

        SvnUtil::clean_env();

        match self.inner.repo_config.get(B("trunk-path")) {
            Some(PhpMixed::Bool(false)) => self.trunk_path = None,
            Some(PhpMixed::String(v)) => self.trunk_path = Some(v.clone()),
            _ => {}
        }
        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get(B("branches-path")).cloned() {
            self.branches_path = v.clone();
        }
        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get(B("tags-path")).cloned() {
            self.tags_path = v.clone();
        }
        if let Some(v) = self.inner.repo_config.get(B("svn-cache-credentials")) {
            self.cache_credentials = v.to_bool();
        }
        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get(B("package-path")).cloned() {
            self.package_path = bconcat!("/", trim2(&v, "/"));
        }

        if let Some(pos) = strrpos(
            &self.inner.url,
            bconcat!("/", self.trunk_path.clone().unwrap_or_default()),
        ) {
            self.base_url = BString::from(&substr(&self.inner.url, 0, Some(pos as i64))[..]);
        }

        self.inner.cache = Some(Cache::new(
            self.inner.io.clone(),
            format!(
                "{}/{}",
                legacy_lossy(
                    &self
                        .inner
                        .config
                        .borrow_mut()
                        .get(&legacy_lossy(b"cache-repo-dir".as_slice()))
                        .as_string()
                        .unwrap_or_default()
                ),
                legacy_lossy(&preg_replace(
                    r"{[^a-z0-9.]}i",
                    "-",
                    Url::sanitize(BString::from(&self.base_url.clone()[..]))
                )),
            ),
            None,
            None,
            false,
        ));
        self.inner.cache.as_mut().unwrap().set_read_only(
            self.inner
                .config
                .borrow_mut()
                .get(&legacy_lossy(b"cache-read-only".as_slice()))
                .as_bool()
                .unwrap_or(false),
        );

        self.get_branches()?;
        self.get_tags()?;
        Ok(())
    }

    pub fn get_root_identifier(&self) -> BString {
        self.root_identifier
            .clone()
            .unwrap_or_else(|| self.trunk_path.clone().unwrap_or_default())
    }

    pub fn get_url(&self) -> &BStr {
        self.inner.url.as_bstr()
    }

    pub fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        let mut m = IndexMap::new();
        m.insert(BString::from("type"), BString::from("svn"));
        m.insert(BString::from("url"), self.base_url.clone());
        m.insert(BString::from("reference"), BString::from(identifier));
        m
    }

    pub fn get_dist(&self, _identifier: &str) -> Option<IndexMap<BString, BString>> {
        None
    }

    fn should_cache(&self, identifier: &BStr) -> bool {
        self.inner.cache.is_some() && preg_is_match(php_regex!(r"{@\d+$}"), identifier)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        if !self.inner.info_cache.contains_key(identifier) {
            if self.should_cache(identifier)
                && let Some(mut res) = self
                    .inner
                    .cache
                    .as_mut()
                    .and_then(|c| c.read(&bconcat!(identifier, ".json")))
            {
                // old cache files had '' stored instead of null due to af3783b5f40bae32a23e353eaf0a00c9b8ce82e2, so we make sure here that we always return null or array
                // and fix outdated invalid cache files
                if res == "\"\"" {
                    res = bstr::BString::from("null".to_string());
                    self.inner
                        .cache
                        .as_mut()
                        .unwrap()
                        .write(&bconcat!(identifier, ".json"), &res)?;
                }

                let parsed = JsonFile::parse_json(Some(&res), None)?;
                let composer: Option<IndexMap<BString, PhpMixed>> = parsed.as_array().cloned();
                self.inner
                    .info_cache
                    .insert(bstr::BString::from(identifier), composer.clone());
                return Ok(composer);
            }

            let base_result = self
                .get_file_content(bstr::BStr::new("composer.json"), identifier)
                .and_then(|file_content| {
                    VcsDriverBase::finish_base_composer_information(
                        identifier,
                        file_content,
                        || self.get_change_date(identifier),
                    )
                });
            let composer: Option<IndexMap<BString, PhpMixed>> = match base_result {
                Ok(c) => c,
                Err(e) => {
                    // PHP catches only TransportException; other exceptions propagate uncaught.
                    if !e.is_instanceof::<TransportException>() {
                        return Err(e);
                    }
                    let message = e
                        .catch::<TransportException>()
                        .unwrap()
                        .get_message()
                        .to_string();
                    if stripos(&message, "path not found").is_none()
                        && stripos(&message, "svn: warning: W160013").is_none()
                    {
                        return Err(e);
                    }
                    // remember a not-existent composer.json
                    None
                }
            };

            if self.should_cache(identifier) {
                let encoded = JsonFile::encode_with_options(
                    &composer
                        .clone()
                        .map(PhpMixed::from)
                        .unwrap_or(PhpMixed::Null),
                    JsonEncodeOptions {
                        pretty_print: false,
                        ..Default::default()
                    },
                )?;
                self.inner
                    .cache
                    .as_mut()
                    .unwrap()
                    .write(&bconcat!(identifier, ".json"), &encoded)?;
            }

            self.inner
                .info_cache
                .insert(bstr::BString::from(identifier), composer);
        }

        // old cache files had '' stored instead of null due to af3783b5f40bae32a23e353eaf0a00c9b8ce82e2, so we make sure here that we always return null or array
        let cached = self
            .inner
            .info_cache
            .get(identifier)
            .and_then(|v| v.clone());
        Ok(cached)
    }

    pub fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        let identifier = bconcat!("/", trim2(identifier, "/"), "/");

        let (path, rev) = if let Some(m) = preg_match(php_regex!(r"{^(.+?)(@\d+)?/$}"), &identifier)
            && let Some(rev) = m.get(2)
        {
            (
                BString::from(m.get(1).unwrap_or_default()),
                BString::from(rev),
            )
        } else {
            (identifier, BString::default())
        };

        let output: BString = match self.execute(
            vec![
                bstr::BString::from("svn".to_string()),
                bstr::BString::from("cat".to_string()),
            ],
            bstr::BStr::new(&bconcat!(self.base_url, path, rev)),
        ) {
            Ok(o) => o,
            Err(e) => {
                if let Some(e) = e.catch::<RuntimeException>() {
                    return Err(TransportException::new(e.get_message().to_string(), 0).into());
                }
                return Err(e);
            }
        };
        if trim(&output).is_empty() {
            return Ok(None);
        }

        Ok(Some(output))
    }

    pub fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        let identifier = bconcat!("/", trim2(identifier, "/"), "/");

        let (path, rev) = if let Some(m) = preg_match(php_regex!(r"{^(.+?)(@\d+)?/$}"), &identifier)
            && let Some(rev) = m.get(2)
        {
            (
                BString::from(m.get(1).unwrap_or_default()),
                BString::from(rev),
            )
        } else {
            (identifier, BString::default())
        };

        let output = self.execute(
            vec![
                bstr::BString::from("svn".to_string()),
                bstr::BString::from("info".to_string()),
            ],
            bstr::BStr::new(&bconcat!(self.base_url, path, rev)),
        )?;
        for line in self.inner.process.borrow().split_lines(&output) {
            if !line.is_empty()
                && let Some(m) = preg_match(php_regex!(r"{^Last Changed Date: ([^(]+)}"), &line)
            {
                let date_str = BString::from(m.get(1).unwrap_or_default());
                return Ok(date_create::<Utc>(date_str.trim())
                    .ok()
                    .map(|d| d.fixed_offset()));
            }
        }

        Ok(None)
    }

    pub fn get_tags(&mut self) -> anyhow::Result<&IndexMap<BString, BString>> {
        if self.tags.is_none() {
            let mut tags: IndexMap<BString, BString> = IndexMap::new();

            // PHP: if ($this->tagsPath !== false) — tagsPath is "string"; treat empty string as false
            if !self.tags_path.is_empty() {
                let output = self.execute(
                    vec![
                        bstr::BString::from("svn".to_string()),
                        bstr::BString::from("ls".to_string()),
                        bstr::BString::from("--verbose".to_string()),
                    ],
                    bstr::BStr::new(&bconcat!(self.base_url, "/", self.tags_path)),
                )?;
                if !output.is_empty() {
                    let mut last_rev: i64 = 0;
                    for line in self.inner.process.borrow().split_lines(&output) {
                        let line = BString::from(&trim(&line)[..]);
                        if !line.is_empty()
                            && let Some(m) =
                                preg_match(php_regex!(r"{^\s*(\S+).*?(\S+)\s*$}"), &line)
                        {
                            let rev: i64 = m
                                .get(1)
                                .and_then(|s| legacy_lossy(s).parse().ok())
                                .unwrap_or(0);
                            let path = BString::from(m.get(2).unwrap_or_default());
                            if path == "./" {
                                last_rev = rev;
                            } else {
                                let identifier = self.build_identifier(
                                    bconcat!("/", self.tags_path, "/", path).as_bstr(),
                                    std::cmp::max(last_rev, rev),
                                );
                                tags.insert(rtrim2(&path, "/"), identifier);
                            }
                        }
                    }
                }
            }

            self.tags = Some(tags);
        }

        Ok(self.tags.as_ref().unwrap())
    }

    pub fn get_branches(&mut self) -> anyhow::Result<&IndexMap<BString, BString>> {
        if self.branches.is_none() {
            let mut branches: IndexMap<BString, BString> = IndexMap::new();

            let trunk_parent = if let Some(trunk_path) = self.trunk_path.as_ref() {
                bconcat!(self.base_url, "/", trunk_path)
            } else {
                bconcat!(self.base_url, "/")
            };

            let output = self.execute(
                vec![
                    bstr::BString::from("svn".to_string()),
                    bstr::BString::from("ls".to_string()),
                    bstr::BString::from("--verbose".to_string()),
                ],
                bstr::BStr::new(&trunk_parent),
            )?;
            if !output.is_empty() {
                for line in self.inner.process.borrow().split_lines(&output) {
                    let line = BString::from(&trim(&line)[..]);
                    if !line.is_empty()
                        && let Some(m) = preg_match(php_regex!(r"{^\s*(\S+).*?(\S+)\s*$}"), &line)
                    {
                        let rev: i64 = m
                            .get(1)
                            .and_then(|s| legacy_lossy(s).parse().ok())
                            .unwrap_or(0);
                        let path = BString::from(m.get(2).unwrap_or_default());
                        if path == "./" {
                            let identifier = self.build_identifier(
                                bconcat!("/", self.trunk_path.clone().unwrap_or_default())
                                    .as_bstr(),
                                rev,
                            );
                            branches.insert(
                                bstr::BString::from("trunk".to_string()),
                                identifier.clone(),
                            );
                            self.root_identifier = Some(identifier);
                            break;
                        }
                    }
                }
            }
            // PHP: unset($output);

            // PHP: if ($this->branchesPath !== false) — branchesPath is "string"; treat empty string as false
            if !self.branches_path.is_empty() {
                let output = self.execute(
                    vec![
                        bstr::BString::from("svn".to_string()),
                        bstr::BString::from("ls".to_string()),
                        bstr::BString::from("--verbose".to_string()),
                    ],
                    bstr::BStr::new(&bconcat!(self.base_url, "/", self.branches_path)),
                )?;
                if !output.is_empty() {
                    let mut last_rev: i64 = 0;
                    for line in self.inner.process.borrow().split_lines(trim(&output)) {
                        let line = BString::from(&trim(&line)[..]);
                        if !line.is_empty()
                            && let Some(m) =
                                preg_match(php_regex!(r"{^\s*(\S+).*?(\S+)\s*$}"), &line)
                        {
                            let rev: i64 = m
                                .get(1)
                                .and_then(|s| legacy_lossy(s).parse().ok())
                                .unwrap_or(0);
                            let path = BString::from(m.get(2).unwrap_or_default());
                            if path == "./" {
                                last_rev = rev;
                            } else {
                                let identifier = self.build_identifier(
                                    bconcat!("/", self.branches_path, "/", path).as_bstr(),
                                    std::cmp::max(last_rev, rev),
                                );
                                branches.insert(rtrim2(&path, "/"), identifier);
                            }
                        }
                    }
                }
            }

            self.branches = Some(branches);
        }

        Ok(self.branches.as_ref().unwrap())
    }

    pub fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        _config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &BStr,
        deep: bool,
    ) -> anyhow::Result<bool> {
        let url = legacy_lossy(&Self::normalize_url(url)).into_owned();
        if preg_is_match(php_regex!(r"#(^svn://|^svn\+ssh://|svn\.)#i"), &url) {
            return Ok(true);
        }

        // proceed with deep check for local urls since they are fast to process
        if !deep && !Filesystem::is_local_path(&url) {
            return Ok(false);
        }

        let process = ProcessExecutor::new(Some(io));
        let mut ignored_output = BString::default();
        let exit = process.execute(
            &[
                "svn".to_string(),
                "info".to_string(),
                "--non-interactive".to_string(),
                "--".to_string(),
                url,
            ],
            &mut ignored_output,
            None,
        )?;

        if exit == 0 {
            // This is definitely a Subversion repository.
            return Ok(true);
        }

        // Subversion client 1.7 and older
        if stripos(process.get_error_output(), "authorization failed:").is_some() {
            // This is likely a remote Subversion repository that requires
            // authentication. We will handle actual authentication later.
            return Ok(true);
        }

        // Subversion client 1.8 and newer
        if stripos(process.get_error_output(), "Authentication failed").is_some() {
            // This is likely a remote Subversion or newer repository that requires
            // authentication. We will handle actual authentication later.
            return Ok(true);
        }

        Ok(false)
    }

    /// An absolute path (leading '/') is converted to a file:// url.
    fn normalize_url(url: &BStr) -> BString {
        let fs = Filesystem::new(None);
        if fs.is_absolute_path(path_from_bytes(url)) {
            return bconcat!("file://", strtr(url, "\\", "/"));
        }

        bstr::BString::from(url)
    }

    /// Execute an SVN command and try to fix up the process with credentials
    /// if necessary.
    fn execute(&mut self, command: Vec<BString>, url: &BStr) -> anyhow::Result<BString> {
        if self.util.is_none() {
            self.util = Some(SvnUtil::new(
                self.base_url.clone(),
                self.inner.io.clone(),
                self.inner.config.clone(),
                Some(self.inner.process.clone()),
            ));
            self.util
                .as_mut()
                .unwrap()
                .set_cache_credentials(self.cache_credentials);
        }

        match self
            .util
            .as_mut()
            .unwrap()
            .execute(command, url, None, None, false)
        {
            Ok(o) => Ok(o),
            Err(e) => {
                if self.util.as_mut().unwrap().binary_version()?.is_none() {
                    return Err(RuntimeException::new(format!(
                        "Failed to load {}, svn was not found, check that it is installed and in your PATH env.\n\n{}",
                        self.inner.url.display_lossy(),
                        self.inner.process.borrow().get_error_output().display_lossy(),
                    ))
                    .into());
                }

                Err(RuntimeException::new(format!(
                    "Repository {} could not be processed, {}",
                    self.inner.url.display_lossy(),
                    e,
                ))
                .into())
            }
        }
    }

    /// Build the identifier respecting "package-path" config option
    fn build_identifier(&self, base_dir: &BStr, revision: i64) -> BString {
        bconcat!(rtrim2(base_dir, "/"), self.package_path, "/@", revision)
    }
}

impl crate::repository::vcs::VcsDriverInterface for SvnDriver {
    fn initialize(&mut self) -> anyhow::Result<()> {
        SvnDriver::initialize(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        SvnDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        SvnDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        SvnDriver::get_change_date(self, identifier)
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<BString> {
        Ok(SvnDriver::get_root_identifier(self))
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        Ok(self.get_branches()?.clone())
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        Ok(self.get_tags()?.clone())
    }

    fn get_dist(&self, identifier: &BStr) -> Option<IndexMap<BString, BString>> {
        SvnDriver::get_dist(self, &legacy_lossy(identifier))
    }

    fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        SvnDriver::get_source(self, identifier)
    }

    fn get_url(&self) -> BString {
        BString::from(SvnDriver::get_url(self))
    }

    fn has_composer_file(&mut self, identifier: &BStr) -> anyhow::Result<bool> {
        match self.get_composer_information(identifier) {
            Ok(info) => Ok(info.is_some()),
            Err(e) => {
                if e.is_instanceof::<TransportException>() {
                    Ok(false)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn cleanup(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &BStr,
        deep: bool,
    ) -> anyhow::Result<bool> {
        SvnDriver::supports(io, config, url, deep)
    }
}
