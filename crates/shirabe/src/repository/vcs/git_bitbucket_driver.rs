//! ref: composer/src/Composer/Repository/Vcs/GitBitbucketDriver.php

use crate::io::io_interface;
use anyhow::Result;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, RuntimeException, array_key_exists,
    array_search_mixed, extension_loaded, http_build_query_mixed, implode, in_array, is_array,
    sprintf, strpos,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::repository::vcs::git_driver::GitDriver;
use crate::repository::vcs::vcs_driver::VcsDriverBase;
use crate::repository::vcs::vcs_driver_interface::VcsDriverInterface;
use crate::util::bitbucket::Bitbucket;
use crate::util::http::response::Response;

#[derive(Debug)]
pub struct GitBitbucketDriver {
    pub(crate) inner: VcsDriverBase,
    /// @var string
    pub(crate) owner: String,
    /// @var string
    pub(crate) repository: String,
    /// @var bool
    has_issues: bool,
    /// @var ?string
    root_identifier: Option<String>,
    /// @var array<int|string, string> Map of tag name to identifier
    tags: Option<IndexMap<String, String>>,
    /// @var array<int|string, string> Map of branch name to identifier
    branches: Option<IndexMap<String, String>>,
    /// @var string
    branches_url: String,
    /// @var string
    tags_url: String,
    /// @var string
    home_url: String,
    /// @var string
    website: String,
    /// @var string
    clone_https_url: String,
    /// @var array<string, mixed>
    repo_data: IndexMap<String, PhpMixed>,
    /// @var ?VcsDriver
    pub(crate) fallback_driver: Option<Box<dyn VcsDriverInterface>>,
    /// @var string|null if set either git or hg
    vcs_type: Option<String>,
}

impl GitBitbucketDriver {
    /// @inheritDoc
    pub fn initialize(&mut self) -> Result<()> {
        let matched = Preg::is_match_strict_groups(
            r"#^https?://bitbucket\.org/([^/]+)/([^/]+?)(?:\.git|/?)?$#i",
            &self.inner.url,
        );
        if matched.is_none() {
            return Err(InvalidArgumentException {
                message: sprintf(
                    "The Bitbucket repository URL %s is invalid. It must be the HTTPS URL of a Bitbucket repository.",
                    &[PhpMixed::String(self.inner.url.clone())],
                ),
                code: 0,
            }
            .into());
        }
        let m = matched.unwrap();

        self.owner = m.get(1).cloned().unwrap_or_default();
        self.repository = m.get(2).cloned().unwrap_or_default();
        self.inner.origin_url = "bitbucket.org".to_string();
        self.inner.cache = Some(Cache::new(
            &*self.inner.io,
            &implode(
                "/",
                &[
                    self.inner
                        .config
                        .get("cache-repo-dir")
                        .as_string()
                        .unwrap_or("")
                        .to_string(),
                    self.inner.origin_url.clone(),
                    self.owner.clone(),
                    self.repository.clone(),
                ],
            ),
            None,
        ));
        self.inner.cache.as_mut().unwrap().set_read_only(
            self.inner
                .config
                .get("cache-read-only")
                .as_bool()
                .unwrap_or(false),
        );

        Ok(())
    }

    /// @inheritDoc
    pub fn get_url(&self) -> String {
        if let Some(fallback) = self.fallback_driver.as_ref() {
            return fallback.get_url();
        }

        self.clone_https_url.clone()
    }

