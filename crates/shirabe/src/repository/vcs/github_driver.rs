//! ref: composer/src/Composer/Repository/Vcs/GitHubDriver.php

use anyhow::Result;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, array_diff, array_key_exists, array_map,
    array_search_mixed, base64_decode, basename, count, empty, explode, extension_loaded, in_array,
    parse_url_all, sprintf, strpos, strtolower, substr, trim, urlencode,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::repository::vcs::git_driver::GitDriver;
use crate::repository::vcs::vcs_driver::VcsDriver;
use crate::util::github::GitHub;
use crate::util::http::response::Response;

#[derive(Debug)]
pub struct GitHubDriver {
    pub(crate) inner: VcsDriver,
    pub(crate) owner: String,
    pub(crate) repository: String,
    /// @var array<int|string, string> Map of tag name to identifier
    pub(crate) tags: Option<IndexMap<String, String>>,
    /// @var array<int|string, string> Map of branch name to identifier
    pub(crate) branches: Option<IndexMap<String, String>>,
    pub(crate) root_identifier: String,
    /// @var mixed[]
    pub(crate) repo_data: Option<IndexMap<String, PhpMixed>>,
    pub(crate) has_issues: bool,
    pub(crate) is_private: bool,
    is_archived: bool,
    /// @var array<int, array{type: string, url: string}>|false|null
    funding_info: Option<PhpMixed>,
    allow_git_fallback: bool,
    /// Git Driver
    pub(crate) git_driver: Option<GitDriver>,
}

impl GitHubDriver {
    pub fn initialize(&mut self) -> Result<()> {
        let match_ = match Preg::is_match_strict_groups(
            r"#^(?:(?:https?|git)://([^/]+)/|git@([^:]+):/?)([^/]+)/([^/]+?)(?:\.git|/)?$#",
            &self.inner.url,
        ) {
            Some(m) => m,
            None => {
                return Err(InvalidArgumentException {
                    message: sprintf(
                        "The GitHub repository URL %s is invalid.",
                        &[PhpMixed::String(self.inner.url.clone())],
                    ),
                    code: 0,
                }
                .into());
            }
        };

        self.owner = match_.get(3).cloned().unwrap_or_default();
        self.repository = match_.get(4).cloned().unwrap_or_default();
        self.inner.origin_url = strtolower(
            &match_
                .get(1)
                .cloned()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| match_.get(2).cloned().unwrap_or_default()),
        );
        if self.inner.origin_url == "www.github.com" {
            self.inner.origin_url = "github.com".to_string();
        }
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
                self.owner,
                self.repository
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

        if self
            .inner
            .repo_config
            .get("allow-git-fallback")
            .and_then(|v| v.as_bool())
            == Some(false)
        {
            self.allow_git_fallback = false;
        }

        if self.inner.config.get("use-github-api").as_bool() == Some(false)
            || self
                .inner
                .repo_config
                .get("no-api")
                .and_then(|v| v.as_bool())
                == Some(true)
        {
            self.setup_git_driver(&self.inner.url.clone())?;

            return Ok(());
        }

