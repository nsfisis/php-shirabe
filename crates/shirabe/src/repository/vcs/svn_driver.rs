//! ref: composer/src/Composer/Repository/Vcs/SvnDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::IOInterface;
use crate::json::JsonEncodeOptions;
use crate::json::JsonFile;
use crate::repository::vcs::VcsDriverBase;
use crate::util::Filesystem;
use crate::util::ProcessExecutor;
use crate::util::Svn as SvnUtil;
use crate::util::Url;
use chrono::{DateTime, FixedOffset, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{
    PhpMixed, RuntimeException, php_regex, stripos, strrpos, strtr, substr, trim,
};

#[derive(Debug)]
pub struct SvnDriver {
    pub(crate) inner: VcsDriverBase,
    /// @var string
    pub(crate) base_url: String,
    /// @var array<int|string, string> Map of tag name to identifier
    pub(crate) tags: Option<IndexMap<String, String>>,
    /// @var array<int|string, string> Map of branch name to identifier
    pub(crate) branches: Option<IndexMap<String, String>>,
    /// @var ?string
    pub(crate) root_identifier: Option<String>,

    pub(crate) trunk_path: Option<String>,
    /// @var string
    pub(crate) branches_path: String,
    /// @var string
    pub(crate) tags_path: String,
    /// @var string
    pub(crate) package_path: String,
    /// @var bool
    pub(crate) cache_credentials: bool,

    /// @var SvnUtil
    util: Option<SvnUtil>,
}

impl SvnDriver {
    pub fn new(
        repo_config: IndexMap<String, shirabe_php_shim::PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
        process: std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    ) -> Self {
        Self {
            inner: VcsDriverBase::new(repo_config, io, config, http_downloader, process),
            base_url: String::new(),
            tags: None,
            branches: None,
            root_identifier: None,
            trunk_path: Some("trunk".to_string()),
            branches_path: "branches".to_string(),
            tags_path: "tags".to_string(),
            package_path: String::new(),
            cache_credentials: true,
            util: None,
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<()> {
        let normalized = Self::normalize_url(&self.inner.url);
        self.inner.url = normalized.trim_end_matches('/').to_string();
        self.base_url = self.inner.url.clone();

        SvnUtil::clean_env();

        match self.inner.repo_config.get("trunk-path") {
            Some(PhpMixed::Bool(false)) => self.trunk_path = None,
            Some(PhpMixed::String(v)) => self.trunk_path = Some(v.clone()),
            _ => {}
        }
        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get("branches-path").cloned() {
            self.branches_path = v;
        }
        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get("tags-path").cloned() {
            self.tags_path = v;
        }
        if let Some(v) = self.inner.repo_config.get("svn-cache-credentials") {
            self.cache_credentials = v.to_bool();
        }
        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get("package-path").cloned() {
            self.package_path = format!("/{}", trim(&v, Some("/")));
        }

        if let Some(pos) = strrpos(
            &self.inner.url,
            &format!("/{}", self.trunk_path.as_deref().unwrap_or("")),
        ) {
            self.base_url = substr(&self.inner.url, 0, Some(pos as i64));
        }

        self.inner.cache = Some(Cache::new(
            self.inner.io.clone(),
            &format!(
                "{}/{}",
                self.inner
                    .config
                    .borrow_mut()
                    .get("cache-repo-dir")
                    .as_string()
                    .unwrap_or(""),
                Preg::replace(r"{[^a-z0-9.]}i", "-", &Url::sanitize(self.base_url.clone())),
            ),
            None,
            None,
            false,
        ));
        self.inner.cache.as_mut().unwrap().set_read_only(
            self.inner
                .config
                .borrow_mut()
                .get("cache-read-only")
                .as_bool()
                .unwrap_or(false),
        );

        self.get_branches()?;
        self.get_tags()?;
        Ok(())
    }

    pub fn get_root_identifier(&self) -> String {
        self.root_identifier
            .clone()
            .unwrap_or_else(|| self.trunk_path.clone().unwrap_or_default())
    }

    pub fn get_url(&self) -> &str {
        &self.inner.url
    }

    pub fn get_source(&self, identifier: &str) -> IndexMap<String, String> {
        let mut m = IndexMap::new();
        m.insert("type".to_string(), "svn".to_string());
        m.insert("url".to_string(), self.base_url.clone());
        m.insert("reference".to_string(), identifier.to_string());
        m
    }

    pub fn get_dist(&self, _identifier: &str) -> Option<IndexMap<String, String>> {
        None
    }

    pub(crate) fn should_cache(&self, identifier: &str) -> bool {
        self.inner.cache.is_some() && Preg::is_match(php_regex!(r"{@\d+$}"), identifier)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        if !self.inner.info_cache.contains_key(identifier) {
            if self.should_cache(identifier)
                && let Some(mut res) = self
                    .inner
                    .cache
                    .as_mut()
                    .and_then(|c| c.read(&format!("{}.json", identifier)))
            {
                // old cache files had '' stored instead of null due to af3783b5f40bae32a23e353eaf0a00c9b8ce82e2, so we make sure here that we always return null or array
                // and fix outdated invalid cache files
                if res == "\"\"" {
                    res = "null".to_string();
                    self.inner
                        .cache
                        .as_mut()
                        .unwrap()
                        .write(&format!("{}.json", identifier), &res)?;
                }

                let parsed = JsonFile::parse_json(Some(res.as_str()), None)?;
                let composer: Option<IndexMap<String, PhpMixed>> = parsed.as_array().cloned();
                self.inner
                    .info_cache
                    .insert(identifier.to_string(), composer.clone());
                return Ok(composer);
            }

            let base_result =
                self.get_file_content("composer.json", identifier)
                    .and_then(|file_content| {
                        VcsDriverBase::finish_base_composer_information(
                            identifier,
                            file_content,
                            || self.get_change_date(identifier),
                        )
                    });
            let composer: Option<IndexMap<String, PhpMixed>> = match base_result {
                Ok(c) => c,
                Err(e) => {
                    // PHP catches only TransportException; other exceptions propagate uncaught.
                    if e.downcast_ref::<TransportException>().is_none() {
                        return Err(e);
                    }
                    let message = e
                        .downcast_ref::<TransportException>()
                        .unwrap()
                        .message
                        .clone();
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
                );
                self.inner
                    .cache
                    .as_mut()
                    .unwrap()
                    .write(&format!("{}.json", identifier), &encoded)?;
            }

            self.inner
                .info_cache
                .insert(identifier.to_string(), composer);
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
        file: &str,
        identifier: &str,
    ) -> anyhow::Result<Option<String>> {
        let identifier = format!("/{}/", trim(identifier, Some("/")));

        let (path, rev) = if let Some(m) =
            Preg::is_match_with_indexed_captures(php_regex!(r"{^(.+?)(@\d+)?/$}"), &identifier)
        {
            if m.get(2).is_some() {
                (
                    m.get(1).cloned().unwrap_or_default(),
                    m.get(2).cloned().unwrap_or_default(),
                )
            } else {
                (identifier.clone(), String::new())
            }
        } else {
            (identifier.clone(), String::new())
        };

        let output: String = match self.execute(
            vec!["svn".to_string(), "cat".to_string()],
            &format!("{}{}{}", self.base_url, path, rev),
        ) {
            Ok(o) => o,
            Err(e) => {
                if let Some(e) = e.downcast_ref::<RuntimeException>() {
                    return Err(TransportException::new(e.message.clone(), 0).into());
                }
                return Err(e);
            }
        };
        if trim(&output, None).is_empty() {
            return Ok(None);
        }

        Ok(Some(output))
    }

    pub fn get_change_date(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        let identifier = format!("/{}/", trim(identifier, Some("/")));

        let (path, rev) = if let Some(m) =
            Preg::is_match_with_indexed_captures(php_regex!(r"{^(.+?)(@\d+)?/$}"), &identifier)
        {
            if m.get(2).is_some() {
                (
                    m.get(1).cloned().unwrap_or_default(),
                    m.get(2).cloned().unwrap_or_default(),
                )
            } else {
                (identifier.clone(), String::new())
            }
        } else {
            (identifier.clone(), String::new())
        };

        let output = self.execute(
            vec!["svn".to_string(), "info".to_string()],
            &format!("{}{}{}", self.base_url, path, rev),
        )?;
        for line in self.inner.process.borrow().split_lines(&output) {
            if !line.is_empty() {
                let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                if Preg::is_match3(
                    php_regex!(r"{^Last Changed Date: ([^(]+)}"),
                    &line,
                    Some(&mut m),
                ) {
                    let date_str = m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default();
                    return Ok(shirabe_php_shim::date_create::<Utc>(date_str.trim())
                        .ok()
                        .map(|d| d.fixed_offset()));
                }
            }
        }

        Ok(None)
    }

    pub fn get_tags(&mut self) -> anyhow::Result<&IndexMap<String, String>> {
        if self.tags.is_none() {
            let mut tags: IndexMap<String, String> = IndexMap::new();

            // PHP: if ($this->tagsPath !== false) — tagsPath is "string"; treat empty string as false
            if !self.tags_path.is_empty() {
                let output = self.execute(
                    vec!["svn".to_string(), "ls".to_string(), "--verbose".to_string()],
                    &format!("{}/{}", self.base_url, self.tags_path),
                )?;
                if !output.is_empty() {
                    let mut last_rev: i64 = 0;
                    for line in self.inner.process.borrow().split_lines(&output) {
                        let line = trim(&line, None);
                        if !line.is_empty() {
                            let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                            if Preg::is_match3(
                                php_regex!(r"{^\s*(\S+).*?(\S+)\s*$}"),
                                &line,
                                Some(&mut m),
                            ) {
                                let rev: i64 = m
                                    .get(&CaptureKey::ByIndex(1))
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(0);
                                let path =
                                    m.get(&CaptureKey::ByIndex(2)).cloned().unwrap_or_default();
                                if path == "./" {
                                    last_rev = rev;
                                } else {
                                    let identifier = self.build_identifier(
                                        &format!("/{}/{}", self.tags_path, path),
                                        std::cmp::max(last_rev, rev),
                                    );
                                    tags.insert(path.trim_end_matches('/').to_string(), identifier);
                                }
                            }
                        }
                    }
                }
            }

            self.tags = Some(tags);
        }

        Ok(self.tags.as_ref().unwrap())
    }

    pub fn get_branches(&mut self) -> anyhow::Result<&IndexMap<String, String>> {
        if self.branches.is_none() {
            let mut branches: IndexMap<String, String> = IndexMap::new();

            let trunk_parent = if let Some(trunk_path) = self.trunk_path.as_ref() {
                format!("{}/{}", self.base_url, trunk_path)
            } else {
                format!("{}/", self.base_url)
            };

            let output = self.execute(
                vec!["svn".to_string(), "ls".to_string(), "--verbose".to_string()],
                &trunk_parent,
            )?;
            if !output.is_empty() {
                for line in self.inner.process.borrow().split_lines(&output) {
                    let line = trim(&line, None);
                    if !line.is_empty() {
                        let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                        if Preg::is_match3(
                            php_regex!(r"{^\s*(\S+).*?(\S+)\s*$}"),
                            &line,
                            Some(&mut m),
                        ) {
                            let rev: i64 = m
                                .get(&CaptureKey::ByIndex(1))
                                .and_then(|s| s.parse().ok())
                                .unwrap_or(0);
                            let path = m.get(&CaptureKey::ByIndex(2)).cloned().unwrap_or_default();
                            if path == "./" {
                                let identifier = self.build_identifier(
                                    &format!("/{}", self.trunk_path.clone().unwrap_or_default()),
                                    rev,
                                );
                                branches.insert("trunk".to_string(), identifier.clone());
                                self.root_identifier = Some(identifier);
                                break;
                            }
                        }
                    }
                }
            }
            // PHP: unset($output);

            // PHP: if ($this->branchesPath !== false) — branchesPath is "string"; treat empty string as false
            if !self.branches_path.is_empty() {
                let output = self.execute(
                    vec!["svn".to_string(), "ls".to_string(), "--verbose".to_string()],
                    &format!("{}/{}", self.base_url, self.branches_path),
                )?;
                if !output.is_empty() {
                    let mut last_rev: i64 = 0;
                    for line in self
                        .inner
                        .process
                        .borrow()
                        .split_lines(&trim(&output, None))
                    {
                        let line = trim(&line, None);
                        if !line.is_empty() {
                            let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                            if Preg::is_match3(
                                php_regex!(r"{^\s*(\S+).*?(\S+)\s*$}"),
                                &line,
                                Some(&mut m),
                            ) {
                                let rev: i64 = m
                                    .get(&CaptureKey::ByIndex(1))
                                    .and_then(|s| s.parse().ok())
                                    .unwrap_or(0);
                                let path =
                                    m.get(&CaptureKey::ByIndex(2)).cloned().unwrap_or_default();
                                if path == "./" {
                                    last_rev = rev;
                                } else {
                                    let identifier = self.build_identifier(
                                        &format!("/{}/{}", self.branches_path, path),
                                        std::cmp::max(last_rev, rev),
                                    );
                                    branches
                                        .insert(path.trim_end_matches('/').to_string(), identifier);
                                }
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
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool> {
        let url = Self::normalize_url(url);
        if Preg::is_match(php_regex!(r"#(^svn://|^svn\+ssh://|svn\.)#i"), &url) {
            return Ok(true);
        }

        // proceed with deep check for local urls since they are fast to process
        if !deep && !Filesystem::is_local_path(&url) {
            return Ok(false);
        }

        let mut process = ProcessExecutor::new(Some(io));
        let mut ignored_output = String::new();
        let exit = process.execute_args(
            &[
                "svn".to_string(),
                "info".to_string(),
                "--non-interactive".to_string(),
                "--".to_string(),
                url.clone(),
            ],
            &mut ignored_output,
            None,
        );

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
    pub(crate) fn normalize_url(url: &str) -> String {
        let fs = Filesystem::new(None);
        if fs.is_absolute_path(url) {
            return format!("file://{}", strtr(url, "\\", "/"));
        }

        url.to_string()
    }

    /// Execute an SVN command and try to fix up the process with credentials
    /// if necessary.
    ///
    /// @param  non-empty-list<string> $command The svn command to run.
    /// @param  string            $url     The SVN URL.
    /// @throws \RuntimeException
    pub(crate) fn execute(&mut self, command: Vec<String>, url: &str) -> anyhow::Result<String> {
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
                if self.util.as_mut().unwrap().binary_version().is_none() {
                    return Err(RuntimeException {
                        message: format!(
                            "Failed to load {}, svn was not found, check that it is installed and in your PATH env.\n\n{}",
                            self.inner.url,
                            self.inner.process.borrow().get_error_output(),
                        ),
                        code: 0,
                    }
                    .into());
                }

                Err(RuntimeException {
                    message: format!(
                        "Repository {} could not be processed, {}",
                        self.inner.url, e,
                    ),
                    code: 0,
                }
                .into())
            }
        }
    }

    /// Build the identifier respecting "package-path" config option
    ///
    /// @param string $baseDir  The path to trunk/branch/tag
    /// @param int $revision The revision mark to add to identifier
    pub(crate) fn build_identifier(&self, base_dir: &str, revision: i64) -> String {
        format!(
            "{}{}/@{}",
            base_dir.trim_end_matches('/'),
            self.package_path,
            revision,
        )
    }
}

impl crate::repository::vcs::VcsDriverInterface for SvnDriver {
    fn initialize(&mut self) -> anyhow::Result<()> {
        SvnDriver::initialize(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        SvnDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(&mut self, file: &str, identifier: &str) -> anyhow::Result<Option<String>> {
        SvnDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        SvnDriver::get_change_date(self, identifier)
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<String> {
        Ok(SvnDriver::get_root_identifier(self))
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        Ok(self.get_branches()?.clone())
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        Ok(self.get_tags()?.clone())
    }

    fn get_dist(&self, identifier: &str) -> Option<IndexMap<String, String>> {
        SvnDriver::get_dist(self, identifier)
    }

    fn get_source(&self, identifier: &str) -> IndexMap<String, String> {
        SvnDriver::get_source(self, identifier)
    }

    fn get_url(&self) -> String {
        SvnDriver::get_url(self).to_string()
    }

    fn has_composer_file(&mut self, identifier: &str) -> anyhow::Result<bool> {
        match self.get_composer_information(identifier) {
            Ok(info) => Ok(info.is_some()),
            Err(e) => {
                if e.downcast_ref::<TransportException>().is_some() {
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
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool> {
        SvnDriver::supports(io, config, url, deep)
    }
}