    /// Attempts to fetch the repository data via the BitBucket API and
    /// sets some parameters which are used in other methods
    ///
    /// @phpstan-impure
    fn get_repo_data(&mut self) -> Result<bool> {
        let resource = sprintf(
            "https://api.bitbucket.org/2.0/repositories/%s/%s?%s",
            &[
                PhpMixed::String(self.owner.clone()),
                PhpMixed::String(self.repository.clone()),
                PhpMixed::String(http_build_query_mixed(
                    &{
                        let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                        m.insert(
                            "fields".to_string(),
                            PhpMixed::String("-project,-owner".to_string()),
                        );
                        m
                    },
                    "",
                    "&",
                )),
            ],
        );

        let repo_data = self
            .fetch_with_oauth_credentials(&resource, true)?
            .decode_json()?;
        if self.fallback_driver.is_some() {
            return Ok(false);
        }
        let clone_links = repo_data
            .get("links")
            .and_then(|v| match v {
                PhpMixed::Array(m) => m.get("clone"),
                _ => None,
            })
            .cloned();
        self.parse_clone_urls(clone_links);

        self.has_issues = !shirabe_php_shim::empty(
            repo_data
                .get("has_issues")
                .cloned()
                .as_ref()
                .unwrap_or(&PhpMixed::Null),
        );
        self.branches_url = repo_data
            .get("links")
            .and_then(|v| match v {
                PhpMixed::Array(m) => m.get("branches"),
                _ => None,
            })
            .and_then(|v| match v.as_ref() {
                PhpMixed::Array(m) => m.get("href").and_then(|v| v.as_string()).map(String::from),
                _ => None,
            })
            .unwrap_or_default();
        self.tags_url = repo_data
            .get("links")
            .and_then(|v| match v {
                PhpMixed::Array(m) => m.get("tags"),
                _ => None,
            })
            .and_then(|v| match v.as_ref() {
                PhpMixed::Array(m) => m.get("href").and_then(|v| v.as_string()).map(String::from),
                _ => None,
            })
            .unwrap_or_default();
        self.home_url = repo_data
            .get("links")
            .and_then(|v| match v {
                PhpMixed::Array(m) => m.get("html"),
                _ => None,
            })
            .and_then(|v| match v.as_ref() {
                PhpMixed::Array(m) => m.get("href").and_then(|v| v.as_string()).map(String::from),
                _ => None,
            })
            .unwrap_or_default();
        self.website = repo_data
            .get("website")
            .and_then(|v| v.as_string())
            .map(String::from)
            .unwrap_or_default();
        self.vcs_type = repo_data
            .get("scm")
            .and_then(|v| v.as_string())
            .map(String::from);

        self.repo_data = repo_data;

        Ok(true)
    }