        self.fetch_root_identifier()
    }

    pub fn get_repository_url(&self) -> String {
        format!(
            "https://{}/{}/{}",
            self.inner.origin_url, self.owner, self.repository
        )
    }

    pub fn get_root_identifier(&mut self) -> Result<String> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_root_identifier();
        }

        Ok(self.root_identifier.clone())
    }

    pub fn get_url(&self) -> String {
        if let Some(ref git_driver) = self.git_driver {
            return git_driver.get_url();
        }

        format!(
            "https://{}/{}/{}.git",
            self.inner.origin_url, self.owner, self.repository
        )
    }

    pub(crate) fn get_api_url(&self) -> String {
        let api_url = if self.inner.origin_url == "github.com" {
            "api.github.com".to_string()
        } else {
            format!("{}/api/v3", self.inner.origin_url)
        };

        format!("https://{}", api_url)
    }

    pub fn get_source(&self, identifier: &str) -> IndexMap<String, PhpMixed> {
        if let Some(ref git_driver) = self.git_driver {
            return git_driver.get_source(identifier);
        }
        let url = if self.is_private {
            // Private GitHub repositories should be accessed using the
            // SSH version of the URL.
            self.generate_ssh_url()
        } else {
            self.get_url()
        };

        let mut result = IndexMap::new();
        result.insert("type".to_string(), PhpMixed::String("git".to_string()));
        result.insert("url".to_string(), PhpMixed::String(url));
        result.insert(
            "reference".to_string(),
            PhpMixed::String(identifier.to_string()),
        );
        result
    }

    pub fn get_dist(&self, identifier: &str) -> Option<IndexMap<String, PhpMixed>> {
        let url = format!(
            "{}/repos/{}/{}/zipball/{}",
            self.get_api_url(),
            self.owner,
            self.repository,
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
                // specials for github
                if composer.contains_key("support")
                    && !matches!(composer.get("support"), Some(PhpMixed::Array(_)))
                {
                    composer.insert("support".to_string(), PhpMixed::Array(IndexMap::new()));
                }
                let support_source_missing = !composer
                    .get("support")
                    .and_then(|v| v.as_array())
                    .map(|m| m.contains_key("source"))
                    .unwrap_or(false);
                if support_source_missing {
                    let tags_map = self.get_tags()?;
                    let branches_map = self.get_branches()?;
                    let label = array_search_mixed(
                        &PhpMixed::String(identifier.to_string()),
                        &PhpMixed::Array(
                            tags_map
                                .into_iter()
                                .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                                .collect(),
                        ),
                        false,
                    )
                    .filter(|v| !matches!(v, PhpMixed::Bool(false) | PhpMixed::Null))
                    .or_else(|| {
                        array_search_mixed(
                            &PhpMixed::String(identifier.to_string()),
                            &PhpMixed::Array(
                                branches_map
                                    .into_iter()
                                    .map(|(k, v)| (k, Box::new(PhpMixed::String(v))))
                                    .collect(),
                            ),
                            false,
                        )
                    })
                    .filter(|v| !matches!(v, PhpMixed::Bool(false) | PhpMixed::Null))
                    .unwrap_or_else(|| PhpMixed::String(identifier.to_string()));
                    let label_str = label.as_string().unwrap_or(identifier).to_string();
                    if let Some(support) = composer.get_mut("support").and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m),
                        _ => None,
                    }) {
                        support.insert(
                            "source".to_string(),
                            Box::new(PhpMixed::String(sprintf(
                                "https://%s/%s/%s/tree/%s",
                                &[
                                    PhpMixed::String(self.inner.origin_url.clone()),
                                    PhpMixed::String(self.owner.clone()),
                                    PhpMixed::String(self.repository.clone()),
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
                if issues_missing && self.has_issues {
                    if let Some(support) = composer.get_mut("support").and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m),
                        _ => None,
                    }) {
                        support.insert(
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
                if !composer.contains_key("abandoned") && self.is_archived {
                    composer.insert("abandoned".to_string(), PhpMixed::Bool(true));
                }
                if !composer.contains_key("funding") {
                    let funding = self.get_funding_info();
                    if !matches!(funding, PhpMixed::Bool(false)) {
                        composer.insert("funding".to_string(), funding);
                    }
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

    /// @return array<int, array{type: string, url: string}>|false
    fn get_funding_info(&mut self) -> PhpMixed {
        if let Some(ref info) = self.funding_info {
            return info.clone();
        }

        if self.inner.origin_url != "github.com" {
            self.funding_info = Some(PhpMixed::Bool(false));
            return PhpMixed::Bool(false);
        }

        let mut funding: Option<Vec<u8>> = None;
        for file_url in &[
            format!(
                "{}/repos/{}/{}/contents/.github/FUNDING.yml",
                self.get_api_url(),
                self.owner,
                self.repository
            ),
            format!(
                "{}/repos/{}/.github/contents/FUNDING.yml",
                self.get_api_url(),
                self.owner
            ),
        ] {
            let mut options: IndexMap<String, PhpMixed> = IndexMap::new();
            options.insert("retry-auth-failure".to_string(), PhpMixed::Bool(false));
            let response = self.inner.http_downloader.get(
                file_url,
                &PhpMixed::Array(options.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
            );
            let response = match response {
                Ok(r) => r,
                Err(_) => continue,
            };
            let response_json = response.decode_json();
            let response_json = match response_json {
                Ok(j) => j,
                Err(_) => continue,
            };
            let response_map = match response_json {
                PhpMixed::Array(ref m) => m.clone(),
                _ => continue,
            };
            let content_empty = response_map
                .get("content")
                .and_then(|v| v.as_string())
                .map(|s| s.is_empty())
                .unwrap_or(true);
            let encoding_not_base64 =
                response_map.get("encoding").and_then(|v| v.as_string()) != Some("base64");
            if content_empty || encoding_not_base64 {
                continue;
            }
            let decoded = base64_decode(
                response_map
                    .get("content")
                    .and_then(|v| v.as_string())
                    .unwrap_or(""),
            );
            match decoded {
                Some(b) if !b.is_empty() => {
                    funding = Some(b);
                    break;
                }
                _ => continue,
            }
        }

        let funding = match funding {
            Some(f) => String::from_utf8_lossy(&f).to_string(),
            None => {
                self.funding_info = Some(PhpMixed::Bool(false));
                return PhpMixed::Bool(false);
            }
        };

        let mut result: Vec<IndexMap<String, PhpMixed>> = vec![];
        let mut key: Option<String> = None;
        for line in Preg::split(r"{\r?\n}", &funding) {
            let line = trim(&line, None);
            if let Some(m) = Preg::is_match_strict_groups(r"{^(\w+)\s*:\s*(.+)$}", &line) {
                let g1 = m.get(1).cloned().unwrap_or_default();
                let g2 = m.get(2).cloned().unwrap_or_default();
                if g2 == "[" {
                    key = Some(g1);
                    continue;
                }
                if let Some(m2) = Preg::is_match_strict_groups(r"{^\[(.*?)\](?:\s*#.*)?$}", &g2) {
                    let inner = m2.get(1).cloned().unwrap_or_default();
                    for item in array_map(
                        |s: &String| trim(s, None),
                        &Preg::split(r#"{[\'\"]?\s*,\s*[\'\"]?}"#, &inner),
                    ) {
                        let mut entry = IndexMap::new();
                        entry.insert("type".to_string(), PhpMixed::String(g1.clone()));
                        entry.insert(
                            "url".to_string(),
                            PhpMixed::String(trim(&item, Some("\"' "))),
                        );
                        result.push(entry);
                    }
                } else if let Some(m2) =
                    Preg::is_match_strict_groups(r"{^([^#].*?)(?:\s+#.*)?$}", &g2)
                {
                    let mut entry = IndexMap::new();
                    entry.insert("type".to_string(), PhpMixed::String(g1.clone()));
                    entry.insert(
                        "url".to_string(),
                        PhpMixed::String(trim(
                            &m2.get(1).cloned().unwrap_or_default(),
                            Some("\"' "),
                        )),
                    );
                    result.push(entry);
                }
                key = None;
            } else if let Some(m) = Preg::is_match_strict_groups(r"{^(\w+)\s*:\s*#\s*$}", &line) {
                key = Some(m.get(1).cloned().unwrap_or_default());
            } else if key.is_some()
                && (Preg::is_match_strict_groups(r"{^-\s*(.+)(?:\s+#.*)?$}", &line).is_some()
                    || Preg::is_match_strict_groups(r"{^(.+),(?:\s*#.*)?$}", &line).is_some())
            {
                let m = Preg::is_match_strict_groups(r"{^-\s*(.+)(?:\s+#.*)?$}", &line)
                    .or_else(|| Preg::is_match_strict_groups(r"{^(.+),(?:\s*#.*)?$}", &line))
                    .unwrap();
                let mut entry = IndexMap::new();
                entry.insert(
                    "type".to_string(),
                    PhpMixed::String(key.clone().unwrap_or_default()),
                );
                entry.insert(
                    "url".to_string(),
                    PhpMixed::String(trim(&m.get(1).cloned().unwrap_or_default(), Some("\"' "))),
                );
                result.push(entry);
            } else if key.is_some() && line == "]" {
                key = None;
            }
        }

        let mut keys_to_remove: Vec<usize> = vec![];
        let mut result_for_iter: Vec<IndexMap<String, PhpMixed>> = result.clone();
        for (key_idx, item) in result_for_iter.iter_mut().enumerate() {
            let item_type = item
                .get("type")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            let item_url = item
                .get("url")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            match item_type.as_str() {
                "community_bridge" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!(
                            "https://funding.communitybridge.org/projects/{}",
                            basename(&item_url)
                        )),
                    );
                }
                "github" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!("https://github.com/{}", basename(&item_url))),
                    );
                }
                "issuehunt" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!("https://issuehunt.io/r/{}", item_url)),
                    );
                }
                "ko_fi" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!("https://ko-fi.com/{}", basename(&item_url))),
                    );
                }
                "liberapay" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!("https://liberapay.com/{}", basename(&item_url))),
                    );
                }
                "open_collective" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!(
                            "https://opencollective.com/{}",
                            basename(&item_url)
                        )),
                    );
                }
                "patreon" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!(
                            "https://www.patreon.com/{}",
                            basename(&item_url)
                        )),
                    );
                }
                "tidelift" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!(
                            "https://tidelift.com/funding/github/{}",
                            item_url
                        )),
                    );
                }
                "polar" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!("https://polar.sh/{}", basename(&item_url))),
                    );
                }
                "buy_me_a_coffee" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!(
                            "https://www.buymeacoffee.com/{}",
                            basename(&item_url)
                        )),
                    );
                }
                "thanks_dev" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!("https://thanks.dev/{}", item_url)),
                    );
                }
                "otechie" => {
                    result[key_idx].insert(
                        "url".to_string(),
                        PhpMixed::String(format!("https://otechie.com/{}", basename(&item_url))),
                    );
                }
                "custom" => {
                    let bits = parse_url_all(&item_url);
                    if matches!(bits, PhpMixed::Bool(false)) {
                        keys_to_remove.push(key_idx);
                        continue;
                    }

                    let bits_map = match bits {
                        PhpMixed::Array(m) => m,
                        _ => IndexMap::new(),
                    };
                    if !array_key_exists("scheme", &bits_map)
                        && !array_key_exists("host", &bits_map)
                    {
                        if Preg::is_match(r"{^[a-z0-9-]++\.[a-z]{2,3}$}", &item_url)
                            .unwrap_or(false)
                        {
                            result[key_idx].insert(
                                "url".to_string(),
                                PhpMixed::String(format!("https://{}", item_url)),
                            );
                            continue;
                        }

                        self.inner.io.write_error(
                            PhpMixed::String(format!(
                                "<warning>Funding URL {} not in a supported format.</warning>",
                                item_url
                            )),
                            true,
                            IOInterface::NORMAL,
                        );
                        keys_to_remove.push(key_idx);
                    }
                }
                _ => {}
            }
        }
        // remove items flagged for deletion (in reverse to preserve indices)
        for key_idx in keys_to_remove.into_iter().rev() {
            result.remove(key_idx);
        }

        let result_mixed = PhpMixed::List(
            result
                .into_iter()
                .map(|m| {
                    Box::new(PhpMixed::Array(
                        m.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                    ))
                })
                .collect(),
        );
        self.funding_info = Some(result_mixed.clone());
        result_mixed
    }

    pub fn get_file_content(&mut self, file: &str, identifier: &str) -> Result<Option<String>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_file_content(file, identifier);
        }

        let resource_url = format!(
            "{}/repos/{}/{}/contents/{}?ref={}",
            self.get_api_url(),
            self.owner,
            self.repository,
            file,
            urlencode(identifier)
        );
        let mut resource = self
            .get_contents(&resource_url, false)
            .map_err(|e| anyhow::anyhow!("{}", e.message))?
            .decode_json()?;

        // The GitHub contents API only returns files up to 1MB as base64 encoded files
        // larger files either need be fetched with a raw accept header or by using the git blob endpoint
        let resource_map = match resource {
            PhpMixed::Array(ref m) => m.clone(),
            _ => IndexMap::new(),
        };
        let needs_git_url = (resource_map
            .get("content")
            .and_then(|v| v.as_string())
            .is_none()
            || resource_map
                .get("content")
                .and_then(|v| v.as_string())
                .map(|s| s.is_empty())
                .unwrap_or(false))
            && resource_map.get("encoding").and_then(|v| v.as_string()) == Some("none")
            && resource_map.contains_key("git_url");
        if needs_git_url {
            let git_url = resource_map
                .get("git_url")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            resource = self
                .get_contents(&git_url, false)
                .map_err(|e| anyhow::anyhow!("{}", e.message))?
                .decode_json()?;
        }

        let resource_map = match resource {
            PhpMixed::Array(m) => m,
            _ => IndexMap::new(),
        };
        let has_content = resource_map.contains_key("content");
        let encoding_base64 =
            resource_map.get("encoding").and_then(|v| v.as_string()) == Some("base64");
        let content = if has_content && encoding_base64 {
            base64_decode(
                resource_map
                    .get("content")
                    .and_then(|v| v.as_string())
                    .unwrap_or(""),
            )
        } else {
            None
        };
        let content = match content {
            Some(c) => String::from_utf8_lossy(&c).to_string(),
            None => {
                return Err(RuntimeException {
                    message: format!("Could not retrieve {} for {}", file, identifier),
                    code: 0,
                }
                .into());
            }
        };

        Ok(Some(content))
    }

    pub fn get_change_date(&mut self, identifier: &str) -> Result<Option<DateTime<Utc>>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_change_date(identifier);
        }

        let resource = format!(
            "{}/repos/{}/{}/commits/{}",
            self.get_api_url(),
            self.owner,
            self.repository,
            urlencode(identifier)
        );
        let commit = self
            .get_contents(&resource, false)
            .map_err(|e| anyhow::anyhow!("{}", e.message))?
            .decode_json()?;

        let date_str = match commit {
            PhpMixed::Array(m) => m
                .get("commit")
                .and_then(|v| v.as_array())
                .and_then(|c| c.get("committer"))
                .and_then(|v| v.as_array())
                .and_then(|c| c.get("date"))
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string(),
            _ => String::new(),
        };

        Ok(Some(
            DateTime::parse_from_rfc3339(&date_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        ))
    }

    pub fn get_tags(&mut self) -> Result<IndexMap<String, String>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_tags();
        }
        if self.tags.is_none() {
            let mut tags: IndexMap<String, String> = IndexMap::new();
            let mut resource: Option<String> = Some(format!(
                "{}/repos/{}/{}/tags?per_page=100",
                self.get_api_url(),
                self.owner,
                self.repository
            ));

            loop {
                let response = self
                    .get_contents(resource.as_deref().unwrap_or(""), false)
                    .map_err(|e| anyhow::anyhow!("{}", e.message))?;
                let tags_data = response.decode_json()?;
                if let PhpMixed::List(ref list) = tags_data {
                    for tag in list {
                        if let PhpMixed::Array(ref tag_map) = **tag {
                            let name = tag_map
                                .get("name")
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string();
                            let sha = tag_map
                                .get("commit")
                                .and_then(|v| v.as_array())
                                .and_then(|m| m.get("sha"))
                                .and_then(|v| v.as_string())
                                .unwrap_or("")
                                .to_string();
                            tags.insert(name, sha);
                        }
                    }
                }

                resource = self.get_next_page(&response);
                if resource.is_none() {
                    break;
                }
            }

            self.tags = Some(tags);
        }

        Ok(self.tags.clone().unwrap_or_default())
    }

    pub fn get_branches(&mut self) -> Result<IndexMap<String, String>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_branches();
        }
        if self.branches.is_none() {
            let mut branches: IndexMap<String, String> = IndexMap::new();
            let mut resource: Option<String> = Some(format!(
                "{}/repos/{}/{}/git/refs/heads?per_page=100",
                self.get_api_url(),
                self.owner,
                self.repository
            ));

            loop {
                let response = self
                    .get_contents(resource.as_deref().unwrap_or(""), false)
                    .map_err(|e| anyhow::anyhow!("{}", e.message))?;
                let branch_data = response.decode_json()?;
                if let PhpMixed::List(ref list) = branch_data {
                    for branch in list {
                        if let PhpMixed::Array(ref branch_map) = **branch {
                            let ref_str = branch_map
                                .get("ref")
                                .and_then(|v| v.as_string())
                                .unwrap_or("");
                            let name = substr(ref_str, 11, None);
                            if name != "gh-pages" {
                                let sha = branch_map
                                    .get("object")
                                    .and_then(|v| v.as_array())
                                    .and_then(|m| m.get("sha"))
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                                    .to_string();
                                branches.insert(name, sha);
                            }
                        }
                    }
                }

                resource = self.get_next_page(&response);
                if resource.is_none() {
                    break;
                }
            }

            self.branches = Some(branches);
        }

        Ok(self.branches.clone().unwrap_or_default())
    }

    pub fn supports(io: &dyn IOInterface, config: &Config, url: &str, _deep: bool) -> bool {
        let matches = match Preg::is_match_strict_groups(
            r"#^((?:https?|git)://([^/]+)/|git@([^:]+):/?)([^/]+)/([^/]+?)(?:\.git|/)?$#",
            url,
        ) {
            Some(m) => m,
            None => return false,
        };

        let origin_url = matches
            .get(2)
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| matches.get(3).cloned().unwrap_or_default());
        if !in_array(
            PhpMixed::String(strtolower(&Preg::replace(r"{^www\.}i", "", origin_url))),
            &config.get("github-domains"),
            false,
        ) {
            return false;
        }

        if !extension_loaded("openssl") {
            io.write_error(
                PhpMixed::String(format!(
                    "Skipping GitHub driver for {} because the OpenSSL PHP extension is missing.",
                    url
                )),
                true,
                IOInterface::VERBOSE,
            );

            return false;
        }

        true
    }

    /// Gives back the loaded <github-api>/repos/<owner>/<repo> result
    ///
    /// @return mixed[]|null
    pub fn get_repo_data(&mut self) -> Result<Option<IndexMap<String, PhpMixed>>> {
        self.fetch_root_identifier()?;

        Ok(self.repo_data.clone())
    }

    /// Generate an SSH URL
    pub(crate) fn generate_ssh_url(&self) -> String {
        if strpos(&self.inner.origin_url, ":").is_some() {
            return format!(
                "ssh://git@{}/{}/{}.git",
                self.inner.origin_url, self.owner, self.repository
            );
        }

        format!(
            "git@{}:{}/{}.git",
            self.inner.origin_url, self.owner, self.repository
        )
    }

    pub(crate) fn get_contents(
        &mut self,
        url: &str,
        fetching_repo_data: bool,
    ) -> Result<Response, TransportException> {
        let response_result = self.inner.get_contents(url);
        match response_result {
            Ok(r) => Ok(r),
            Err(e) => {
                let mut git_hub_util = GitHub::new(
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

                        if git_hub_util.authorize_oauth(&self.inner.origin_url) {
                            return self.inner.get_contents(url);
                        }

                        if !self.inner.io.is_interactive() {
                            self.attempt_clone_fallback(Some(&e)).map_err(|err| {
                                TransportException {
                                    message: err.to_string(),
                                    code: 0,
                                }
                            })?;

                            let mut req = IndexMap::new();
                            req.insert("url".to_string(), PhpMixed::String("dummy".to_string()));
                            return Ok(Response::new(
                                req,
                                Some(200),
                                vec![],
                                Some("null".to_string()),
                            )
                            .unwrap()
                            .unwrap());
                        }

                        let mut scopes_issued: Vec<String> = vec![];
                        let mut scopes_needed: Vec<String> = vec![];
                        let headers = e.get_headers().cloned().unwrap_or_default();
                        if !headers.is_empty() {
                            if let Some(scopes) =
                                Response::find_header_value(&headers, "X-OAuth-Scopes")
                            {
                                scopes_issued = explode(" ", &scopes);
                            }
                            if let Some(scopes) =
                                Response::find_header_value(&headers, "X-Accepted-OAuth-Scopes")
                            {
                                scopes_needed = explode(" ", &scopes);
                            }
                        }
                        let scopes_failed = array_diff(&scopes_needed, &scopes_issued);
                        // non-authenticated requests get no scopesNeeded, so ask for credentials
                        // authenticated requests which failed some scopes should ask for new credentials too
                        if headers.is_empty()
                            || count(&PhpMixed::List(
                                scopes_needed
                                    .iter()
                                    .map(|s| Box::new(PhpMixed::String(s.clone())))
                                    .collect(),
                            )) == 0
                            || count(&PhpMixed::List(
                                scopes_failed
                                    .iter()
                                    .map(|s| Box::new(PhpMixed::String(s.clone())))
                                    .collect(),
                            )) > 0
                        {
                            git_hub_util.authorize_oauth_interactively(
                                &self.inner.origin_url,
                                Some(&format!(
                                    "Your GitHub credentials are required to fetch private repository metadata (<info>{}</info>)",
                                    self.inner.url
                                )),
                            );
                        }

                        self.inner.get_contents(url)
                    }
                    403 => {
                        if !self.inner.io.has_authentication(&self.inner.origin_url)
                            && git_hub_util.authorize_oauth(&self.inner.origin_url)
                        {
                            return self.inner.get_contents(url);
                        }

                        if !self.inner.io.is_interactive() && fetching_repo_data {
                            self.attempt_clone_fallback(Some(&e)).map_err(|err| {
                                TransportException {
                                    message: err.to_string(),
                                    code: 0,
                                }
                            })?;

                            let mut req = IndexMap::new();
                            req.insert("url".to_string(), PhpMixed::String("dummy".to_string()));
                            return Ok(Response::new(
                                req,
                                Some(200),
                                vec![],
                                Some("null".to_string()),
                            )
                            .unwrap()
                            .unwrap());
                        }

                        let rate_limited = git_hub_util
                            .is_rate_limited(e.get_headers().map(|h| h.as_slice()).unwrap_or(&[]));

                        if !self.inner.io.has_authentication(&self.inner.origin_url) {
                            if !self.inner.io.is_interactive() {
                                self.inner.io.write_error(
                                    PhpMixed::String(format!(
                                        "<error>GitHub API limit exhausted. Failed to get metadata for the {} repository, try running in interactive mode so that you can enter your GitHub credentials to increase the API limit</error>",
                                        self.inner.url
                                    )),
                                    true,
                                    IOInterface::NORMAL,
                                );
                                return Err(e);
                            }

                            git_hub_util.authorize_oauth_interactively(
                                &self.inner.origin_url,
                                Some(&format!(
                                    "API limit exhausted. Enter your GitHub credentials to get a larger API limit (<info>{}</info>)",
                                    self.inner.url
                                )),
                            );

                            return self.inner.get_contents(url);
                        }

                        if rate_limited {
                            let rate_limit = git_hub_util.get_rate_limit(
                                e.get_headers().map(|h| h.as_slice()).unwrap_or(&[]),
                            );
                            self.inner.io.write_error(
                                PhpMixed::String(sprintf(
                                    "<error>GitHub API limit (%d calls/hr) is exhausted. You are already authorized so you have to wait until %s before doing more requests</error>",
                                    &[
                                        rate_limit.get("limit").cloned().unwrap_or(PhpMixed::Null),
                                        rate_limit.get("reset").cloned().unwrap_or(PhpMixed::Null),
                                    ],
                                )),
                                true,
                                IOInterface::NORMAL,
                            );
                        }

                        Err(e)
                    }
                    _ => Err(e),
                }
            }
        }
    }

    /// Fetch root identifier from GitHub
    ///
    /// @throws TransportException
    pub(crate) fn fetch_root_identifier(&mut self) -> Result<()> {
        if self.repo_data.is_some() {
            return Ok(());
        }

        let repo_data_url = format!(
            "{}/repos/{}/{}",
            self.get_api_url(),
            self.owner,
            self.repository
        );

        let repo_data_result = self.get_contents(&repo_data_url, true);
        match repo_data_result {
            Ok(response) => {
                let data = response.decode_json()?;
                self.repo_data = match data {
                    PhpMixed::Array(m) => Some(m.into_iter().map(|(k, v)| (k, *v)).collect()),
                    _ => None,
                };
            }
            Err(e) => {
                if e.code == 499 {
                    self.attempt_clone_fallback(Some(&e))?;
                } else {
                    return Err(e.into());
                }
            }
        }
        if self.repo_data.is_none() && self.git_driver.is_some() {
            return Ok(());
        }

        let repo_data = self.repo_data.clone().unwrap_or_default();
        self.owner = repo_data
            .get("owner")
            .and_then(|v| v.as_array())
            .and_then(|m| m.get("login"))
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        self.repository = repo_data
            .get("name")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();

        self.is_private = !empty(&repo_data.get("private").cloned().unwrap_or(PhpMixed::Null));
        if let Some(default_branch) = repo_data.get("default_branch").and_then(|v| v.as_string()) {
            self.root_identifier = default_branch.to_string();
        } else if let Some(master_branch) =
            repo_data.get("master_branch").and_then(|v| v.as_string())
        {
            self.root_identifier = master_branch.to_string();
        } else {
            self.root_identifier = "master".to_string();
        }
        self.has_issues = !empty(
            &repo_data
                .get("has_issues")
                .cloned()
                .unwrap_or(PhpMixed::Null),
        );
        self.is_archived = !empty(&repo_data.get("archived").cloned().unwrap_or(PhpMixed::Null));

        Ok(())
    }

    /// @phpstan-impure
    ///
    /// @return true
    /// @throws \RuntimeException
    pub(crate) fn attempt_clone_fallback(
        &mut self,
        e: Option<&TransportException>,
    ) -> Result<bool> {
        if !self.allow_git_fallback {
            return Err(RuntimeException {
                message: format!(
                    "Fallback to git driver disabled{}",
                    e.map(|e| format!(": {}", e.message)).unwrap_or_default()
                ),
                code: 0,
            }
            .into());
        }

        self.is_private = true;

        let ssh_url = self.generate_ssh_url();
        // If this repository may be private (hard to say for sure,
        // GitHub returns 404 for private repositories) and we
        // cannot ask for authentication credentials (because we
        // are not interactive) then we fallback to GitDriver.
        match self.setup_git_driver(&ssh_url) {
            Ok(()) => Ok(true),
            Err(setup_err) => {
                self.git_driver = None;

                self.inner.io.write_error(
                    PhpMixed::String(format!(
                        "<error>Failed to clone the {} repository, try running in interactive mode so that you can enter your GitHub credentials</error>",
                        self.generate_ssh_url()
                    )),
                    true,
                    IOInterface::NORMAL,
                );
                Err(setup_err)
            }
        }
    }

    pub(crate) fn setup_git_driver(&mut self, url: &str) -> Result<()> {
        if !self.allow_git_fallback {
            return Err(RuntimeException {
                message: "Fallback to git driver disabled".to_string(),
                code: 0,
            }
            .into());
        }
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

    pub(crate) fn get_next_page(&self, response: &Response) -> Option<String> {
        let header = response.get_header("link")?;
        if header.is_empty() {
            return None;
        }

        let links = explode(",", &header);
        for link in &links {
            if let Some(m) = Preg::is_match_strict_groups(r#"{<(.+?)>; *rel="next"}"#, link) {
                return Some(m.get(1).cloned().unwrap_or_default());
            }
        }

        None
    }
}
