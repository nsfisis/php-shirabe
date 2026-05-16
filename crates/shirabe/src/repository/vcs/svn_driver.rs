//! ref: composer/src/Composer/Repository/Vcs/SvnDriver.php

use anyhow::Result;
use chrono::{DateTime, TimeZone, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    array_key_exists, is_array, max, sprintf, stripos, strrpos, strtr, substr, trim, PhpMixed,
    RuntimeException, JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::repository::vcs::vcs_driver::VcsDriver;
use crate::util::filesystem::Filesystem;
use crate::util::process_executor::ProcessExecutor;
use crate::util::svn::Svn as SvnUtil;
use crate::util::url::Url;

#[derive(Debug)]
pub struct SvnDriver {
    pub(crate) inner: VcsDriver,
    /// @var string
    pub(crate) base_url: String,
    /// @var array<int|string, string> Map of tag name to identifier
    pub(crate) tags: Option<IndexMap<String, String>>,
    /// @var array<int|string, string> Map of branch name to identifier
    pub(crate) branches: Option<IndexMap<String, String>>,
    /// @var ?string
    pub(crate) root_identifier: Option<String>,

    /// @var string|false
    // TODO(phase-b): PHP uses 'false' as a sentinel; model as Option<String>
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
    pub fn initialize(&mut self) -> Result<()> {
        let normalized = Self::normalize_url(&self.inner.url);
        self.inner.url = normalized
            .trim_end_matches('/')
            .to_string();
        self.base_url = self.inner.url.clone();

        SvnUtil::clean_env();

        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get("trunk-path").cloned() {
            self.trunk_path = Some(v);
        }
        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get("branches-path").cloned() {
            self.branches_path = v;
        }
        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get("tags-path").cloned() {
            self.tags_path = v;
        }
        if array_key_exists("svn-cache-credentials", &self.inner.repo_config) {
            self.cache_credentials = self
                .inner
                .repo_config
                .get("svn-cache-credentials")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        }
        if let Some(PhpMixed::String(v)) = self.inner.repo_config.get("package-path").cloned() {
            self.package_path = format!("/{}", trim(&v, Some("/")));
        }

        if let Some(trunk_path) = &self.trunk_path {
            if let Some(pos) = strrpos(&self.inner.url, &format!("/{}", trunk_path)) {
                self.base_url = substr(&self.inner.url, 0, Some(pos as i64));
            }
        }

        self.inner.cache = Some(Cache::new(
            // TODO(phase-b): pass io by reference/clone
            todo!("self.inner.io clone"),
            &format!(
                "{}/{}",
                self.inner.config.get("cache-repo-dir").as_string().unwrap_or(""),
                Preg::replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(self.base_url.clone())),
            ),
            None,
            None,
            false,
        ));
        self.inner
            .cache
            .as_mut()
            .unwrap()
            .set_read_only(
                self.inner
                    .config
                    .get("cache-read-only")
                    .as_bool()
                    .unwrap_or(false),
            );

        self.get_branches();
        self.get_tags();
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
        self.inner.cache.is_some() && Preg::is_match(r"{@\d+$}", identifier)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> Result<Option<IndexMap<String, PhpMixed>>> {
        if !self.inner.info_cache.contains_key(identifier) {
            if self.should_cache(identifier) {
                if let Some(mut res) = self
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

                    let parsed = JsonFile::parse_json(&res, None)?;
                    self.inner
                        .info_cache
                        .insert(identifier.to_string(), parsed.clone());
                    return Ok(parsed);
                }
            }

            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            let composer: Option<IndexMap<String, PhpMixed>> = match self
                .inner
                .get_base_composer_information(identifier)
            {
                Ok(c) => c,
                Err(e) => {
                    // TODO(phase-b): downcast to TransportException
                    let _te: &TransportException = todo!("downcast e to TransportException");
                    let message = e.to_string();
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
                let encoded = JsonFile::encode(
                    &composer
                        .clone()
                        .map(PhpMixed::from)
                        .unwrap_or(PhpMixed::Null),
                    JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES,
                    None,
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
        if cached.is_none()
            || !is_array(
                // TODO(phase-b): wrap IndexMap to PhpMixed for is_array check
                &cached
                    .clone()
                    .map(PhpMixed::from)
                    .unwrap_or(PhpMixed::Null),
            )
        {
            return Ok(None);
        }

        Ok(cached)
    }

    pub fn get_file_content(
        &mut self,
        file: &str,
        identifier: &str,
    ) -> Result<Option<String>> {
        let identifier = format!("/{}/", trim(identifier, Some("/")));

        let (path, rev) = if let Ok(Some(m)) =
            Preg::is_match_with_indexed_captures(r"{^(.+?)(@\d+)?/$}", &identifier)
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

        // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
        let output: String = match self.execute(
            vec!["svn".to_string(), "cat".to_string()],
            &format!("{}{}{}", self.base_url, path, rev),
        ) {
            Ok(o) => o,
            Err(e) => {
                return Err(TransportException::new(e.to_string(), 0).into());
            }
        };
        if trim(&output, None) == "" {
            return Ok(None);
        }

        Ok(Some(output))
    }

    pub fn get_change_date(&mut self, identifier: &str) -> Result<Option<DateTime<Utc>>> {
        let identifier = format!("/{}/", trim(identifier, Some("/")));

        let (path, rev) = if let Ok(Some(m)) =
            Preg::is_match_with_indexed_captures(r"{^(.+?)(@\d+)?/$}", &identifier)
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
        for line in self.inner.process.split_lines(&output) {
            if !line.is_empty() {
                if let Some(m) = Preg::is_match_strict_groups(
                    r"{^Last Changed Date: ([^(]+)}",
                    &line,
                ) {
                    let date_str = m.get(1).cloned().unwrap_or_default();
                    // PHP: new \DateTimeImmutable($match[1], new \DateTimeZone('UTC'))
                    return Ok(Utc
                        .datetime_from_str(date_str.trim(), "%Y-%m-%d %H:%M:%S %z")
                        .ok());
                }
            }
        }

        Ok(None)
    }

    pub fn get_tags(&mut self) -> &IndexMap<String, String> {
        if self.tags.is_none() {
            let mut tags: IndexMap<String, String> = IndexMap::new();

            // PHP: if ($this->tagsPath !== false) — tagsPath is "string"; treat empty string as false
            if !self.tags_path.is_empty() {
                let output = self.execute(
                    vec![
                        "svn".to_string(),
                        "ls".to_string(),
                        "--verbose".to_string(),
                    ],
                    &format!("{}/{}", self.base_url, self.tags_path),
                ).unwrap_or_default();
                if !output.is_empty() {
                    let mut last_rev: i64 = 0;
                    for line in self.inner.process.split_lines(&output) {
                        let line = trim(&line, None);
                        if !line.is_empty() {
                            if let Some(m) = Preg::is_match_strict_groups(
                                r"{^\s*(\S+).*?(\S+)\s*$}",
                                &line,
                            ) {
                                let rev: i64 = m.get(1).map(|s| s.parse().unwrap_or(0)).unwrap_or(0);
                                let path = m.get(2).cloned().unwrap_or_default();
                                if path == "./" {
                                    last_rev = rev;
                                } else {
                                    let identifier = self.build_identifier(
                                        &format!("/{}/{}", self.tags_path, path),
                                        max(last_rev, rev),
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

        self.tags.as_ref().unwrap()
    }

    pub fn get_branches(&mut self) -> &IndexMap<String, String> {
        if self.branches.is_none() {
            let mut branches: IndexMap<String, String> = IndexMap::new();

            let trunk_parent = if self.trunk_path.is_none() {
                format!("{}/", self.base_url)
            } else {
                format!("{}/{}", self.base_url, self.trunk_path.as_ref().unwrap())
            };

            let output = self
                .execute(
                    vec![
                        "svn".to_string(),
                        "ls".to_string(),
                        "--verbose".to_string(),
                    ],
                    &trunk_parent,
                )
                .unwrap_or_default();
            if !output.is_empty() {
                for line in self.inner.process.split_lines(&output) {
                    let line = trim(&line, None);
                    if !line.is_empty() {
                        if let Some(m) = Preg::is_match_strict_groups(
                            r"{^\s*(\S+).*?(\S+)\s*$}",
                            &line,
                        ) {
                            let rev: i64 = m.get(1).map(|s| s.parse().unwrap_or(0)).unwrap_or(0);
                            let path = m.get(2).cloned().unwrap_or_default();
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
                let output = self
                    .execute(
                        vec![
                            "svn".to_string(),
                            "ls".to_string(),
                            "--verbose".to_string(),
                        ],
                        &format!("{}/{}", self.base_url, self.branches_path),
                    )
                    .unwrap_or_default();
                if !output.is_empty() {
                    let mut last_rev: i64 = 0;
                    for line in self.inner.process.split_lines(&trim(&output, None)) {
                        let line = trim(&line, None);
                        if !line.is_empty() {
                            if let Some(m) = Preg::is_match_strict_groups(
                                r"{^\s*(\S+).*?(\S+)\s*$}",
                                &line,
                            ) {
                                let rev: i64 =
                                    m.get(1).map(|s| s.parse().unwrap_or(0)).unwrap_or(0);
                                let path = m.get(2).cloned().unwrap_or_default();
                                if path == "./" {
                                    last_rev = rev;
                                } else {
                                    let identifier = self.build_identifier(
                                        &format!("/{}/{}", self.branches_path, path),
                                        max(last_rev, rev),
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

        self.branches.as_ref().unwrap()
    }

    pub fn supports(io: &dyn IOInterface, _config: &Config, url: &str, deep: bool) -> bool {
        let url = Self::normalize_url(url);
        if Preg::is_match(r"#(^svn://|^svn\+ssh://|svn\.)#i", &url) {
            return true;
        }

        // proceed with deep check for local urls since they are fast to process
        if !deep && !Filesystem::is_local_path(&url) {
            return false;
        }

        let mut process = ProcessExecutor::new(io);
        let mut ignored_output = String::new();
        let exit = process.execute(
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
            return true;
        }

        // Subversion client 1.7 and older
        if stripos(&process.get_error_output(), "authorization failed:").is_some() {
            // This is likely a remote Subversion repository that requires
            // authentication. We will handle actual authentication later.
            return true;
        }

        // Subversion client 1.8 and newer
        if stripos(&process.get_error_output(), "Authentication failed").is_some() {
            // This is likely a remote Subversion or newer repository that requires
            // authentication. We will handle actual authentication later.
            return true;
        }

        false
    }

    /// An absolute path (leading '/') is converted to a file:// url.
    pub(crate) fn normalize_url(url: &str) -> String {
        let fs = Filesystem::new();
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
    pub(crate) fn execute(&mut self, command: Vec<String>, url: &str) -> Result<String> {
        if self.util.is_none() {
            self.util = Some(SvnUtil::new(
                self.base_url.clone(),
                // TODO(phase-b): clone or borrow io/config
                todo!("self.inner.io clone"),
                todo!("self.inner.config clone"),
                Some(todo!("self.inner.process clone")),
            ));
            self.util
                .as_mut()
                .unwrap()
                .set_cache_credentials(self.cache_credentials);
        }

        // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
        match self.util.as_mut().unwrap().execute(command, url, None, None, false) {
            Ok(o) => Ok(o),
            Err(e) => {
                if self.util.as_mut().unwrap().binary_version().is_none() {
                    return Err(RuntimeException {
                        message: format!(
                            "Failed to load {}, svn was not found, check that it is installed and in your PATH env.\n\n{}",
                            self.inner.url,
                            self.inner.process.get_error_output(),
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
