//! ref: composer/src/Composer/Repository/Vcs/ForgejoDriver.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, base64_decode, explode, extension_loaded, urlencode,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::repository::vcs::git_driver::GitDriver;
use crate::repository::vcs::vcs_driver::VcsDriver;
use crate::util::forgejo::Forgejo;
use crate::util::forgejo_repository_data::ForgejoRepositoryData;
use crate::util::forgejo_url::ForgejoUrl;
use crate::util::http::response::Response;

#[derive(Debug)]
pub struct ForgejoDriver {
    pub(crate) inner: VcsDriver,
    pub(crate) forgejo_url: Option<ForgejoUrl>,
    pub(crate) repository_data: Option<ForgejoRepositoryData>,
    pub(crate) git_driver: Option<GitDriver>,
    pub(crate) tags: Option<IndexMap<String, String>>,
    pub(crate) branches: Option<IndexMap<String, String>>,
}

impl ForgejoDriver {
    pub fn initialize(&mut self) -> Result<()> {
        let forgejo_url = ForgejoUrl::create(&self.inner.url)?;
        self.inner.origin_url = forgejo_url.origin_url.clone();

        let cache_dir = format!(
            "{}/{}/{}/{}",
            self.inner
                .config
                .get("cache-repo-dir")
                .as_string()
                .unwrap_or(""),
            forgejo_url.origin_url,
            forgejo_url.owner,
            forgejo_url.repository
        );
        self.forgejo_url = Some(forgejo_url);

        self.inner.cache = Some(Cache::new(&*self.inner.io, cache_dir));
        self.inner.cache.as_mut().map(|c| {
            c.set_read_only(
                self.inner
                    .config
                    .get("cache-read-only")
                    .as_bool()
                    .unwrap_or(false),
            )
        });

        self.fetch_repository_data()?;

        Ok(())
    }

