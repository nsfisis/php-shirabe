//! ref: composer/src/Composer/Repository/Vcs/GitBitbucketDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::json::{JsonEncodeOptions, JsonFile};
use crate::repository::vcs::{GitDriver, VcsDriverBase, VcsDriverInterface};
use crate::util::Bitbucket;
use crate::util::http::Response;
use bstr::{B, BString, ByteSlice};
use chrono::{DateTime, FixedOffset};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, InvalidArgumentException, LogicException, PhpMixed, RuntimeException,
    array_key_exists, array_search_mixed, date_create, empty, extension_loaded, http_build_query,
    implode, is_array, legacy_lossy, php_regex, preg_is_match, preg_match, preg_replace, strpos,
};

#[derive(Debug)]
pub struct GitBitbucketDriver {
    inner: VcsDriverBase,
    owner: String,
    repository: String,
    has_issues: bool,
    root_identifier: Option<String>,
    tags: Option<IndexMap<String, String>>,
    branches: Option<IndexMap<String, String>>,
    branches_url: String,
    tags_url: String,
    home_url: String,
    website: String,
    clone_https_url: String,
    repo_data: IndexMap<BString, PhpMixed>,
    fallback_driver: Option<Box<dyn VcsDriverInterface>>,
    vcs_type: Option<String>,
}

