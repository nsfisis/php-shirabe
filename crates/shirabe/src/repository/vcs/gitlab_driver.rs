//! ref: composer/src/Composer/Repository/Vcs/GitLabDriver.php

use anyhow::Result;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    array_search_mixed, array_shift, ctype_alnum, empty, explode, extension_loaded, implode,
    in_array, is_array, is_string, ord, sprintf, strpos, strtolower, InvalidArgumentException,
    LogicException, PhpMixed, RuntimeException,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::repository::vcs::git_driver::GitDriver;
use crate::repository::vcs::vcs_driver::VcsDriver;
use crate::util::gitlab::GitLab;
use crate::util::http::response::Response;
use crate::util::http_downloader::HttpDownloader;

/// Driver for GitLab API, use the Git driver for local checkouts.
#[derive(Debug)]
pub struct GitLabDriver {
    pub(crate) inner: VcsDriver,
    /// @phpstan-var 'https'|'http'
    scheme: String,
    namespace: String,
    repository: String,
    /// @var mixed[] Project data returned by GitLab API
    project: Option<IndexMap<String, PhpMixed>>,
    /// @var array<string|int, mixed[]> Keeps commits returned by GitLab API as commit id => info
    commits: IndexMap<String, IndexMap<String, PhpMixed>>,
    /// @var array<int|string, string> Map of tag name to identifier
    tags: Option<IndexMap<String, String>>,
    /// @var array<int|string, string> Map of branch name to identifier
    branches: Option<IndexMap<String, String>>,
    /// Git Driver
    pub(crate) git_driver: Option<GitDriver>,
    /// Protocol to force use of for repository URLs.
    /// @var string One of ssh, http
    pub(crate) protocol: String,
    /// Defaults to true unless we can make sure it is public
    /// @var bool defines whether the repo is private or not
    is_private: bool,
    /// @var bool true if the origin has a port number or a path component in it
    has_nonstandard_origin: bool,
}

impl GitLabDriver {
    pub const URL_REGEX: &'static str = r##"#^(?:(?P<scheme>https?)://(?P<domain>.+?)(?::(?P<port>[0-9]+))?/|git@(?P<domain2>[^:]+):)(?P<parts>.+)/(?P<repo>[^/]+?)(?:\.git|/)?$#"##;

    /// Extracts information from the repository url.
    ///
    /// SSH urls use https by default. Set "secure-http": false on the repository config to use http instead.
    pub fn initialize(&mut self) -> Result<()> {
        let match_ = match Preg::is_match_strict_groups(Self::URL_REGEX, &self.inner.url) {
            Some(m) => m,
            None => {
                return Err(InvalidArgumentException {
                    message: sprintf(
                        "The GitLab repository URL %s is invalid. It must be the HTTP URL of a GitLab project.",
                        &[PhpMixed::String(self.inner.url.clone())],
                    ),
                    code: 0,
                }
                .into());
            }
        };

        let guessed_domain = match_
            .get("domain")
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| match_.get("domain2").cloned().unwrap_or_default());
        let configured_domains = self.inner.config.get("gitlab-domains");
        let mut url_parts: Vec<String> = explode("/", &match_.get("parts").cloned().unwrap_or_default());