    pub fn get_file_content(&mut self, file: &str, identifier: &str) -> Result<Option<String>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_file_content(file, identifier);
        }

        let api_url = self.forgejo_url.as_ref().unwrap().api_url.clone();
        let resource_url = format!(
            "{}/contents/{}?ref={}",
            api_url,
            file,
            urlencode(identifier)
        );
        let response = self
            .get_contents(&resource_url, false)
            .map_err(|e| anyhow::anyhow!("{}", e.message))?;
        let mut resource = response.decode_json()?;

        // The Forgejo contents API only returns files up to 1MB as base64 encoded files;
        // larger files either need be fetched with a raw accept header or by using the git blob endpoint.
        let needs_git_blob = if let PhpMixed::Array(ref arr) = resource {
            let content_empty = arr
                .get("content")
                .map_or(true, |v| v.as_string().map_or(true, |s| s.is_empty()));
            let encoding_none = arr
                .get("encoding")
                .and_then(|v| v.as_string())
                .map_or(false, |s| s == "none");
            let has_git_url = arr.contains_key("git_url");
            content_empty && encoding_none && has_git_url
        } else {
            false
        };

        if needs_git_blob {
            let git_url = if let PhpMixed::Array(ref arr) = resource {
                arr.get("git_url")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_owned())
            } else {
                None
            };
            if let Some(git_url) = git_url {
                resource = self
                    .get_contents(&git_url, false)
                    .map_err(|e| anyhow::anyhow!("{}", e.message))?
                    .decode_json()?;
            }
        }

        let content_b64 = if let PhpMixed::Array(ref arr) = resource {
            let has_content = arr.contains_key("content");
            let encoding_ok = arr
                .get("encoding")
                .and_then(|v| v.as_string())
                .map_or(false, |s| s == "base64");
            if has_content && encoding_ok {
                arr.get("content")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_owned())
            } else {
                None
            }
        } else {
            None
        };

        match content_b64 {
            Some(b64) => match base64_decode(&b64) {
                Some(bytes) => match String::from_utf8(bytes) {
                    Ok(s) => Ok(Some(s)),
                    Err(_) => Err(RuntimeException {
                        message: format!("Could not retrieve {} for {}", file, identifier),
                        code: 0,
                    }
                    .into()),
                },
                None => Err(RuntimeException {
                    message: format!("Could not retrieve {} for {}", file, identifier),
                    code: 0,
                }
                .into()),
            },
            None => Err(RuntimeException {
                message: format!("Could not retrieve {} for {}", file, identifier),
                code: 0,
            }
            .into()),
        }
    }

    pub fn get_change_date(
        &mut self,
        identifier: &str,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_change_date(identifier);
        }

        let api_url = self.forgejo_url.as_ref().unwrap().api_url.clone();
        let resource_url = format!(
            "{}/git/commits/{}?verification=false&files=false",
            api_url,
            urlencode(identifier)
        );
        let commit = self
            .get_contents(&resource_url, false)
            .map_err(|e| anyhow::anyhow!("{}", e.message))?
            .decode_json()?;

        let date_str = if let PhpMixed::Array(ref arr) = commit {
            arr.get("commit")
                .and_then(|v| v.as_array())
                .and_then(|c| c.get("committer"))
                .and_then(|v| v.as_array())
                .and_then(|c| c.get("date"))
                .and_then(|v| v.as_string())
                .map(|s| s.to_owned())
        } else {
            None
        };

        let date_str = date_str.ok_or_else(|| RuntimeException {
            message: format!("Could not parse commit date for {}", identifier),
            code: 0,
        })?;

        let date = chrono::DateTime::parse_from_rfc3339(&date_str)
            .map(|d| d.with_timezone(&chrono::Utc))?;
        Ok(Some(date))
    }

    pub fn get_root_identifier(&mut self) -> Result<String> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_root_identifier();
        }

        Ok(self
            .repository_data
            .as_ref()
            .unwrap()
            .default_branch
            .clone())
    }

    pub fn get_branches(&mut self) -> Result<IndexMap<String, String>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_branches();
        }

        if self.branches.is_none() {
            let mut branches = IndexMap::new();
            let api_url = self.forgejo_url.as_ref().unwrap().api_url.clone();
            let mut resource: Option<String> = Some(format!("{}/branches?per_page=100", api_url));

            while let Some(url) = resource {
                let response = self
                    .get_contents(&url, false)
                    .map_err(|e| anyhow::anyhow!("{}", e.message))?;
                let branch_data = response.decode_json()?;
                if let PhpMixed::List(ref list) = branch_data {
                    for branch in list {
                        if let PhpMixed::Array(ref arr) = **branch {
                            let name = arr
                                .get("name")
                                .and_then(|v| v.as_string())
                                .map(|s| s.to_owned());
                            let id = arr
                                .get("commit")
                                .and_then(|v| v.as_array())
                                .and_then(|c| c.get("id"))
                                .and_then(|v| v.as_string())
                                .map(|s| s.to_owned());
                            if let (Some(n), Some(i)) = (name, id) {
                                branches.insert(n, i);
                            }
                        }
                    }
                }
                resource = self.get_next_page(&response);
            }

            self.branches = Some(branches);
        }

        Ok(self.branches.clone().unwrap_or_default())
    }

    pub fn get_tags(&mut self) -> Result<IndexMap<String, String>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_tags();
        }

        if self.tags.is_none() {
            let mut tags = IndexMap::new();
            let api_url = self.forgejo_url.as_ref().unwrap().api_url.clone();
            let mut resource: Option<String> = Some(format!("{}/tags?per_page=100", api_url));

            while let Some(url) = resource {
                let response = self
                    .get_contents(&url, false)
                    .map_err(|e| anyhow::anyhow!("{}", e.message))?;
                let tags_data = response.decode_json()?;
                if let PhpMixed::List(ref list) = tags_data {
                    for tag in list {
                        if let PhpMixed::Array(ref arr) = **tag {
                            let name = arr
                                .get("name")
                                .and_then(|v| v.as_string())
                                .map(|s| s.to_owned());
                            let sha = arr
                                .get("commit")
                                .and_then(|v| v.as_array())
                                .and_then(|c| c.get("sha"))
                                .and_then(|v| v.as_string())
                                .map(|s| s.to_owned());
                            if let (Some(n), Some(s)) = (name, sha) {
                                tags.insert(n, s);
                            }
                        }
                    }
                }
                resource = self.get_next_page(&response);
            }

            self.tags = Some(tags);
        }

        Ok(self.tags.clone().unwrap_or_default())
    }

    pub fn get_dist(&self, identifier: &str) -> Option<IndexMap<String, String>> {
        let url = format!(
            "{}/archive/{}.zip",
            self.forgejo_url.as_ref().unwrap().api_url,
            identifier
        );
        let mut map = IndexMap::new();
        map.insert("type".to_string(), "zip".to_string());
        map.insert("url".to_string(), url);
        map.insert("reference".to_string(), identifier.to_string());
        map.insert("shasum".to_string(), "".to_string());
        Some(map)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> Result<Option<IndexMap<String, PhpMixed>>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.inner.get_composer_information(identifier);
        }

        if !self.inner.info_cache.contains_key(identifier) {
            let composer = if self.inner.should_cache(identifier) {
                if let Some(res) = self.inner.cache.as_ref().and_then(|c| c.read(identifier)) {
                    JsonFile::parse_json(&res, None)?
                } else {
                    let c = self.inner.get_base_composer_information(identifier)?;
                    if self.inner.should_cache(identifier) {
                        if let Some(ref composer_map) = c {
                            let encoded = JsonFile::encode_with_options(
                                composer_map,
                                shirabe_php_shim::JSON_UNESCAPED_UNICODE
                                    | shirabe_php_shim::JSON_UNESCAPED_SLASHES,
                            );
                            self.inner
                                .cache
                                .as_ref()
                                .map(|c| c.write(identifier, &encoded));
                        }
                    }
                    c
                }
            } else {
                self.inner.get_base_composer_information(identifier)?
            };

            let mut composer = composer;

            if let Some(ref mut composer_map) = composer {
                // specials for forgejo
                let support_not_array = composer_map
                    .get("support")
                    .map_or(false, |v| v.as_array().is_none());
                if support_not_array {
                    composer_map.insert("support".to_string(), PhpMixed::Array(IndexMap::new()));
                }

                let has_source = composer_map
                    .get("support")
                    .and_then(|v| v.as_array())
                    .map_or(false, |arr| arr.contains_key("source"));

                if !has_source {
                    let html_url = self
                        .repository_data
                        .as_ref()
                        .map(|r| r.html_url.clone())
                        .unwrap_or_default();

                    let tags = self.get_tags()?;
                    let branches = self.get_branches()?;

                    let source_url = if let Some(label) = tags
                        .into_iter()
                        .find(|(_, v)| v == identifier)
                        .map(|(k, _)| k)
                    {
                        format!("{}/tag/{}", html_url, label)
                    } else if let Some(label) = branches
                        .into_iter()
                        .find(|(_, v)| v == identifier)
                        .map(|(k, _)| k)
                    {
                        format!("{}/branch/{}", html_url, label)
                    } else {
                        format!("{}/commit/{}", html_url, identifier)
                    };

                    if let Some(PhpMixed::Array(ref mut support)) = composer_map.get_mut("support")
                    {
                        support
                            .insert("source".to_string(), Box::new(PhpMixed::String(source_url)));
                    }
                }

                let has_issues = composer_map
                    .get("support")
                    .and_then(|v| v.as_array())
                    .map_or(false, |arr| arr.contains_key("issues"));

                if !has_issues
                    && self
                        .repository_data
                        .as_ref()
                        .map_or(false, |r| r.has_issues)
                {
                    let issues_url = format!(
                        "{}/issues",
                        self.repository_data
                            .as_ref()
                            .map(|r| r.html_url.clone())
                            .unwrap_or_default()
                    );
                    if let Some(PhpMixed::Array(ref mut support)) = composer_map.get_mut("support")
                    {
                        support
                            .insert("issues".to_string(), Box::new(PhpMixed::String(issues_url)));
                    }
                }

                if !composer_map.contains_key("abandoned")
                    && self
                        .repository_data
                        .as_ref()
                        .map_or(false, |r| r.is_archived)
                {
                    composer_map.insert("abandoned".to_string(), PhpMixed::Bool(true));
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
            .and_then(|v| v.clone()))
    }

    pub fn get_source(&mut self, identifier: &str) -> IndexMap<String, String> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_source(identifier);
        }

        let mut map = IndexMap::new();
        map.insert("type".to_string(), "git".to_string());
        map.insert("url".to_string(), self.get_url());
        map.insert("reference".to_string(), identifier.to_string());
        map
    }

    pub fn get_url(&self) -> String {
        if let Some(ref git_driver) = self.git_driver {
            return git_driver.get_url();
        }

        let repo = self.repository_data.as_ref().unwrap();
        if repo.is_private {
            repo.ssh_url.clone()
        } else {
            repo.http_clone_url.clone()
        }
    }

    pub fn supports(io: &dyn IOInterface, config: &Config, url: &str, _deep: bool) -> bool {
        let forgejo_url = ForgejoUrl::try_from(Some(url));
        if forgejo_url.is_none() {
            return false;
        }
        let forgejo_url = forgejo_url.unwrap();

        let forgejo_domains = config.get("forgejo-domains");
        let in_domains = if let Some(list) = forgejo_domains.as_list() {
            list.iter().any(|d| {
                d.as_string().map_or(false, |s| {
                    s.to_lowercase() == forgejo_url.origin_url.to_lowercase()
                })
            })
        } else {
            false
        };
        if !in_domains {
            return false;
        }

        if !extension_loaded("openssl") {
            io.write_error(
                PhpMixed::String(format!(
                    "Skipping Forgejo driver for {} because the OpenSSL PHP extension is missing.",
                    url
                )),
                true,
                IOInterface::VERBOSE,
            );

            return false;
        }

        true
    }

    fn setup_git_driver(&mut self, url: &str) -> Result<()> {
        let mut git_driver = GitDriver {
            inner: VcsDriver::new(
                {
                    let mut m = IndexMap::new();
                    m.insert("url".to_string(), PhpMixed::String(url.to_string()));
                    m
                },
                todo!("clone io for GitDriver setup"),
                self.inner.config.clone(),
                self.inner.http_downloader.clone(),
                self.inner.process.clone(),
            ),
            tags: None,
            branches: None,
            root_identifier: None,
            repo_dir: String::new(),
        };
        git_driver.initialize()?;
        self.git_driver = Some(git_driver);
        Ok(())
    }

    fn fetch_repository_data(&mut self) -> Result<()> {
        if self.repository_data.is_some() {
            return Ok(());
        }

        let api_url = self.forgejo_url.as_ref().unwrap().api_url.clone();
        match self.get_contents(&api_url, true) {
            Err(_) => {
                if self.git_driver.is_some() {
                    return Ok(());
                }
                return Err(anyhow::anyhow!("Failed to fetch repository data"));
            }
            Ok(response) => {
                let data = response.decode_json()?;
                if data.is_null() && self.git_driver.is_some() {
                    return Ok(());
                }
                if let PhpMixed::Array(ref arr) = data {
                    let map: IndexMap<String, PhpMixed> =
                        arr.iter().map(|(k, v)| (k.clone(), *v.clone())).collect();
                    self.repository_data = Some(ForgejoRepositoryData::from_remote_data(&map)?);
                }
            }
        }

        Ok(())
    }

    fn get_next_page(&self, response: &Response) -> Option<String> {
        let header = response.get_header("link")?;

        let links = explode(",", &header);
        for link in links {
            if let Some(m) = Preg::match_strict_groups(r#"{<(.+?)>; *rel="next"}"#, &link) {
                if let Some(url) = m.get("1") {
                    return Some(url.clone());
                }
            }
        }

        None
    }

    pub(crate) fn get_contents(
        &mut self,
        url: &str,
        fetching_repo_data: bool,
    ) -> anyhow::Result<Response, TransportException> {
        match self.inner.get_contents(url) {
            Ok(response) => Ok(response),
            Err(e) => match e.get_code() {
                401 | 403 | 404 | 429 => {
                    if !fetching_repo_data {
                        return Err(e);
                    }

                    if !self.inner.io.is_interactive() {
                        self.attempt_clone_fallback()
                            .map_err(|inner_e| TransportException {
                                message: inner_e.to_string(),
                                code: 0,
                                headers: None,
                                response: None,
                                status_code: None,
                                response_info: vec![],
                            })?;

                        return Ok(Response::new(
                            {
                                let mut m = IndexMap::new();
                                m.insert("url".to_string(), PhpMixed::String("dummy".to_string()));
                                m
                            },
                            Some(200),
                            vec![],
                            Some("null".to_string()),
                        )
                        .unwrap()
                        .unwrap());
                    }

                    if !self.inner.io.has_authentication(&self.inner.origin_url) {
                        let origin_url = self.forgejo_url.as_ref().unwrap().origin_url.clone();
                        let message = if e.get_code() == 429 {
                            Some(format!(
                                "API limit exhausted. Enter your Forgejo credentials to get a larger API limit (<info>{}</info>)",
                                self.inner.url
                            ))
                        } else {
                            None
                        };

                        let mut forgejo = Forgejo::new(
                            todo!("clone io for Forgejo OAuth"),
                            self.inner.config.clone(),
                            self.inner.http_downloader.clone(),
                        );
                        let auth_result = forgejo
                            .authorize_o_auth_interactively(&origin_url, message.as_deref())
                            .map_err(|inner_e| TransportException {
                                message: inner_e.to_string(),
                                code: 0,
                                headers: None,
                                response: None,
                                status_code: None,
                                response_info: vec![],
                            })?;

                        if let Ok(true) = auth_result {
                            return self.inner.get_contents(url);
                        }
                    }

                    Err(e)
                }
                _ => Err(e),
            },
        }
    }

    // Returns true on success, throws on failure.
    fn attempt_clone_fallback(&mut self) -> anyhow::Result<bool> {
        let ssh_url = self.forgejo_url.as_ref().unwrap().generate_ssh_url();
        match self.setup_git_driver(&ssh_url) {
            Ok(()) => Ok(true),
            Err(e) => {
                self.git_driver = None;
                self.inner.io.write_error(
                    PhpMixed::String(format!(
                        "<error>Failed to clone the {} repository, try running in interactive mode so that you can enter your Forgejo credentials</error>",
                        ssh_url
                    )),
                    true,
                    IOInterface::NORMAL,
                );
                Err(e)
            }
        }
    }
}