impl GitBitbucketDriver {
    pub fn new(
        repo_config: IndexMap<BString, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
        process: std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    ) -> Self {
        Self {
            inner: VcsDriverBase::new(repo_config, io, config, http_downloader, process),
            owner: String::new(),
            repository: String::new(),
            has_issues: false,
            root_identifier: None,
            tags: None,
            branches: None,
            branches_url: String::new(),
            tags_url: String::new(),
            home_url: String::new(),
            website: String::new(),
            clone_https_url: String::new(),
            repo_data: IndexMap::new(),
            fallback_driver: None,
            vcs_type: None,
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<()> {
        let Some(m) = preg_match(
            php_regex!(r"#^https?://bitbucket\.org/([^/]+)/([^/]+?)(?:\.git|/?)?$#i"),
            &self.inner.url,
        ) else {
            return Err(InvalidArgumentException::new(format!(
                "The Bitbucket repository URL {} is invalid. It must be the HTTPS URL of a Bitbucket repository.",
                self.inner.url.clone(),
            ))
            .into());
        };

        self.owner = legacy_lossy(m.get(1).unwrap_or_default()).into_owned();
        self.repository = legacy_lossy(m.get(2).unwrap_or_default()).into_owned();
        self.inner.origin_url = "bitbucket.org".to_string();
        self.inner.cache = Some(Cache::new(
            self.inner.io.clone(),
            &legacy_lossy(&implode(
                "/",
                &[
                    legacy_lossy(
                        self.inner
                            .config
                            .borrow_mut()
                            .get("cache-repo-dir")
                            .as_string()
                            .unwrap_or_default(),
                    )
                    .into_owned(),
                    self.inner.origin_url.clone(),
                    self.owner.clone(),
                    self.repository.clone(),
                ],
            )),
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

        Ok(())
    }

    pub fn get_url(&self) -> String {
        if let Some(fallback) = self.fallback_driver.as_ref() {
            return fallback.get_url();
        }

        self.clone_https_url.clone()
    }

    /// Attempts to fetch the repository data via the BitBucket API and
    /// sets some parameters which are used in other methods
    fn get_repo_data(&mut self) -> anyhow::Result<bool> {
        let resource = format!(
            "https://api.bitbucket.org/2.0/repositories/{}/{}?{}",
            self.owner.clone(),
            self.repository.clone(),
            legacy_lossy(&http_build_query(&[("fields", "-project,-owner")])),
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
                PhpMixed::Array(m) => m.get(B("clone")),
                _ => None,
            })
            .cloned();
        self.parse_clone_urls(clone_links);

        self.has_issues = !empty(
            repo_data
                .get("has_issues")
                .cloned()
                .as_ref()
                .unwrap_or(&PhpMixed::Null),
        );
        self.branches_url = repo_data
            .get("links")
            .and_then(|v| match v {
                PhpMixed::Array(m) => m.get(B("branches")),
                _ => None,
            })
            .and_then(|v| match v {
                PhpMixed::Array(m) => m
                    .get(B("href"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_str_lossy().into_owned()),
                _ => None,
            })
            .unwrap_or_default();
        self.tags_url = repo_data
            .get("links")
            .and_then(|v| match v {
                PhpMixed::Array(m) => m.get(B("tags")),
                _ => None,
            })
            .and_then(|v| match v {
                PhpMixed::Array(m) => m
                    .get(B("href"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_str_lossy().into_owned()),
                _ => None,
            })
            .unwrap_or_default();
        self.home_url = repo_data
            .get("links")
            .and_then(|v| match v {
                PhpMixed::Array(m) => m.get(B("html")),
                _ => None,
            })
            .and_then(|v| match v {
                PhpMixed::Array(m) => m
                    .get(B("href"))
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_str_lossy().into_owned()),
                _ => None,
            })
            .unwrap_or_default();
        self.website = repo_data
            .get("website")
            .and_then(|v| v.as_string())
            .map(|s| s.to_str_lossy().into_owned())
            .unwrap_or_default();
        self.vcs_type = repo_data
            .get("scm")
            .and_then(|v| v.as_string())
            .map(|s| s.to_str_lossy().into_owned());

        self.repo_data = match repo_data {
            PhpMixed::Array(m) => m,
            _ => IndexMap::new(),
        };

        Ok(true)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_composer_information(identifier);
        }

        if !self.inner.info_cache.contains_key(identifier) {
            let mut composer: Option<IndexMap<BString, PhpMixed>> = None;
            if self.inner.should_cache(identifier) && {
                let res = self.inner.cache.as_mut().and_then(|c| c.read(identifier));
                if let Some(res) = res {
                    composer = JsonFile::parse_json(Some(&res), None)?.as_array().cloned();
                    true
                } else {
                    false
                }
            } {
                // composer already set above
            } else {
                let file_content = self.get_file_content("composer.json", identifier)?;
                composer = VcsDriverBase::finish_base_composer_information(
                    identifier,
                    file_content,
                    || self.get_change_date(identifier),
                )?;

                if self.inner.should_cache(identifier) {
                    self.inner.cache.as_mut().unwrap().write(
                        identifier,
                        &JsonFile::encode_with_options(
                            &PhpMixed::Array(
                                composer.clone().unwrap_or_default().into_iter().collect(),
                            ),
                            JsonEncodeOptions {
                                pretty_print: false,
                                ..Default::default()
                            },
                        )?,
                    )?;
                }
            }

            if let Some(mut composer_map) = composer.clone() {
                // specials for bitbucket
                if composer_map.contains_key(B("support"))
                    && !is_array(composer_map.get(B("support")).unwrap())
                {
                    composer_map.insert("support".into(), PhpMixed::Array(IndexMap::new()));
                }
                let support_has_source = composer_map
                    .get(B("support"))
                    .and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m.contains_key(B("source"))),
                        _ => None,
                    })
                    .unwrap_or(false);
                if !support_has_source {
                    let tags = self.get_tags()?;
                    let branches_for_search = self.get_branches()?;
                    let label = array_search_mixed(
                        &PhpMixed::string(identifier.to_string()),
                        &PhpMixed::Array(
                            tags.iter()
                                .map(|(k, v)| (k.as_str().into(), PhpMixed::string(v.clone())))
                                .collect(),
                        ),
                        false,
                    )
                    .or_else(|| {
                        array_search_mixed(
                            &PhpMixed::string(identifier.to_string()),
                            &PhpMixed::Array(
                                branches_for_search
                                    .iter()
                                    .map(|(k, v)| (k.as_str().into(), PhpMixed::string(v.clone())))
                                    .collect(),
                            ),
                            false,
                        )
                    })
                    .map(|v| legacy_lossy(v.as_string().unwrap_or_default()).into_owned())
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
                        .entry("support".into())
                        .or_insert(PhpMixed::Array(IndexMap::new()));
                    match &hash {
                        None => {
                            if let PhpMixed::Array(support_map) = support_entry {
                                support_map.insert(
                                    "source".into(),
                                    PhpMixed::string(format!(
                                        "https://{}/{}/{}/src",
                                        self.inner.origin_url.clone(),
                                        self.owner.clone(),
                                        self.repository.clone(),
                                    )),
                                );
                            }
                        }
                        Some(hash) => {
                            if let PhpMixed::Array(support_map) = support_entry {
                                support_map.insert(
                                    "source".into(),
                                    PhpMixed::string(format!(
                                        "https://{}/{}/{}/src/{}/?at={}",
                                        self.inner.origin_url.clone(),
                                        self.owner.clone(),
                                        self.repository.clone(),
                                        hash,
                                        label,
                                    )),
                                );
                            }
                        }
                    }
                }
                let support_has_issues = composer_map
                    .get(B("support"))
                    .and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m.contains_key(B("issues"))),
                        _ => None,
                    })
                    .unwrap_or(false);
                if !support_has_issues && self.has_issues {
                    let support_entry = composer_map
                        .entry("support".into())
                        .or_insert(PhpMixed::Array(IndexMap::new()));
                    if let PhpMixed::Array(support_map) = support_entry {
                        support_map.insert(
                            "issues".into(),
                            PhpMixed::string(format!(
                                "https://{}/{}/{}/issues",
                                self.inner.origin_url.clone(),
                                self.owner.clone(),
                                self.repository.clone(),
                            )),
                        );
                    }
                }
                if !composer_map.contains_key(B("homepage")) {
                    composer_map.insert(
                        "homepage".into(),
                        if self.website.is_empty() {
                            PhpMixed::string(self.home_url.clone())
                        } else {
                            PhpMixed::string(self.website.clone())
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

    pub fn get_file_content(
        &mut self,
        file: &str,
        identifier: &str,
    ) -> anyhow::Result<Option<String>> {
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

        let resource = format!(
            "https://api.bitbucket.org/2.0/repositories/{}/{}/src/{}/{}",
            self.owner.clone(),
            self.repository.clone(),
            identifier,
            file,
        );

        Ok(self
            .fetch_with_oauth_credentials(&resource, false)?
            .get_body()
            .map(|s| s.to_string()))
    }

    pub fn get_change_date(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
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

        let resource = format!(
            "https://api.bitbucket.org/2.0/repositories/{}/{}/commit/{}?fields=date",
            self.owner.clone(),
            self.repository.clone(),
            identifier,
        );
        let commit = self
            .fetch_with_oauth_credentials(&resource, false)?
            .decode_json()?;

        let date_str = commit
            .get("date")
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let date: DateTime<FixedOffset> = date_create(&date_str.to_str_lossy())?;
        Ok(Some(date))
    }

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

    pub fn get_dist(&self, identifier: &str) -> Option<IndexMap<String, String>> {
        if let Some(fallback) = self.fallback_driver.as_ref() {
            return fallback.get_dist(identifier);
        }

        let url = format!(
            "https://bitbucket.org/{}/{}/get/{}.zip",
            self.owner.clone(),
            self.repository.clone(),
            identifier,
        );

        let mut m: IndexMap<String, String> = IndexMap::new();
        m.insert("type".to_string(), "zip".to_string());
        m.insert("url".to_string(), url);
        m.insert("reference".to_string(), identifier.to_string());
        m.insert("shasum".to_string(), String::new());
        Some(m)
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_tags();
        }

        if self.tags.is_none() {
            let mut tags: IndexMap<String, String> = IndexMap::new();
            let mut resource = format!(
                "{}?{}",
                self.tags_url.clone(),
                legacy_lossy(&http_build_query(&[
                    ("pagelen", "100"),
                    ("fields", "values.name,values.target.hash,next"),
                    ("sort", "-target.date"),
                ])),
            );
            let mut has_next = true;
            while has_next {
                let tags_data = self
                    .fetch_with_oauth_credentials(&resource, false)?
                    .decode_json()?;
                let values = tags_data.get("values").cloned();
                if let Some(PhpMixed::List(list)) = values {
                    for data in list {
                        if let PhpMixed::Array(m) = data {
                            let name = legacy_lossy(
                                m.get(B("name"))
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default(),
                            )
                            .into_owned();
                            let hash = legacy_lossy(
                                m.get(B("target"))
                                    .and_then(|v| match v {
                                        PhpMixed::Array(m) => m.get(B("hash")),
                                        _ => None,
                                    })
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default(),
                            )
                            .into_owned();
                            tags.insert(name, hash);
                        }
                    }
                }
                if empty(
                    tags_data
                        .get("next")
                        .cloned()
                        .as_ref()
                        .unwrap_or(&PhpMixed::Null),
                ) {
                    has_next = false;
                } else {
                    resource = legacy_lossy(
                        tags_data
                            .get("next")
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                    )
                    .into_owned();
                }
            }

            self.tags = Some(tags);
        }

        Ok(self.tags.clone().unwrap_or_default())
    }

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_branches();
        }

        if self.branches.is_none() {
            let mut branches: IndexMap<String, String> = IndexMap::new();
            let mut resource = format!(
                "{}?{}",
                self.branches_url.clone(),
                legacy_lossy(&http_build_query(&[
                    ("pagelen", "100"),
                    ("fields", "values.name,values.target.hash,values.heads,next"),
                    ("sort", "-target.date"),
                ])),
            );
            let mut has_next = true;
            while has_next {
                let branch_data = self
                    .fetch_with_oauth_credentials(&resource, false)?
                    .decode_json()?;
                let values = branch_data.get("values").cloned();
                if let Some(PhpMixed::List(list)) = values {
                    for data in list {
                        if let PhpMixed::Array(m) = data {
                            let name = legacy_lossy(
                                m.get(B("name"))
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default(),
                            )
                            .into_owned();
                            let hash = legacy_lossy(
                                m.get(B("target"))
                                    .and_then(|v| match v {
                                        PhpMixed::Array(m) => m.get(B("hash")),
                                        _ => None,
                                    })
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default(),
                            )
                            .into_owned();
                            branches.insert(name, hash);
                        }
                    }
                }
                if empty(
                    branch_data
                        .get("next")
                        .cloned()
                        .as_ref()
                        .unwrap_or(&PhpMixed::Null),
                ) {
                    has_next = false;
                } else {
                    resource = legacy_lossy(
                        branch_data
                            .get("next")
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                    )
                    .into_owned();
                }
            }

            self.branches = Some(branches);
        }

        Ok(self.branches.clone().unwrap_or_default())
    }