    /// @inheritDoc
    pub fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> Result<Option<IndexMap<String, PhpMixed>>> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_composer_information(identifier);
        }

        if !self.inner.info_cache.contains_key(identifier) {
            let mut composer: Option<IndexMap<String, PhpMixed>> = None;
            if self.inner.should_cache(identifier) && {
                let res = self
                    .inner
                    .cache
                    .as_ref()
                    .and_then(|c| c.read(identifier).ok().flatten());
                if let Some(res) = res {
                    composer = Some(JsonFile::parse_json(&res, None)?);
                    true
                } else {
                    false
                }
            } {
                // composer already set above
            } else {
                composer = self.inner.get_base_composer_information(identifier)?;

                if self.inner.should_cache(identifier) {
                    self.inner.cache.as_ref().unwrap().write(
                        identifier,
                        &JsonFile::encode(
                            &PhpMixed::Array(
                                composer
                                    .clone()
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(|(k, v)| (k, Box::new(v)))
                                    .collect(),
                            ),
                            shirabe_php_shim::JSON_UNESCAPED_UNICODE
                                | shirabe_php_shim::JSON_UNESCAPED_SLASHES,
                            JsonFile::INDENT_DEFAULT,
                        ),
                    )?;
                }
            }

            if let Some(mut composer_map) = composer.clone() {
                // specials for bitbucket
                if composer_map.contains_key("support")
                    && !is_array(composer_map.get("support").unwrap())
                {
                    composer_map.insert("support".to_string(), PhpMixed::Array(IndexMap::new()));
                }
                let support_has_source = composer_map
                    .get("support")
                    .and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m.contains_key("source")),
                        _ => None,
                    })
                    .unwrap_or(false);
                if !support_has_source {
                    let tags = self.get_tags()?;
                    let branches_for_search = self.get_branches()?;
                    let label = array_search_mixed(
                        &PhpMixed::String(identifier.to_string()),
                        &PhpMixed::Array(
                            tags.iter()
                                .map(|(k, v)| (k.clone(), Box::new(PhpMixed::String(v.clone()))))
                                .collect(),
                        ),
                        false,
                    )
                    .or_else(|| {
                        array_search_mixed(
                            &PhpMixed::String(identifier.to_string()),
                            &PhpMixed::Array(
                                branches_for_search
                                    .iter()
                                    .map(|(k, v)| {
                                        (k.clone(), Box::new(PhpMixed::String(v.clone())))
                                    })
                                    .collect(),
                            ),
                            false,
                        )
                    })
                    .map(|v| v.as_string().unwrap_or("").to_string())
                    .unwrap_or_else(|| identifier.to_string());

                    let tags2 = self.get_tags()?;
                    let branches2 = self.get_branches()?;
                    let mut hash: Option<String> = None;
                    if array_key_exists(&label, &tags2) {
                        hash = tags2.get(&label).cloned();
                    } else if array_key_exists(&label, &branches2) {
                        hash = branches2.get(&label).cloned();
                    }

                    let support_entry = composer_map
                        .entry("support".to_string())
                        .or_insert(PhpMixed::Array(IndexMap::new()));
                    if hash.is_none() {
                        if let PhpMixed::Array(support_map) = support_entry {
                            support_map.insert(
                                "source".to_string(),
                                Box::new(PhpMixed::String(sprintf(
                                    "https://%s/%s/%s/src",
                                    &[
                                        PhpMixed::String(self.inner.origin_url.clone()),
                                        PhpMixed::String(self.owner.clone()),
                                        PhpMixed::String(self.repository.clone()),
                                    ],
                                ))),
                            );
                        }
                    } else if let PhpMixed::Array(support_map) = support_entry {
                        support_map.insert(
                            "source".to_string(),
                            Box::new(PhpMixed::String(sprintf(
                                "https://%s/%s/%s/src/%s/?at=%s",
                                &[
                                    PhpMixed::String(self.inner.origin_url.clone()),
                                    PhpMixed::String(self.owner.clone()),
                                    PhpMixed::String(self.repository.clone()),
                                    PhpMixed::String(hash.unwrap()),
                                    PhpMixed::String(label.clone()),
                                ],
                            ))),
                        );
                    }
                }
                let support_has_issues = composer_map
                    .get("support")
                    .and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m.contains_key("issues")),
                        _ => None,
                    })
                    .unwrap_or(false);
                if !support_has_issues && self.has_issues {
                    let support_entry = composer_map
                        .entry("support".to_string())
                        .or_insert(PhpMixed::Array(IndexMap::new()));
                    if let PhpMixed::Array(support_map) = support_entry {
                        support_map.insert(
                            "issues".to_string(),
                            Box::new(PhpMixed::String(sprintf(
                                "https://%s/%s/%s/issues",
                                &[
                                    PhpMixed::String(self.inner.origin_url.clone()),
                                    PhpMixed::String(self.owner.clone()),
                                    PhpMixed::String(self.repository.clone()),
                                ],
                            ))),
                        );
                    }
                }
                if !composer_map.contains_key("homepage") {
                    composer_map.insert(
                        "homepage".to_string(),
                        if self.website.is_empty() {
                            PhpMixed::String(self.home_url.clone())
                        } else {
                            PhpMixed::String(self.website.clone())
                        },
                    );
                }
                composer = Some(composer_map);
            }

            self.inner
                .info_cache
                .insert(identifier.to_string(), composer);
        }

        Ok(self.inner.info_cache.get(identifier).cloned().flatten())
    }

    /// @inheritDoc
    pub fn get_file_content(&mut self, file: &str, identifier: &str) -> Result<Option<String>> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_file_content(file, identifier);
        }

        let mut identifier = identifier.to_string();
        if strpos(&identifier, "/").is_some() {
            let branches = self.get_branches()?;
            if let Some(b) = branches.get(&identifier) {
                identifier = b.clone();
            }
        }

        let resource = sprintf(
            "https://api.bitbucket.org/2.0/repositories/%s/%s/src/%s/%s",
            &[
                PhpMixed::String(self.owner.clone()),
                PhpMixed::String(self.repository.clone()),
                PhpMixed::String(identifier),
                PhpMixed::String(file.to_string()),
            ],
        );

        Ok(Some(
            self.fetch_with_oauth_credentials(&resource, false)?
                .get_body(),
        ))
    }

    /// @inheritDoc
    pub fn get_change_date(&mut self, identifier: &str) -> Result<Option<DateTime<Utc>>> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_change_date(identifier);
        }

        let mut identifier = identifier.to_string();
        if strpos(&identifier, "/").is_some() {
            let branches = self.get_branches()?;
            if let Some(b) = branches.get(&identifier) {
                identifier = b.clone();
            }
        }

        let resource = sprintf(
            "https://api.bitbucket.org/2.0/repositories/%s/%s/commit/%s?fields=date",
            &[
                PhpMixed::String(self.owner.clone()),
                PhpMixed::String(self.repository.clone()),
                PhpMixed::String(identifier),
            ],
        );
        let commit = self
            .fetch_with_oauth_credentials(&resource, false)?
            .decode_json()?;

        // TODO(phase-b): port PHP `new \DateTimeImmutable($commit['date'])`
        let date_str = commit.get("date").and_then(|v| v.as_string()).unwrap_or("");
        let date: DateTime<Utc> = chrono::DateTime::parse_from_rfc3339(date_str)
            .map_err(|e| anyhow::anyhow!(e))?
            .with_timezone(&Utc);
        Ok(Some(date))
    }

    /// @inheritDoc
    pub fn get_source(&self, identifier: &str) -> IndexMap<String, String> {
        if let Some(fallback) = self.fallback_driver.as_ref() {
            return fallback.get_source(identifier);
        }

        let mut m: IndexMap<String, String> = IndexMap::new();
        m.insert(
            "type".to_string(),
            self.vcs_type.clone().unwrap_or_default(),
        );
        m.insert("url".to_string(), self.get_url());
        m.insert("reference".to_string(), identifier.to_string());
        m
    }

    /// @inheritDoc
    pub fn get_dist(&self, identifier: &str) -> Option<IndexMap<String, String>> {
        if let Some(fallback) = self.fallback_driver.as_ref() {
            return fallback.get_dist(identifier);
        }

        let url = sprintf(
            "https://bitbucket.org/%s/%s/get/%s.zip",
            &[
                PhpMixed::String(self.owner.clone()),
                PhpMixed::String(self.repository.clone()),
                PhpMixed::String(identifier.to_string()),
            ],
        );

        let mut m: IndexMap<String, String> = IndexMap::new();
        m.insert("type".to_string(), "zip".to_string());
        m.insert("url".to_string(), url);
        m.insert("reference".to_string(), identifier.to_string());
        m.insert("shasum".to_string(), String::new());
        Some(m)
    }

    /// @inheritDoc
    pub fn get_tags(&mut self) -> Result<IndexMap<String, String>> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_tags();
        }

        if self.tags.is_none() {
            let mut tags: IndexMap<String, String> = IndexMap::new();
            let mut resource = sprintf(
                "%s?%s",
                &[
                    PhpMixed::String(self.tags_url.clone()),
                    PhpMixed::String(http_build_query_mixed(
                        &{
                            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                            m.insert("pagelen".to_string(), PhpMixed::Int(100));
                            m.insert(
                                "fields".to_string(),
                                PhpMixed::String("values.name,values.target.hash,next".to_string()),
                            );
                            m.insert(
                                "sort".to_string(),
                                PhpMixed::String("-target.date".to_string()),
                            );
                            m
                        },
                        "",
                        "&",
                    )),
                ],
            );
            let mut has_next = true;
            while has_next {
                let tags_data = self
                    .fetch_with_oauth_credentials(&resource, false)?
                    .decode_json()?;
                let values = tags_data.get("values").cloned();
                if let Some(PhpMixed::List(list)) = values {
                    for data in list {
                        if let PhpMixed::Array(m) = data.as_ref() {
                            let name = m
                                .get("name")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string();
                            let hash = m
                                .get("target")
                                .and_then(|v| match v.as_ref() {
                                    PhpMixed::Array(m) => m.get("hash"),
                                    _ => None,
                                })
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string();
                            tags.insert(name, hash);
                        }
                    }
                }
                if shirabe_php_shim::empty(
                    tags_data
                        .get("next")
                        .cloned()
                        .as_ref()
                        .unwrap_or(&PhpMixed::Null),
                ) {
                    has_next = false;
                } else {
                    resource = tags_data
                        .get("next")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                }
            }

            self.tags = Some(tags);
        }

        Ok(self.tags.clone().unwrap_or_default())
    }

    /// @inheritDoc
    pub fn get_branches(&mut self) -> Result<IndexMap<String, String>> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_branches();
        }

        if self.branches.is_none() {
            let mut branches: IndexMap<String, String> = IndexMap::new();
            let mut resource = sprintf(
                "%s?%s",
                &[
                    PhpMixed::String(self.branches_url.clone()),
                    PhpMixed::String(http_build_query_mixed(
                        &{
                            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                            m.insert("pagelen".to_string(), PhpMixed::Int(100));
                            m.insert(
                                "fields".to_string(),
                                PhpMixed::String(
                                    "values.name,values.target.hash,values.heads,next".to_string(),
                                ),
                            );
                            m.insert(
                                "sort".to_string(),
                                PhpMixed::String("-target.date".to_string()),
                            );
                            m
                        },
                        "",
                        "&",
                    )),
                ],
            );
            let mut has_next = true;
            while has_next {
                let branch_data = self
                    .fetch_with_oauth_credentials(&resource, false)?
                    .decode_json()?;
                let values = branch_data.get("values").cloned();
                if let Some(PhpMixed::List(list)) = values {
                    for data in list {
                        if let PhpMixed::Array(m) = data.as_ref() {
                            let name = m
                                .get("name")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string();
                            let hash = m
                                .get("target")
                                .and_then(|v| match v.as_ref() {
                                    PhpMixed::Array(m) => m.get("hash"),
                                    _ => None,
                                })
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string();
                            branches.insert(name, hash);
                        }
                    }
                }
                if shirabe_php_shim::empty(
                    branch_data
                        .get("next")
                        .cloned()
                        .as_ref()
                        .unwrap_or(&PhpMixed::Null),
                ) {
                    has_next = false;
                } else {
                    resource = branch_data
                        .get("next")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                }
            }

            self.branches = Some(branches);
        }

        Ok(self.branches.clone().unwrap_or_default())
    }

    /// Get the remote content.
    ///
    /// @phpstan-impure
    fn fetch_with_oauth_credentials(
        &mut self,
        url: &str,
        fetching_repo_data: bool,
    ) -> Result<Response> {
        match self.inner.get_contents(url, false) {
            Ok(r) => Ok(r),
            Err(e) => {
                // TODO(phase-b): only handle TransportException
                let bitbucket_util = Bitbucket::new(
                    &*self.inner.io,
                    &self.inner.config,
                    Some(self.inner.process.clone()),
                    Some(self.inner.http_downloader.clone()),
                );

                if let Some(te) = e.downcast_ref::<TransportException>() {
                    let code = te.get_code();
                    let in_set = in_array(
                        PhpMixed::Int(code),
                        &PhpMixed::List(vec![
                            Box::new(PhpMixed::Int(403)),
                            Box::new(PhpMixed::Int(404)),
                        ]),
                        true,
                    );
                    if in_set
                        || (401 == code
                            && strpos(te.get_message(), "Could not authenticate against")
                                == Some(0))
                    {
                        if !self.inner.io.has_authentication(&self.inner.origin_url)
                            && bitbucket_util.authorize_oauth(&self.inner.origin_url)
                        {
                            return self.inner.get_contents(url, false);
                        }

                        if !self.inner.io.is_interactive() && fetching_repo_data {
                            self.attempt_clone_fallback()?;

                            let mut headers: IndexMap<String, PhpMixed> = IndexMap::new();
                            headers
                                .insert("url".to_string(), PhpMixed::String("dummy".to_string()));
                            return Ok(Response::new(
                                headers,
                                200,
                                IndexMap::new(),
                                "null".to_string(),
                            ));
                        }
                    }
                }

                Err(e)
            }
        }
    }

    /// Generate an SSH URL
    fn generate_ssh_url(&self) -> String {
        format!(
            "git@{}:{}/{}.git",
            self.inner.origin_url, self.owner, self.repository
        )
    }

    /// @phpstan-impure
    ///
    /// @return true
    /// @throws \RuntimeException
    fn attempt_clone_fallback(&mut self) -> Result<bool> {
        match self.setup_fallback_driver(&self.generate_ssh_url()) {
            Ok(()) => Ok(true),
            Err(e) => {
                // TODO(phase-b): only catch RuntimeException
                self.fallback_driver = None;

                self.inner.io.write_error(&format!(
                    "<error>Failed to clone the {} repository, try running in interactive mode so that you can enter your Bitbucket OAuth consumer credentials</error>",
                    self.generate_ssh_url()
                ));
                Err(e)
            }
        }
    }

    fn setup_fallback_driver(&mut self, url: &str) -> Result<()> {
        let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
        repo_config.insert("url".to_string(), PhpMixed::String(url.to_string()));
        // TODO(phase-b): construct VcsDriver from repo_config / io / config / etc.
        let mut driver = GitDriver {
            inner: todo!("phase-b: build VcsDriver for fallback GitDriver"),
            tags: None,
            branches: None,
            root_identifier: None,
            repo_dir: String::new(),
        };
        driver.initialize()?;
        self.fallback_driver = Some(Box::new(driver));
        Ok(())
    }

    /// @param  array<array{name: string, href: string}> $cloneLinks
    fn parse_clone_urls(&mut self, clone_links: Option<Box<PhpMixed>>) {
        let list = match clone_links.as_deref() {
            Some(PhpMixed::List(l)) => l.clone(),
            _ => return,
        };
        for clone_link in list {
            if let PhpMixed::Array(m) = clone_link.as_ref() {
                if m.get("name").and_then(|v| v.as_string()) == Some("https") {
                    // Format: https://(user@)bitbucket.org/{user}/{repo}
                    // Strip username from URL (only present in clone URL's for private repositories)
                    self.clone_https_url = Preg::replace(
                        r"/https:\/\/([^@]+@)?/",
                        "https://",
                        m.get("href").and_then(|v| v.as_string()).unwrap_or(""),
                    );
                }
            }
        }
    }

    /// @inheritDoc
    pub fn get_root_identifier(&mut self) -> Result<String> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_root_identifier();
        }

        if self.root_identifier.is_none() {
            if !self.get_repo_data()? {
                if self.fallback_driver.is_none() {
                    return Err(LogicException {
                        message: "A fallback driver should be setup if getRepoData returns false"
                            .to_string(),
                        code: 0,
                    }
                    .into());
                }

                return self.fallback_driver.as_mut().unwrap().get_root_identifier();
            }

            if self.vcs_type.as_deref() != Some("git") {
                return Err(RuntimeException {
                    message: format!(
                        "{} does not appear to be a git repository, use {} but remember that Bitbucket no longer supports the mercurial repositories. https://bitbucket.org/blog/sunsetting-mercurial-support-in-bitbucket",
                        self.inner.url, self.clone_https_url
                    ),
                    code: 0,
                }
                .into());
            }

            self.root_identifier = self
                .repo_data
                .get("mainbranch")
                .and_then(|v| match v {
                    PhpMixed::Array(m) => m.get("name"),
                    _ => None,
                })
                .and_then(|v| v.as_string())
                .map(String::from)
                .or_else(|| Some("master".to_string()));
        }

        Ok(self.root_identifier.clone().unwrap_or_default())
    }

    /// @inheritDoc
    pub fn supports(io: &dyn IOInterface, _config: &Config, url: &str, _deep: bool) -> bool {
        if !Preg::is_match(
            r"#^https?://bitbucket\.org/([^/]+)/([^/]+?)(\.git|/?)?$#i",
            url,
        ) {
            return false;
        }

        if !extension_loaded("openssl") {
            io.write_error(
                &format!(
                    "Skipping Bitbucket git driver for {} because the OpenSSL PHP extension is missing.",
                    url
                ),
            );
            // PHP: writeError(..., true, io_interface::VERBOSE)
            // TODO(phase-b): io_interface::VERBOSE verbosity argument

            return false;
        }

        true
    }
}