        let scheme_match = match_.get("scheme").cloned().unwrap_or_default();
        self.scheme = if in_array(
            PhpMixed::String(scheme_match.clone()),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::String("https".to_string())),
                Box::new(PhpMixed::String("http".to_string())),
            ]),
            true,
        ) {
            scheme_match
        } else if self
            .inner
            .repo_config
            .get("secure-http")
            .and_then(|v| v.as_bool())
            == Some(false)
        {
            "http".to_string()
        } else {
            "https".to_string()
        };
        let port = match_.get("port").cloned();
        let origin = Self::determine_origin(
            &configured_domains,
            guessed_domain,
            &mut url_parts,
            port.clone(),
        );
        let origin = match origin {
            Some(o) => o,
            None => {
                return Err(LogicException {
                    message: format!(
                        "It should not be possible to create a gitlab driver with an unparsable origin URL ({})",
                        self.inner.url
                    ),
                    code: 0,
                }
                .into());
            }
        };
        self.inner.origin_url = origin;

        let protocol_value = self.inner.config.get("gitlab-protocol");
        if let Some(protocol) = protocol_value.as_string().filter(|_| is_string(&protocol_value)) {
            // https treated as a synonym for http.
            if !in_array(
                PhpMixed::String(protocol.to_string()),
                &PhpMixed::List(vec![
                    Box::new(PhpMixed::String("git".to_string())),
                    Box::new(PhpMixed::String("http".to_string())),
                    Box::new(PhpMixed::String("https".to_string())),
                ]),
                true,
            ) {
                return Err(RuntimeException {
                    message: "gitlab-protocol must be one of git, http.".to_string(),
                    code: 0,
                }
                .into());
            }
            self.protocol = if protocol == "git" {
                "ssh".to_string()
            } else {
                "http".to_string()
            };
        }

        if strpos(&self.inner.origin_url, ":").is_some()
            || strpos(&self.inner.origin_url, "/").is_some()
        {
            self.has_nonstandard_origin = true;
        }

        self.namespace = implode("/", &url_parts);
        self.repository = Preg::replace(
            r"#(\.git)$#",
            "",
            match_.get("repo").cloned().unwrap_or_default(),
        );

        self.inner.cache = Some(Cache::new(
            self.inner.io.as_ref(),
            &format!(
                "{}/{}/{}/{}",
                self.inner
                    .config
                    .get("cache-repo-dir")
                    .as_string()
                    .unwrap_or(""),
                self.inner.origin_url,
                self.namespace,
                self.repository,
            ),
            None,
            None,
            false,
        ));
        self.inner.cache.as_mut().map(|c| {
            c.set_read_only(
                self.inner
                    .config
                    .get("cache-read-only")
                    .as_bool()
                    .unwrap_or(false),
            )
        });

        self.fetch_project()?;

        Ok(())
    }

    /// Updates the HttpDownloader instance.
    /// Mainly useful for tests.
    ///
    /// @internal
    pub fn set_http_downloader(&mut self, http_downloader: HttpDownloader) {
        self.inner.http_downloader = http_downloader;
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> Result<Option<IndexMap<String, PhpMixed>>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_composer_information(identifier);
        }

        if !self.inner.info_cache.contains_key(identifier) {
            let composer = if self.inner.should_cache(identifier)
                && self
                    .inner
                    .cache
                    .as_ref()
                    .and_then(|c| c.read(identifier))
                    .is_some()
            {
                let res = self
                    .inner
                    .cache
                    .as_ref()
                    .and_then(|c| c.read(identifier))
                    .unwrap_or_default();
                JsonFile::parse_json(&res, None)?
            } else {
                let composer = self.inner.get_base_composer_information(identifier)?;

                if self.inner.should_cache(identifier) {
                    if let Some(ref composer_map) = composer {
                        self.inner.cache.as_ref().map(|c| {
                            c.write(
                                identifier,
                                &JsonFile::encode_with_options(
                                    composer_map,
                                    shirabe_php_shim::JSON_UNESCAPED_UNICODE
                                        | shirabe_php_shim::JSON_UNESCAPED_SLASHES,
                                ),
                            )
                        });
                    }
                }

                composer
            };

            let mut composer = composer;
            if let Some(ref mut composer) = composer {
                // specials for gitlab (this data is only available if authentication is provided)
                if composer.contains_key("support")
                    && !is_array(composer.get("support").cloned().unwrap_or(PhpMixed::Null))
                {
                    composer.insert(
                        "support".to_string(),
                        PhpMixed::Array(IndexMap::new()),
                    );
                }
                let project = self.project.clone().unwrap_or_default();
                let has_web_url = project.contains_key("web_url");
                let support_source_missing = !composer
                    .get("support")
                    .and_then(|v| v.as_array())
                    .map(|m| m.contains_key("source"))
                    .unwrap_or(false);
                if support_source_missing && has_web_url {
                    let label = array_search_mixed(
                        &PhpMixed::String(identifier.to_string()),
                        &PhpMixed::Array(
                            self.get_tags()?
                                .into_iter()
                                .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                                .collect(),
                        ),
                        true,
                    )
                    .filter(|v| !matches!(v, PhpMixed::Bool(false) | PhpMixed::Null))
                    .or_else(|| {
                        array_search_mixed(
                            &PhpMixed::String(identifier.to_string()),
                            &PhpMixed::Array(
                                self.get_branches().unwrap_or_default()
                                    .into_iter()
                                    .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                                    .collect(),
                            ),
                            true,
                        )
                    })
                    .filter(|v| !matches!(v, PhpMixed::Bool(false) | PhpMixed::Null))
                    .unwrap_or_else(|| PhpMixed::String(identifier.to_string()));
                    let label_str = label.as_string().unwrap_or(identifier).to_string();
                    let web_url = project
                        .get("web_url")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    if let Some(support) = composer.get_mut("support").and_then(|v| {
                        match v {
                            PhpMixed::Array(m) => Some(m),
                            _ => None,
                        }
                    }) {
                        support.insert(
                            "source".to_string(),
                            Box::new(PhpMixed::String(sprintf(
                                "%s/-/tree/%s",
                                &[
                                    PhpMixed::String(web_url),
                                    PhpMixed::String(label_str),
                                ],
                            ))),
                        );
                    }
                }
                let issues_missing = !composer
                    .get("support")
                    .and_then(|v| v.as_array())
                    .map(|m| m.contains_key("issues"))
                    .unwrap_or(false);
                let issues_enabled = !empty(
                    &project
                        .get("issues_enabled")
                        .cloned()
                        .unwrap_or(PhpMixed::Null),
                );
                if issues_missing && issues_enabled && has_web_url {
                    let web_url = project
                        .get("web_url")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    if let Some(support) = composer.get_mut("support").and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m),
                        _ => None,
                    }) {
                        support.insert(
                            "issues".to_string(),
                            Box::new(PhpMixed::String(sprintf(
                                "%s/-/issues",
                                &[PhpMixed::String(web_url)],
                            ))),
                        );
                    }
                }
                if !composer.contains_key("abandoned")
                    && !empty(
                        &project
                            .get("archived")
                            .cloned()
                            .unwrap_or(PhpMixed::Null),
                    )
                {
                    composer.insert("abandoned".to_string(), PhpMixed::Bool(true));
                }
            }

            self.inner
                .info_cache
                .insert(identifier.to_string(), composer);
        }

        Ok(self
            .inner
            .info_cache
            .get(identifier)
            .cloned()
            .unwrap_or(None))
    }

    pub fn get_file_content(&mut self, file: &str, identifier: &str) -> Result<Option<String>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_file_content(file, identifier);
        }

        // Convert the root identifier to a cacheable commit id
        let mut identifier = identifier.to_string();
        if !Preg::is_match(r"{[a-f0-9]{40}}i", &identifier).unwrap_or(false) {
            let branches = self.get_branches()?;
            if let Some(sha) = branches.get(&identifier) {
                identifier = sha.clone();
            }
        }

        let resource = format!(
            "{}/repository/files/{}/raw?ref={}",
            self.get_api_url(),
            self.url_encode_all(file),
            identifier,
        );

        let content = match self.get_contents(&resource, false) {
            Ok(response) => response.get_body().map(|s| s.to_string()),
            Err(e) => {
                if e.code != 404 {
                    return Err(e.into());
                }

                return Ok(None);
            }
        };

        Ok(content)
    }

    pub fn get_change_date(&mut self, identifier: &str) -> Result<Option<DateTime<Utc>>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_change_date(identifier);
        }

        if let Some(commit) = self.commits.get(identifier) {
            let committed_date = commit
                .get("committed_date")
                .and_then(|v| v.as_string())
                .unwrap_or("");
            return Ok(Some(
                DateTime::parse_from_rfc3339(committed_date)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            ));
        }

        Ok(None)
    }

    pub fn get_repository_url(&self) -> String {
        let project = self.project.clone().unwrap_or_default();
        if !self.protocol.is_empty() {
            return project
                .get(&format!("{}_url_to_repo", self.protocol))
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
        }

        if self.is_private {
            project
                .get("ssh_url_to_repo")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string()
        } else {
            project
                .get("http_url_to_repo")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string()
        }
    }

    pub fn get_url(&self) -> String {
        if let Some(ref git_driver) = self.git_driver {
            return git_driver.get_url();
        }

        self.project
            .as_ref()
            .and_then(|p| p.get("web_url"))
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string()
    }

    pub fn get_dist(&self, identifier: &str) -> Option<IndexMap<String, PhpMixed>> {
        let url = format!(
            "{}/repository/archive.zip?sha={}",
            self.get_api_url(),
            identifier
        );

        let mut result = IndexMap::new();
        result.insert("type".to_string(), PhpMixed::String("zip".to_string()));
        result.insert("url".to_string(), PhpMixed::String(url));
        result.insert(
            "reference".to_string(),
            PhpMixed::String(identifier.to_string()),
        );
        result.insert("shasum".to_string(), PhpMixed::String(String::new()));
        Some(result)
    }

    pub fn get_source(&self, identifier: &str) -> IndexMap<String, PhpMixed> {
        if let Some(ref git_driver) = self.git_driver {
            return git_driver.get_source(identifier);
        }

        let mut result = IndexMap::new();
        result.insert("type".to_string(), PhpMixed::String("git".to_string()));
        result.insert(
            "url".to_string(),
            PhpMixed::String(self.get_repository_url()),
        );
        result.insert(
            "reference".to_string(),
            PhpMixed::String(identifier.to_string()),
        );
        result
    }

    pub fn get_root_identifier(&mut self) -> Result<String> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_root_identifier();
        }

        Ok(self
            .project
            .as_ref()
            .and_then(|p| p.get("default_branch"))
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string())
    }

    pub fn get_branches(&mut self) -> Result<IndexMap<String, String>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_branches();
        }

        if self.branches.is_none() {
            self.branches = Some(self.get_references("branches")?);
        }

        Ok(self.branches.clone().unwrap_or_default())
    }

    pub fn get_tags(&mut self) -> Result<IndexMap<String, String>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_tags();
        }

        if self.tags.is_none() {
            self.tags = Some(self.get_references("tags")?);
        }

        Ok(self.tags.clone().unwrap_or_default())
    }

    /// @return string Base URL for GitLab API v3
    pub fn get_api_url(&self) -> String {
        format!(
            "{}://{}/api/v4/projects/{}%2F{}",
            self.scheme,
            self.inner.origin_url,
            self.url_encode_all(&self.namespace),
            self.url_encode_all(&self.repository),
        )
    }

    /// Urlencode all non alphanumeric characters. rawurlencode() can not be used as it does not encode `.`
    fn url_encode_all(&self, string: &str) -> String {
        let mut encoded = String::new();
        let bytes: Vec<char> = string.chars().collect();
        for i in 0..bytes.len() {
            let character = bytes[i].to_string();
            let final_character = if !ctype_alnum(&character)
                && !in_array(
                    PhpMixed::String(character.clone()),
                    &PhpMixed::List(vec![
                        Box::new(PhpMixed::String("-".to_string())),
                        Box::new(PhpMixed::String("_".to_string())),
                    ]),
                    true,
                )
            {
                format!(
                    "%{}",
                    sprintf("%02X", &[PhpMixed::Int(ord(&character))])
                )
            } else {
                character
            };
            encoded.push_str(&final_character);
        }

        encoded
    }

    /// @return string[] where keys are named references like tags or branches and the value a sha
    pub(crate) fn get_references(&mut self, r#type: &str) -> Result<IndexMap<String, String>> {
        let per_page = 100;
        let mut resource: Option<String> = Some(format!(
            "{}/repository/{}?per_page={}",
            self.get_api_url(),
            r#type,
            per_page
        ));

        let mut references: IndexMap<String, String> = IndexMap::new();
        loop {
            let response = self
                .get_contents(resource.as_deref().unwrap_or(""), false)
                .map_err(|e| anyhow::anyhow!("{}", e.message))?;
            let data = response.decode_json()?;

            if let PhpMixed::List(ref list) = data {
                for datum in list {
                    if let PhpMixed::Array(ref datum_map) = **datum {
                        let name = datum_map
                            .get("name")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string();
                        let commit_id = datum_map
                            .get("commit")
                            .and_then(|v| v.as_array())
                            .and_then(|m| m.get("id"))
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string();
                        references.insert(name, commit_id.clone());

                        // Keep the last commit date of a reference to avoid
                        // unnecessary API call when retrieving the composer file.
                        let commit_data = datum_map
                            .get("commit")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(k, v)| (k, *v))
                            .collect();
                        self.commits.insert(commit_id, commit_data);
                    }
                }
            }

            let len = match data {
                PhpMixed::List(ref l) => l.len() as i64,
                PhpMixed::Array(ref a) => a.len() as i64,
                _ => 0,
            };
            if len >= per_page {
                resource = self.get_next_page(&response);
            } else {
                resource = None;
            }
            if resource.is_none() {
                break;
            }
        }

        Ok(references)
    }

    pub(crate) fn fetch_project(&mut self) -> Result<()> {
        if self.project.is_some() {
            return Ok(());
        }

        // we need to fetch the default branch from the api
        let resource = self.get_api_url();
        let project = self
            .get_contents(&resource, true)
            .map_err(|e| anyhow::anyhow!("{}", e.message))?
            .decode_json()?;
        self.project = match project {
            PhpMixed::Array(m) => Some(m.into_iter().map(|(k, v)| (k, *v)).collect()),
            _ => None,
        };
        let project = self.project.clone().unwrap_or_default();
        if project.contains_key("visibility") {
            self.is_private = project
                .get("visibility")
                .and_then(|v| v.as_string())
                .map(|s| s != "public")
                .unwrap_or(true);
        } else {
            // client is not authenticated, therefore repository has to be public
            self.is_private = false;
        }

        Ok(())
    }

    /// @phpstan-impure
    ///
    /// @return true
    /// @throws \RuntimeException
    pub(crate) fn attempt_clone_fallback(&mut self) -> Result<bool> {
        let url = if !self.is_private {
            self.generate_public_url()
        } else {
            self.generate_ssh_url()
        };

        // If this repository may be private and we
        // cannot ask for authentication credentials (because we
        // are not interactive) then we fallback to GitDriver.
        match self.setup_git_driver(&url) {
            Ok(()) => Ok(true),
            Err(e) => {
                self.git_driver = None;

                self.inner.io.write_error(
                    PhpMixed::String(format!(
                        "<error>Failed to clone the {} repository, try running in interactive mode so that you can enter your credentials</error>",
                        url
                    )),
                    true,
                    IOInterface::NORMAL,
                );
                Err(e)
            }
        }
    }

    /// Generate an SSH URL
    pub(crate) fn generate_ssh_url(&self) -> String {
        if self.has_nonstandard_origin {
            return format!(
                "ssh://git@{}/{}/{}.git",
                self.inner.origin_url, self.namespace, self.repository
            );
        }

        format!(
            "git@{}:{}/{}.git",
            self.inner.origin_url, self.namespace, self.repository
        )
    }

    pub(crate) fn generate_public_url(&self) -> String {
        format!(
            "{}://{}/{}/{}.git",
            self.scheme, self.inner.origin_url, self.namespace, self.repository
        )
    }

    pub(crate) fn setup_git_driver(&mut self, url: &str) -> Result<()> {
        let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
        repo_config.insert("url".to_string(), PhpMixed::String(url.to_string()));
        let mut git_driver = GitDriver::new(
            repo_config,
            self.inner.io.clone(),
            self.inner.config.clone(),
            self.inner.http_downloader.clone(),
            self.inner.process.clone(),
        );
        git_driver.initialize()?;
        self.git_driver = Some(git_driver);
        Ok(())
    }

    pub(crate) fn get_contents(
        &mut self,
        url: &str,
        fetching_repo_data: bool,
    ) -> Result<Response, TransportException> {
        let response_result = self.inner.get_contents(url);
        match response_result {
            Ok(response) => {
                if fetching_repo_data {
                    let json = response.decode_json().map_err(|e| TransportException {
                        message: e.to_string(),
                        code: 0,
                    })?;
                    let json_map = match json {
                        PhpMixed::Array(ref m) => m.clone(),
                        _ => IndexMap::new(),
                    };

                    // Accessing the API with a token with Guest (10) or Planner (15) access will return
                    // more data than unauthenticated access but no default_branch data
                    // accessing files via the API will then also fail
                    if !json_map.contains_key("default_branch")
                        && json_map.contains_key("permissions")
                    {
                        self.is_private = json_map
                            .get("visibility")
                            .and_then(|v| v.as_string())
                            .map(|s| s != "public")
                            .unwrap_or(true);

                        let mut more_than_guest_access = false;
                        // Check both access levels (e.g. project, group)
                        // - value will be null if no access is set
                        // - value will be array with key access_level if set
                        if let Some(permissions) = json_map
                            .get("permissions")
                            .and_then(|v| v.as_array())
                        {
                            for (_, permission) in permissions {
                                if let Some(perm_map) = permission.as_array() {
                                    if let Some(level) = perm_map
                                        .get("access_level")
                                        .and_then(|v| v.as_int())
                                    {
                                        if level >= 20 {
                                            more_than_guest_access = true;
                                        }
                                    }
                                }
                            }
                        }

                        if !more_than_guest_access {
                            self.inner.io.write_error(
                                PhpMixed::String(
                                    "<warning>GitLab token with Guest or Planner only access detected</warning>"
                                        .to_string(),
                                ),
                                true,
                                IOInterface::NORMAL,
                            );

                            self.attempt_clone_fallback()
                                .map_err(|e| TransportException {
                                    message: e.to_string(),
                                    code: 0,
                                })?;

                            let mut req = IndexMap::new();
                            req.insert(
                                "url".to_string(),
                                PhpMixed::String("dummy".to_string()),
                            );
                            return Ok(Response::new(req, Some(200), vec![], Some("null".to_string()))
                                .unwrap()
                                .unwrap());
                        }
                    }

                    // force auth as the unauthenticated version of the API is broken
                    if !json_map.contains_key("default_branch") {
                        // GitLab allows you to disable the repository inside a project to use a project only for issues and wiki
                        if json_map
                            .get("repository_access_level")
                            .and_then(|v| v.as_string())
                            == Some("disabled")
                        {
                            return Err(TransportException {
                                message: "The GitLab repository is disabled in the project"
                                    .to_string(),
                                code: 400,
                            });
                        }

                        if !empty(
                            &json_map.get("id").cloned().unwrap_or(PhpMixed::Null),
                        ) {
                            self.is_private = false;
                        }

                        return Err(TransportException {
                            message:
                                "GitLab API seems to not be authenticated as it did not return a default_branch"
                                    .to_string(),
                            code: 401,
                        });
                    }
                }

                Ok(response)
            }
            Err(e) => {
                let mut git_lab_util = GitLab::new(
                    self.inner.io.as_ref(),
                    &self.inner.config,
                    &self.inner.process,
                    &self.inner.http_downloader,
                );

                match e.code {
                    401 | 404 => {
                        // try to authorize only if we are fetching the main /repos/foo/bar data, otherwise it must be a real 404
                        if !fetching_repo_data {
                            return Err(e);
                        }

                        if git_lab_util.authorize_oauth(&self.inner.origin_url) {
                            return self.inner.get_contents(url);
                        }

                        if git_lab_util.is_oauth_expired(&self.inner.origin_url)
                            && git_lab_util
                                .authorize_oauth_refresh(&self.scheme, &self.inner.origin_url)
                        {
                            return self.inner.get_contents(url);
                        }

                        if !self.inner.io.is_interactive() {
                            self.attempt_clone_fallback()
                                .map_err(|err| TransportException {
                                    message: err.to_string(),
                                    code: 0,
                                })?;

                            let mut req = IndexMap::new();
                            req.insert(
                                "url".to_string(),
                                PhpMixed::String("dummy".to_string()),
                            );
                            return Ok(Response::new(req, Some(200), vec![], Some("null".to_string()))
                                .unwrap()
                                .unwrap());
                        }
                        self.inner.io.write_error(
                            PhpMixed::String(format!(
                                "<warning>Failed to download {}/{}:{}</warning>",
                                self.namespace, self.repository, e.message
                            )),
                            true,
                            IOInterface::NORMAL,
                        );
                        git_lab_util.authorize_oauth_interactively(
                            &self.scheme,
                            &self.inner.origin_url,
                            Some(&format!(
                                "Your credentials are required to fetch private repository metadata (<info>{}</info>)",
                                self.inner.url
                            )),
                        );

                        self.inner.get_contents(url)
                    }
                    403 => {
                        if !self.inner.io.has_authentication(&self.inner.origin_url)
                            && git_lab_util.authorize_oauth(&self.inner.origin_url)
                        {
                            return self.inner.get_contents(url);
                        }

                        if !self.inner.io.is_interactive() && fetching_repo_data {
                            self.attempt_clone_fallback()
                                .map_err(|err| TransportException {
                                    message: err.to_string(),
                                    code: 0,
                                })?;

                            let mut req = IndexMap::new();
                            req.insert(
                                "url".to_string(),
                                PhpMixed::String("dummy".to_string()),
                            );
                            return Ok(Response::new(req, Some(200), vec![], Some("null".to_string()))
                                .unwrap()
                                .unwrap());
                        }

                        Err(e)
                    }
                    _ => Err(e),
                }
            }
        }
    }

    /// Uses the config `gitlab-domains` to see if the driver supports the url for the
    /// repository given.
    pub fn supports(io: &dyn IOInterface, config: &Config, url: &str, _deep: bool) -> bool {
        let match_ = match Preg::is_match_strict_groups(Self::URL_REGEX, url) {
            Some(m) => m,
            None => return false,
        };

        let scheme = match_.get("scheme").cloned().unwrap_or_default();
        let guessed_domain = match_
            .get("domain")
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| match_.get("domain2").cloned().unwrap_or_default());
        let mut url_parts: Vec<String> =
            explode("/", &match_.get("parts").cloned().unwrap_or_default());

        if Self::determine_origin(
            &config.get("gitlab-domains"),
            guessed_domain,
            &mut url_parts,
            match_.get("port").cloned(),
        )
        .is_none()
        {
            return false;
        }

        if scheme == "https" && !extension_loaded("openssl") {
            io.write_error(
                PhpMixed::String(format!(
                    "Skipping GitLab driver for {} because the OpenSSL PHP extension is missing.",
                    url
                )),
                true,
                IOInterface::VERBOSE,
            );

            return false;
        }

        true
    }

    /// Gives back the loaded <gitlab-api>/projects/<owner>/<repo> result
    ///
    /// @return mixed[]|null
    pub fn get_repo_data(&mut self) -> Result<Option<IndexMap<String, PhpMixed>>> {
        self.fetch_project()?;

        Ok(self.project.clone())
    }

    pub(crate) fn get_next_page(&self, response: &Response) -> Option<String> {
        let header = response.get_header("link").unwrap_or_default();

        let links = explode(",", &header);
        for link in &links {
            if let Some(match_) =
                Preg::is_match_strict_groups(r#"{<(.+?)>; *rel="next"}"#, link)
            {
                return Some(match_.get(1).cloned().unwrap_or_default());
            }
        }

        None
    }

    /// @param  array<string> $configuredDomains
    /// @param  array<string> $urlParts
    ///
    /// @return string|false
    fn determine_origin(
        configured_domains: &PhpMixed,
        guessed_domain: String,
        url_parts: &mut Vec<String>,
        port_number: Option<String>,
    ) -> Option<String> {
        let mut guessed_domain = strtolower(&guessed_domain);

        if in_array(
            PhpMixed::String(guessed_domain.clone()),
            configured_domains,
            false,
        ) || (port_number.is_some()
            && in_array(
                PhpMixed::String(format!(
                    "{}:{}",
                    guessed_domain,
                    port_number.as_deref().unwrap_or("")
                )),
                configured_domains,
                false,
            ))
        {
            if let Some(ref port) = port_number {
                return Some(format!("{}:{}", guessed_domain, port));
            }

            return Some(guessed_domain);
        }

        if let Some(ref port) = port_number {
            guessed_domain.push_str(&format!(":{}", port));
        }

        while let Some(part) = array_shift(url_parts) {
            guessed_domain.push_str(&format!("/{}", part));

            if in_array(
                PhpMixed::String(guessed_domain.clone()),
                configured_domains,
                false,
            ) || (port_number.is_some()
                && in_array(
                    PhpMixed::String(Preg::replace(
                        r"{:\d+}",
                        "",
                        guessed_domain.clone(),
                    )),
                    configured_domains,
                    false,
                ))
            {
                return Some(guessed_domain);
            }
        }

        None
    }
}