    /// Get the remote content.
    fn fetch_with_oauth_credentials(
        &mut self,
        url: &str,
        fetching_repo_data: bool,
    ) -> anyhow::Result<Response> {
        let e = match self.inner.get_contents(url)? {
            Ok(r) => return Ok(r),
            Err(e) => e,
        };

        let mut bitbucket_util = Bitbucket::new(
            self.inner.io.clone(),
            self.inner.config.clone(),
            Some(self.inner.process.clone()),
            Some(self.inner.http_downloader.clone()),
            None,
        )?;

        let code = e.get_code();
        let in_set = matches!(code, 403 | 404);
        if in_set
            || (401 == code && strpos(e.get_message(), "Could not authenticate against") == Some(0))
        {
            if !self.inner.io.has_authentication(&self.inner.origin_url)
                && bitbucket_util.authorize_oauth(&self.inner.origin_url)?
            {
                return Ok(self.inner.get_contents(url)??);
            }

            if !self.inner.io.is_interactive() && fetching_repo_data {
                self.attempt_clone_fallback()?;

                return Ok(Response::new(
                    "dummy".to_string(),
                    Some(200),
                    vec![],
                    Some("null".to_string()),
                ));
            }
        }

        Err((*e).into())
    }

    /// Generate an SSH URL
    fn generate_ssh_url(&self) -> String {
        format!(
            "git@{}:{}/{}.git",
            self.inner.origin_url, self.owner, self.repository
        )
    }

    fn attempt_clone_fallback(&mut self) -> anyhow::Result<bool> {
        match self.setup_fallback_driver(&self.generate_ssh_url()) {
            Ok(()) => Ok(true),
            Err(e) => {
                if e.is_instanceof::<RuntimeException>() {
                    self.fallback_driver = None;

                    self.inner.io.write_error(&format!(
                        "<error>Failed to clone the {} repository, try running in interactive mode so that you can enter your Bitbucket OAuth consumer credentials</error>",
                        self.generate_ssh_url()
                    ));
                }
                Err(e)
            }
        }
    }

    fn setup_fallback_driver(&mut self, url: &str) -> anyhow::Result<()> {
        let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
        repo_config.insert("url".into(), PhpMixed::string(url.to_string()));
        let mut driver = GitDriver::new(
            repo_config,
            self.inner.io.clone(),
            self.inner.config.clone(),
            self.inner.http_downloader.clone(),
            self.inner.process.clone(),
        );
        driver.initialize()?;
        self.fallback_driver = Some(Box::new(driver));
        Ok(())
    }

    fn parse_clone_urls(&mut self, clone_links: Option<PhpMixed>) {
        let list = match clone_links {
            Some(PhpMixed::List(l)) => l,
            _ => return,
        };
        for clone_link in list {
            if let PhpMixed::Array(m) = clone_link
                && m.get(B("name")).and_then(|v| v.as_string()) == Some("https".into())
            {
                // Format: https://(user@)bitbucket.org/{user}/{repo}
                // Strip username from URL (only present in clone URL's for private repositories)
                self.clone_https_url = legacy_lossy(&preg_replace(
                    php_regex!(r"/https:\/\/([^@]+@)?/"),
                    "https://",
                    m.get(B("href"))
                        .and_then(|v| v.as_string())
                        .unwrap_or_default(),
                ))
                .into_owned();
            }
        }
    }

    pub fn get_root_identifier(&mut self) -> anyhow::Result<String> {
        if let Some(fallback) = self.fallback_driver.as_mut() {
            return fallback.get_root_identifier();
        }

        if self.root_identifier.is_none() {
            if !self.get_repo_data()? {
                if self.fallback_driver.is_none() {
                    return Err(LogicException::new(
                        "A fallback driver should be setup if getRepoData returns false"
                            .to_string(),
                    )
                    .into());
                }

                return self.fallback_driver.as_mut().unwrap().get_root_identifier();
            }

            if self.vcs_type.as_deref() != Some("git") {
                return Err(RuntimeException::new(format!(
                    "{} does not appear to be a git repository, use {} but remember that Bitbucket no longer supports the mercurial repositories. https://bitbucket.org/blog/sunsetting-mercurial-support-in-bitbucket",
                    self.inner.url, self.clone_https_url
                ))
                .into());
            }

            self.root_identifier = self
                .repo_data
                .get(B("mainbranch"))
                .and_then(|v| match v {
                    PhpMixed::Array(m) => m.get(B("name")),
                    _ => None,
                })
                .and_then(|v| v.as_string())
                .map(|s| s.to_str_lossy().into_owned())
                .or_else(|| Some("master".to_string()));
        }

        Ok(self.root_identifier.clone().unwrap_or_default())
    }

    pub fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        _config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &str,
        _deep: bool,
    ) -> anyhow::Result<bool> {
        if !preg_is_match(
            php_regex!(r"#^https?://bitbucket\.org/([^/]+)/([^/]+?)(\.git|/?)?$#i"),
            url,
        ) {
            return Ok(false);
        }

        if !extension_loaded("openssl") {
            io.write_error3(
                &format!(
                    "Skipping Bitbucket git driver for {} because the OpenSSL PHP extension is missing.",
                    url
                ),
                true,
                io_interface::VERBOSE,
            );

            return Ok(false);
        }

        Ok(true)
    }
}

impl crate::repository::vcs::VcsDriverInterface for GitBitbucketDriver {
    fn initialize(&mut self) -> anyhow::Result<()> {
        GitBitbucketDriver::initialize(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        GitBitbucketDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(&mut self, file: &str, identifier: &str) -> anyhow::Result<Option<String>> {
        GitBitbucketDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        GitBitbucketDriver::get_change_date(self, identifier)
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<String> {
        GitBitbucketDriver::get_root_identifier(self)
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        GitBitbucketDriver::get_branches(self)
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        GitBitbucketDriver::get_tags(self)
    }

    fn get_dist(&self, identifier: &str) -> Option<IndexMap<String, String>> {
        GitBitbucketDriver::get_dist(self, identifier)
    }

    fn get_source(&self, identifier: &str) -> IndexMap<String, String> {
        GitBitbucketDriver::get_source(self, identifier)
    }

    fn get_url(&self) -> String {
        GitBitbucketDriver::get_url(self)
    }

    fn has_composer_file(&mut self, identifier: &str) -> anyhow::Result<bool> {
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
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool> {
        GitBitbucketDriver::supports(io, config, url, deep)
    }
}
