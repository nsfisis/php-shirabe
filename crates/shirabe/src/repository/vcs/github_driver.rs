//! ref: composer/src/Composer/Repository/Vcs/GitHubDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::json::{JsonEncodeOptions, JsonFile};
use crate::repository::vcs::{GitDriver, VcsDriverBase};
use crate::util::GitHub;
use crate::util::http::Response;
use bstr::{B, BStr, BString, ByteSlice};
use chrono::{DateTime, FixedOffset};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, InvalidArgumentException, PhpMixed, RuntimeException, array_diff, array_map,
    array_search_mixed, base64_decode, basename, bconcat, date_create, empty, explode,
    extension_loaded, in_array_loose, legacy_lossy, parse_url, php_regex, php_to_string,
    preg_is_match, preg_match, preg_replace, preg_split, strpos, strtolower, substr, trim, trim2,
    urlencode,
};

#[derive(Debug)]
pub struct GitHubDriver {
    inner: VcsDriverBase,
    owner: BString,
    repository: BString,
    tags: Option<IndexMap<BString, BString>>,
    branches: Option<IndexMap<BString, BString>>,
    root_identifier: BString,
    repo_data: Option<IndexMap<BString, PhpMixed>>,
    has_issues: bool,
    is_private: bool,
    is_archived: bool,
    funding_info: Option<PhpMixed>,
    allow_git_fallback: bool,
    /// Git Driver
    git_driver: Option<GitDriver>,
}

impl GitHubDriver {
    pub fn new(
        repo_config: IndexMap<BString, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
        process: std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    ) -> Self {
        Self {
            inner: VcsDriverBase::new(repo_config, io, config, http_downloader, process),
            owner: BString::default(),
            repository: BString::default(),
            tags: None,
            branches: None,
            root_identifier: BString::default(),
            repo_data: None,
            has_issues: false,
            is_private: false,
            is_archived: false,
            funding_info: None,
            allow_git_fallback: true,
            git_driver: None,
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<()> {
        let Some(match_) = preg_match(
            php_regex!(
                r"#^(?:(?:https?|git)://([^/]+)/|git@([^:]+):/?)([^/]+)/([^/]+?)(?:\.git|/)?$#"
            ),
            &self.inner.url,
        ) else {
            return Err(InvalidArgumentException::new(format!(
                "The GitHub repository URL {} is invalid.",
                self.inner.url.clone().display_lossy(),
            ))
            .into());
        };

        self.owner = BString::from(match_.get(3).unwrap_or_default());
        self.repository = BString::from(match_.get(4).unwrap_or_default());
        self.inner.origin_url = BString::from(
            &strtolower(
                match_
                    .get(1)
                    .filter(|s| !s.is_empty())
                    .map(BString::from)
                    .unwrap_or_else(|| BString::from(match_.get(2).unwrap_or_default())),
            )[..],
        );
        if self.inner.origin_url == "www.github.com" {
            self.inner.origin_url = bstr::BString::from("github.com".to_string());
        }
        self.inner.cache = Some(Cache::new(
            self.inner.io.clone(),
            format!(
                "{}/{}/{}/{}",
                legacy_lossy(
                    &self
                        .inner
                        .config
                        .borrow_mut()
                        .get(&legacy_lossy(b"cache-repo-dir".as_slice()))
                        .as_string()
                        .unwrap_or_default()
                ),
                legacy_lossy(&self.inner.origin_url),
                legacy_lossy(&self.owner),
                legacy_lossy(&self.repository)
            ),
            None,
            None,
            false,
        ));
        if let Some(c) = self.inner.cache.as_mut() {
            c.set_read_only(
                self.inner
                    .config
                    .borrow_mut()
                    .get(&legacy_lossy(b"cache-read-only".as_slice()))
                    .as_bool()
                    .unwrap_or(false),
            )
        }

        if self
            .inner
            .repo_config
            .get(B("allow-git-fallback"))
            .and_then(|v| v.as_bool())
            == Some(false)
        {
            self.allow_git_fallback = false;
        }

        if self
            .inner
            .config
            .borrow_mut()
            .get(&legacy_lossy(b"use-github-api".as_slice()))
            .as_bool()
            == Some(false)
            || self
                .inner
                .repo_config
                .get(B("no-api"))
                .is_some_and(|v| v.to_bool())
        {
            self.setup_git_driver(self.inner.url.clone().as_bstr())?;

            return Ok(());
        }

        self.fetch_root_identifier()
    }

    pub fn get_repository_url(&self) -> BString {
        bconcat!(
            "https://",
            self.inner.origin_url,
            "/",
            self.owner,
            "/",
            self.repository
        )
    }

    pub fn get_root_identifier(&mut self) -> anyhow::Result<BString> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_root_identifier();
        }

        Ok(self.root_identifier.clone())
    }

    pub fn get_url(&self) -> BString {
        if let Some(ref git_driver) = self.git_driver {
            return git_driver.get_url();
        }

        bconcat!(
            "https://",
            self.inner.origin_url,
            "/",
            self.owner,
            "/",
            self.repository,
            ".git"
        )
    }

    fn get_api_url(&self) -> BString {
        let api_url = if self.inner.origin_url == "github.com" {
            BString::from("api.github.com")
        } else {
            bconcat!(self.inner.origin_url, "/api/v3")
        };

        bconcat!("https://", api_url)
    }

    pub fn get_source(&self, identifier: &BStr) -> IndexMap<BString, PhpMixed> {
        if let Some(ref git_driver) = self.git_driver {
            return git_driver
                .get_source(identifier)
                .into_iter()
                .map(|(k, v)| (k, PhpMixed::string(v)))
                .collect();
        }
        let url = if self.is_private {
            // Private GitHub repositories should be accessed using the
            // SSH version of the URL.
            self.generate_ssh_url()
        } else {
            self.get_url()
        };

        let mut result = IndexMap::new();
        result.insert("type".into(), PhpMixed::string("git".to_string()));
        result.insert("url".into(), PhpMixed::string(url));
        result.insert(
            "reference".into(),
            PhpMixed::string(BString::from(identifier)),
        );
        result
    }

    pub fn get_dist(&self, identifier: &BStr) -> Option<IndexMap<BString, PhpMixed>> {
        let url = bconcat!(
            self.get_api_url(),
            "/repos/",
            self.owner,
            "/",
            self.repository,
            "/zipball/",
            identifier
        );

        let mut result = IndexMap::new();
        result.insert("type".into(), PhpMixed::string("zip".to_string()));
        result.insert("url".into(), PhpMixed::string(url));
        result.insert(
            "reference".into(),
            PhpMixed::string(BString::from(identifier)),
        );
        result.insert("shasum".into(), PhpMixed::string(BString::default()));
        Some(result)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_composer_information(identifier);
        }

        if !self.inner.info_cache.contains_key(identifier) {
            let composer = if self.inner.should_cache(identifier)
                && self
                    .inner
                    .cache
                    .as_mut()
                    .and_then(|c| c.read(&*legacy_lossy(identifier)))
                    .is_some()
            {
                let res = self
                    .inner
                    .cache
                    .as_mut()
                    .and_then(|c| c.read(&*legacy_lossy(identifier)))
                    .unwrap_or_default();
                let parsed = JsonFile::parse_json(Some(&res), None)?;
                parsed.as_array().cloned()
            } else {
                let file_content =
                    self.get_file_content(bstr::BStr::new("composer.json"), identifier)?;
                let composer = VcsDriverBase::finish_base_composer_information(
                    identifier,
                    file_content,
                    || self.get_change_date(identifier),
                )?;

                if self.inner.should_cache(identifier)
                    && let Some(ref composer_map) = composer
                {
                    let php_value: PhpMixed = PhpMixed::Array(
                        composer_map
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                    );
                    self.inner.cache.as_mut().map(|c| {
                        c.write(
                            &*legacy_lossy(identifier),
                            &JsonFile::encode_with_options(
                                &php_value,
                                JsonEncodeOptions {
                                    pretty_print: false,
                                    ..Default::default()
                                },
                            )?,
                        )
                    });
                }

                composer
            };

            let mut composer = composer;
            if let Some(ref mut composer) = composer {
                // specials for github
                if composer.contains_key(B("support"))
                    && !matches!(composer.get(B("support")), Some(PhpMixed::Array(_)))
                {
                    composer.insert("support".into(), PhpMixed::Array(IndexMap::new()));
                }
                let support_source_missing = !composer
                    .get(B("support"))
                    .and_then(|v| v.as_array())
                    .map(|m| m.contains_key(B("source")))
                    .unwrap_or(false);
                if support_source_missing {
                    // PHP's `?:` chain only evaluates getBranches() when the tags search is falsy.
                    let tags_map = self.get_tags()?;
                    let mut label = array_search_mixed(
                        &PhpMixed::string(BString::from(identifier)),
                        &PhpMixed::Array(
                            tags_map
                                .into_iter()
                                .map(|(k, v)| (k, PhpMixed::string(v)))
                                .collect(),
                        ),
                        false,
                    )
                    .filter(|v| !matches!(v, PhpMixed::Bool(false) | PhpMixed::Null));
                    if label.is_none() {
                        let branches_map = self.get_branches()?;
                        label = array_search_mixed(
                            &PhpMixed::string(BString::from(identifier)),
                            &PhpMixed::Array(
                                branches_map
                                    .into_iter()
                                    .map(|(k, v)| (k, PhpMixed::string(v)))
                                    .collect(),
                            ),
                            false,
                        )
                        .filter(|v| !matches!(v, PhpMixed::Bool(false) | PhpMixed::Null));
                    }
                    let label =
                        label.unwrap_or_else(|| PhpMixed::string(BString::from(identifier)));
                    let label_str = BString::from(label.as_string().unwrap_or(identifier));
                    if let Some(support) = composer.get_mut(B("support")).and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m),
                        _ => None,
                    }) {
                        support.insert(
                            "source".into(),
                            PhpMixed::string(bconcat!(
                                "https://",
                                self.inner.origin_url.clone(),
                                "/",
                                self.owner.clone(),
                                "/",
                                self.repository.clone(),
                                "/tree/",
                                label_str
                            )),
                        );
                    }
                }
                let issues_missing = !composer
                    .get(B("support"))
                    .and_then(|v| v.as_array())
                    .map(|m| m.contains_key(B("issues")))
                    .unwrap_or(false);
                if issues_missing
                    && self.has_issues
                    && let Some(support) = composer.get_mut(B("support")).and_then(|v| match v {
                        PhpMixed::Array(m) => Some(m),
                        _ => None,
                    })
                {
                    support.insert(
                        "issues".into(),
                        PhpMixed::string(bconcat!(
                            "https://",
                            self.inner.origin_url.clone(),
                            "/",
                            self.owner.clone(),
                            "/",
                            self.repository.clone(),
                            "/issues"
                        )),
                    );
                }
                if !composer.contains_key(B("abandoned")) && self.is_archived {
                    composer.insert("abandoned".into(), PhpMixed::Bool(true));
                }
                if !composer.contains_key(B("funding")) {
                    let funding = self.get_funding_info();
                    if !matches!(funding, PhpMixed::Bool(false)) {
                        composer.insert("funding".into(), funding);
                    }
                }
            }

            self.inner
                .info_cache
                .insert(bstr::BString::from(identifier), composer);
        }

        Ok(self
            .inner
            .info_cache
            .get(identifier)
            .cloned()
            .unwrap_or(None))
    }

    fn get_funding_info(&mut self) -> PhpMixed {
        if let Some(ref info) = self.funding_info {
            return info.clone();
        }

        if self.inner.origin_url != "github.com" {
            self.funding_info = Some(PhpMixed::Bool(false));
            return PhpMixed::Bool(false);
        }

        let mut funding: Option<bstr::BString> = None;
        for file_url in &[
            bconcat!(
                self.get_api_url(),
                "/repos/",
                self.owner,
                "/",
                self.repository,
                "/contents/.github/FUNDING.yml"
            ),
            bconcat!(
                self.get_api_url(),
                "/repos/",
                self.owner,
                "/.github/contents/FUNDING.yml"
            ),
        ] {
            let mut options: IndexMap<BString, PhpMixed> = IndexMap::new();
            options.insert("retry-auth-failure".into(), PhpMixed::Bool(false));
            let response = self
                .inner
                .http_downloader
                .borrow_mut()
                .get(file_url, options);
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
                .get(B("content"))
                .and_then(|v| v.as_string())
                .map(|s| s.is_empty())
                .unwrap_or(true);
            let encoding_not_base64 = response_map.get(B("encoding")).and_then(|v| v.as_string())
                != Some("base64".into());
            if content_empty || encoding_not_base64 {
                continue;
            }
            let decoded = base64_decode(
                response_map
                    .get(B("content"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            );
            match decoded {
                Some(b) if !b.is_empty() => {
                    funding = Some(b.into());
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

        let mut result: Vec<IndexMap<BString, PhpMixed>> = vec![];
        let mut key: Option<BString> = None;
        for line in preg_split(php_regex!(r"{\r?\n}"), &funding) {
            let line = BString::from(&trim(&line)[..]);
            if let Some(m) = preg_match(php_regex!(r"{^(\w+)\s*:\s*(.+)$}"), &line) {
                let g1 = BString::from(m.get(1).unwrap_or_default());
                let g2 = BString::from(m.get(2).unwrap_or_default());
                if g2 == "[" {
                    key = Some(g1);
                    continue;
                }
                if let Some(m2) = preg_match(php_regex!(r"{^\[(.*?)\](?:\s*#.*)?$}"), &g2) {
                    let inner = BString::from(m2.get(1).unwrap_or_default());
                    for item in array_map(
                        |s: &BString| trim(s),
                        &preg_split(php_regex!(r#"{[\'\"]?\s*,\s*[\'\"]?}"#), &inner),
                    ) {
                        let mut entry = IndexMap::new();
                        entry.insert("type".into(), PhpMixed::string(g1.clone()));
                        entry.insert("url".into(), PhpMixed::string(trim2(&item, "\"' ")));
                        result.push(entry);
                    }
                } else if let Some(m2) = preg_match(php_regex!(r"{^([^#].*?)(?:\s+#.*)?$}"), &g2) {
                    let mut entry = IndexMap::new();
                    entry.insert("type".into(), PhpMixed::string(g1.clone()));
                    entry.insert(
                        "url".into(),
                        PhpMixed::string(trim2(m2.get(1).unwrap_or_default(), "\"' ")),
                    );
                    result.push(entry);
                }
                key = None;
            } else if let Some(m) = preg_match(php_regex!(r"{^(\w+)\s*:\s*#\s*$}"), &line) {
                key = Some(BString::from(m.get(1).unwrap_or_default()));
            } else if key.is_some()
                && let Some(m) = preg_match(php_regex!(r"{^-\s*(.+)(?:\s+#.*)?$}"), &line)
                    .or_else(|| preg_match(php_regex!(r"{^(.+),(?:\s*#.*)?$}"), &line))
            {
                let mut entry = IndexMap::new();
                entry.insert(
                    "type".into(),
                    PhpMixed::string(key.clone().unwrap_or_default()),
                );
                entry.insert(
                    "url".into(),
                    PhpMixed::string(trim2(m.get(1).unwrap_or_default(), "\"' ")),
                );
                result.push(entry);
            } else if key.is_some() && line == "]" {
                key = None;
            }
        }

        let mut keys_to_remove: Vec<usize> = vec![];
        let mut result_for_iter: Vec<IndexMap<BString, PhpMixed>> = result.clone();
        for (key_idx, item) in result_for_iter.iter_mut().enumerate() {
            let item_type = BString::from(
                item.get(B("type"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            );
            let item_url = legacy_lossy(&BString::from(
                item.get(B("url"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            ))
            .into_owned();
            match item_type.as_slice() {
                b"community_bridge" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(bconcat!(
                            "https://funding.communitybridge.org/projects/",
                            basename(&item_url)
                        )),
                    );
                }
                b"github" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(bconcat!("https://github.com/", basename(&item_url))),
                    );
                }
                b"issuehunt" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(format!("https://issuehunt.io/r/{}", item_url)),
                    );
                }
                b"ko_fi" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(bconcat!("https://ko-fi.com/", basename(&item_url))),
                    );
                }
                b"liberapay" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(bconcat!("https://liberapay.com/", basename(&item_url))),
                    );
                }
                b"open_collective" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(bconcat!(
                            "https://opencollective.com/",
                            basename(&item_url)
                        )),
                    );
                }
                b"patreon" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(bconcat!("https://www.patreon.com/", basename(&item_url))),
                    );
                }
                b"tidelift" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(format!(
                            "https://tidelift.com/funding/github/{}",
                            item_url
                        )),
                    );
                }
                b"polar" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(bconcat!("https://polar.sh/", basename(&item_url))),
                    );
                }
                b"buy_me_a_coffee" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(bconcat!(
                            "https://www.buymeacoffee.com/",
                            basename(&item_url)
                        )),
                    );
                }
                b"thanks_dev" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(format!("https://thanks.dev/{}", item_url)),
                    );
                }
                b"otechie" => {
                    result[key_idx].insert(
                        "url".into(),
                        PhpMixed::string(bconcat!("https://otechie.com/", basename(&item_url))),
                    );
                }
                b"custom" => {
                    let Some(bits) = parse_url(&item_url) else {
                        keys_to_remove.push(key_idx);
                        continue;
                    };

                    if bits.scheme.is_none() && bits.host.is_none() {
                        if preg_is_match(php_regex!(r"{^[a-z0-9-]++\.[a-z]{2,3}$}"), &item_url) {
                            result[key_idx].insert(
                                "url".into(),
                                PhpMixed::string(format!("https://{}", item_url)),
                            );
                            continue;
                        }

                        self.inner.io.write_error3(
                            &format!(
                                "<warning>Funding URL {} not in a supported format.</warning>",
                                item_url
                            ),
                            true,
                            io_interface::NORMAL,
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
                .map(|m| PhpMixed::Array(m.into_iter().collect()))
                .collect(),
        );
        self.funding_info = Some(result_mixed.clone());
        result_mixed
    }

    pub fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_file_content(file, identifier);
        }

        let resource_url = bconcat!(
            self.get_api_url(),
            "/repos/",
            self.owner,
            "/",
            self.repository,
            "/contents/",
            file,
            "?ref=",
            urlencode(identifier)
        );
        let mut resource = self
            .get_contents(bstr::BStr::new(&resource_url), false)??
            .decode_json()?;

        // The GitHub contents API only returns files up to 1MB as base64 encoded files
        // larger files either need be fetched with a raw accept header or by using the git blob endpoint
        let resource_map = match resource {
            PhpMixed::Array(ref m) => m.clone(),
            _ => IndexMap::new(),
        };
        let needs_git_url = (resource_map
            .get(B("content"))
            .and_then(|v| v.as_string())
            .is_none()
            || resource_map
                .get(B("content"))
                .and_then(|v| v.as_string())
                .map(|s| s.is_empty())
                .unwrap_or(false))
            && resource_map.get(B("encoding")).and_then(|v| v.as_string()) == Some("none".into())
            && resource_map.contains_key(B("git_url"));
        if needs_git_url {
            let git_url = BString::from(
                resource_map
                    .get(B("git_url"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            );
            resource = self
                .get_contents(git_url.as_bstr(), false)??
                .decode_json()?;
        }

        let resource_map = match resource {
            PhpMixed::Array(m) => m,
            _ => IndexMap::new(),
        };
        let has_content = resource_map.contains_key(B("content"));
        let encoding_base64 =
            resource_map.get(B("encoding")).and_then(|v| v.as_string()) == Some("base64".into());
        let content = if has_content && encoding_base64 {
            base64_decode(
                resource_map
                    .get(B("content"))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            )
        } else {
            None
        };
        let content = match content {
            Some(c) => String::from_utf8_lossy(&c).to_string(),
            None => {
                return Err(RuntimeException::new(format!(
                    "Could not retrieve {} for {}",
                    file.display_lossy(),
                    identifier.display_lossy()
                ))
                .into());
            }
        };

        Ok(Some(bstr::BString::from(content)))
    }

    pub fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_change_date(identifier);
        }

        let resource = bconcat!(
            self.get_api_url(),
            "/repos/",
            self.owner,
            "/",
            self.repository,
            "/commits/",
            urlencode(identifier)
        );
        let commit = self
            .get_contents(bstr::BStr::new(&resource), false)??
            .decode_json()?;

        let date_str = match commit {
            PhpMixed::Array(m) => BString::from(
                m.get(B("commit"))
                    .and_then(|v| v.as_array())
                    .and_then(|c| c.get(B("committer")))
                    .and_then(|v| v.as_array())
                    .and_then(|c| c.get(B("date")))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            ),
            _ => BString::default(),
        };

        let date: DateTime<FixedOffset> = date_create(&date_str)?;
        Ok(Some(date))
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_tags();
        }
        if self.tags.is_none() {
            let mut tags: IndexMap<BString, BString> = IndexMap::new();
            let mut resource: Option<BString> = Some(bconcat!(
                self.get_api_url(),
                "/repos/",
                self.owner,
                "/",
                self.repository,
                "/tags?per_page=100"
            ));

            loop {
                let response = self.get_contents(
                    resource.as_ref().map(|r| r.as_bstr()).unwrap_or_default(),
                    false,
                )??;
                let tags_data = response.decode_json()?;
                if let PhpMixed::List(ref list) = tags_data {
                    for tag in list {
                        if let PhpMixed::Array(ref tag_map) = *tag {
                            let name = BString::from(
                                tag_map
                                    .get(B("name"))
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default(),
                            );
                            let sha = BString::from(
                                tag_map
                                    .get(B("commit"))
                                    .and_then(|v| v.as_array())
                                    .and_then(|m| m.get(B("sha")))
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default(),
                            );
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

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        if let Some(ref mut git_driver) = self.git_driver {
            return git_driver.get_branches();
        }
        if self.branches.is_none() {
            let mut branches: IndexMap<BString, BString> = IndexMap::new();
            let mut resource: Option<BString> = Some(bconcat!(
                self.get_api_url(),
                "/repos/",
                self.owner,
                "/",
                self.repository,
                "/git/refs/heads?per_page=100"
            ));

            loop {
                let response = self.get_contents(
                    resource.as_ref().map(|r| r.as_bstr()).unwrap_or_default(),
                    false,
                )??;
                let branch_data = response.decode_json()?;
                if let PhpMixed::List(ref list) = branch_data {
                    for branch in list {
                        if let PhpMixed::Array(ref branch_map) = *branch {
                            let ref_str = branch_map
                                .get(B("ref"))
                                .and_then(|v| v.as_string())
                                .unwrap_or_default();
                            let name = BString::from(&substr(ref_str, 11, None)[..]);
                            if name != "gh-pages" {
                                let sha = BString::from(
                                    branch_map
                                        .get(B("object"))
                                        .and_then(|v| v.as_array())
                                        .and_then(|m| m.get(B("sha")))
                                        .and_then(|v| v.as_string())
                                        .unwrap_or_default(),
                                );
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

    pub fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &BStr,
        _deep: bool,
    ) -> anyhow::Result<bool> {
        let Some(matches) = preg_match(
            php_regex!(
                r"#^((?:https?|git)://([^/]+)/|git@([^:]+):/?)([^/]+)/([^/]+?)(?:\.git|/)?$#"
            ),
            url,
        ) else {
            return Ok(false);
        };

        let origin_url = matches
            .get(2)
            .filter(|s| !s.is_empty())
            .map(BString::from)
            .unwrap_or_else(|| BString::from(matches.get(3).unwrap_or_default()));
        if !in_array_loose(
            strtolower(preg_replace(php_regex!(r"{^www\.}i"), "", &origin_url)),
            config
                .borrow()
                .get(&legacy_lossy(b"github-domains".as_slice()))
                .values(),
        ) {
            return Ok(false);
        }

        if !extension_loaded("openssl") {
            io.write_error3(
                &format!(
                    "Skipping GitHub driver for {} because the OpenSSL PHP extension is missing.",
                    url.display_lossy()
                ),
                true,
                io_interface::VERBOSE,
            );

            return Ok(false);
        }

        Ok(true)
    }

    /// Gives back the loaded <github-api>/repos/<owner>/<repo> result
    pub fn get_repo_data(&mut self) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        self.fetch_root_identifier()?;

        Ok(self.repo_data.clone())
    }

    /// Generate an SSH URL
    fn generate_ssh_url(&self) -> BString {
        if strpos(&self.inner.origin_url, ":").is_some() {
            return bconcat!(
                "ssh://git@",
                self.inner.origin_url,
                "/",
                self.owner,
                "/",
                self.repository,
                ".git"
            );
        }

        bconcat!(
            "git@",
            self.inner.origin_url,
            ":",
            self.owner,
            "/",
            self.repository,
            ".git"
        )
    }

    fn get_contents(
        &mut self,
        url: &BStr,
        fetching_repo_data: bool,
    ) -> anyhow::Result<Result<Response, Box<TransportException>>> {
        let response_result = self.inner.get_contents(url)?;
        match response_result {
            Ok(r) => Ok(Ok(r)),
            Err(e) => {
                let mut git_hub_util = GitHub::new(
                    self.inner.io.clone(),
                    self.inner.config.clone(),
                    Some(self.inner.process.clone()),
                    Some(self.inner.http_downloader.clone()),
                )?;

                match e.get_code() {
                    401 | 404 => {
                        // try to authorize only if we are fetching the main /repos/foo/bar data, otherwise it must be a real 404
                        if !fetching_repo_data {
                            return Ok(Err(e));
                        }

                        if git_hub_util.authorize_oauth(self.inner.origin_url.as_bstr())? {
                            return self.inner.get_contents(url);
                        }

                        if !self.inner.io.is_interactive() {
                            self.attempt_clone_fallback(Some(std::sync::Arc::new((*e).into())))?;

                            return Ok(Ok(Response::new(
                                "dummy".into(),
                                Some(200),
                                vec![],
                                Some(b"null".to_vec()),
                            )));
                        }

                        let mut scopes_issued: Vec<BString> = vec![];
                        let mut scopes_needed: Vec<BString> = vec![];
                        let headers = e.get_headers().cloned().unwrap_or_default();
                        if !headers.is_empty() {
                            if let Some(scopes) =
                                Response::find_header_value(&headers, "X-OAuth-Scopes")
                            {
                                scopes_issued = explode(" ", &scopes).to_vec();
                            }
                            if let Some(scopes) =
                                Response::find_header_value(&headers, "X-Accepted-OAuth-Scopes")
                            {
                                scopes_needed = explode(" ", &scopes).to_vec();
                            }
                        }
                        let scopes_failed = array_diff(&scopes_needed, &scopes_issued);
                        // non-authenticated requests get no scopesNeeded, so ask for credentials
                        // authenticated requests which failed some scopes should ask for new credentials too
                        if headers.is_empty()
                            || scopes_needed.is_empty()
                            || !scopes_failed.is_empty()
                        {
                            git_hub_util.authorize_oauth_interactively(
                                self.inner.origin_url.as_bstr(),
                                Some(&*legacy_lossy(&bconcat!("Your GitHub credentials are required to fetch private repository metadata (<info>", self.inner.url, "</info>)"))),
                            )?;
                        }

                        self.inner.get_contents(url)
                    }
                    403 => {
                        if !self
                            .inner
                            .io
                            .has_authentication(self.inner.origin_url.as_bstr())
                            && git_hub_util.authorize_oauth(self.inner.origin_url.as_bstr())?
                        {
                            return self.inner.get_contents(url);
                        }

                        if !self.inner.io.is_interactive() && fetching_repo_data {
                            self.attempt_clone_fallback(Some(std::sync::Arc::new((*e).into())))?;

                            return Ok(Ok(Response::new(
                                "dummy".into(),
                                Some(200),
                                vec![],
                                Some(b"null".to_vec()),
                            )));
                        }

                        let headers: Vec<BString> = e.get_headers().cloned().unwrap_or_default();
                        let rate_limited = git_hub_util.is_rate_limited(&headers);

                        if !self
                            .inner
                            .io
                            .has_authentication(self.inner.origin_url.as_bstr())
                        {
                            if !self.inner.io.is_interactive() {
                                self.inner.io.write_error3(
                                    &format!(
                                        "<error>GitHub API limit exhausted. Failed to get metadata for the {} repository, try running in interactive mode so that you can enter your GitHub credentials to increase the API limit</error>",
                                        self.inner.url.display_lossy()
                                    ),
                                    true,
                                    io_interface::NORMAL,
                                );
                                return Ok(Err(e));
                            }

                            git_hub_util.authorize_oauth_interactively(
                                self.inner.origin_url.as_bstr(),
                                Some(&*legacy_lossy(&bconcat!("API limit exhausted. Enter your GitHub credentials to get a larger API limit (<info>", self.inner.url, "</info>)"))),
                            )?;

                            return self.inner.get_contents(url);
                        }

                        if rate_limited {
                            let rate_limit = git_hub_util.get_rate_limit(&headers);
                            self.inner.io.write_error3(
                                &format!(
                                    "<error>GitHub API limit ({} calls/hr) is exhausted. You are already authorized so you have to wait until {} before doing more requests</error>",
                                    rate_limit.get(B("limit")).map(php_to_string).unwrap_or_default().display_lossy(),
                                    rate_limit.get(B("reset")).map(php_to_string).unwrap_or_default().display_lossy(),
                                ),
                                true,
                                io_interface::NORMAL,
                            );
                        }

                        Ok(Err(e))
                    }
                    _ => Ok(Err(e)),
                }
            }
        }
    }

    /// Fetch root identifier from GitHub
    fn fetch_root_identifier(&mut self) -> anyhow::Result<()> {
        if self.repo_data.is_some() {
            return Ok(());
        }

        let repo_data_url = bconcat!(
            self.get_api_url(),
            "/repos/",
            self.owner,
            "/",
            self.repository
        );

        let repo_data_result = self.get_contents(bstr::BStr::new(&repo_data_url), true)?;
        match repo_data_result {
            Ok(response) => {
                let data = response.decode_json()?;
                self.repo_data = match data {
                    PhpMixed::Array(m) => Some(m),
                    _ => None,
                };
            }
            Err(e) => {
                if e.get_code() == 499 {
                    self.attempt_clone_fallback(Some(std::sync::Arc::new((*e).into())))?;
                } else {
                    return Err((*e).into());
                }
            }
        }
        if self.repo_data.is_none() && self.git_driver.is_some() {
            return Ok(());
        }

        let repo_data = self.repo_data.clone().unwrap_or_default();
        self.owner = BString::from(
            repo_data
                .get(B("owner"))
                .and_then(|v| v.as_array())
                .and_then(|m| m.get(B("login")))
                .and_then(|v| v.as_string())
                .unwrap_or_default(),
        );
        self.repository = BString::from(
            repo_data
                .get(B("name"))
                .and_then(|v| v.as_string())
                .unwrap_or_default(),
        );

        self.is_private = !empty(
            &repo_data
                .get(B("private"))
                .cloned()
                .unwrap_or(PhpMixed::Null),
        );
        if let Some(default_branch) = repo_data
            .get(B("default_branch"))
            .and_then(|v| v.as_string())
        {
            self.root_identifier = BString::from(default_branch);
        } else if let Some(master_branch) = repo_data
            .get(B("master_branch"))
            .and_then(|v| v.as_string())
        {
            self.root_identifier = BString::from(master_branch);
        } else {
            self.root_identifier = bstr::BString::from("master".to_string());
        }
        self.has_issues = !empty(
            &repo_data
                .get(B("has_issues"))
                .cloned()
                .unwrap_or(PhpMixed::Null),
        );
        self.is_archived = !empty(
            &repo_data
                .get(B("archived"))
                .cloned()
                .unwrap_or(PhpMixed::Null),
        );

        Ok(())
    }

    fn attempt_clone_fallback(
        &mut self,
        e: Option<std::sync::Arc<anyhow::Error>>,
    ) -> anyhow::Result<bool> {
        if !self.allow_git_fallback {
            return Err(RuntimeException::with_code_and_previous(
                "Fallback to git driver disabled".to_string(),
                0,
                e,
            )
            .into());
        }

        self.is_private = true;

        let ssh_url = self.generate_ssh_url();
        // If this repository may be private (hard to say for sure,
        // GitHub returns 404 for private repositories) and we
        // cannot ask for authentication credentials (because we
        // are not interactive) then we fallback to GitDriver.
        match self.setup_git_driver(ssh_url.as_bstr()) {
            Ok(()) => Ok(true),
            Err(setup_err) => {
                self.git_driver = None;

                self.inner.io.write_error3(
                    &format!(
                        "<error>Failed to clone the {} repository, try running in interactive mode so that you can enter your GitHub credentials</error>",
                        self.generate_ssh_url().display_lossy()
                    ),
                    true,
                    io_interface::NORMAL,
                );
                Err(setup_err)
            }
        }
    }

    fn setup_git_driver(&mut self, url: &BStr) -> anyhow::Result<()> {
        if !self.allow_git_fallback {
            return Err(
                RuntimeException::new("Fallback to git driver disabled".to_string()).into(),
            );
        }
        let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
        repo_config.insert("url".into(), PhpMixed::string(BString::from(url)));
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

    fn get_next_page(&self, response: &Response) -> Option<BString> {
        let header = response.get_header("link")?;
        if header.is_empty() {
            return None;
        }

        let links = explode(",", &header);
        for link in links.iter() {
            if let Some(m) = preg_match(php_regex!(r#"{<(.+?)>; *rel="next"}"#), link) {
                return Some(BString::from(m.get(1).unwrap_or_default()));
            }
        }

        None
    }

    /// For testing only. Mirrors the `setAttribute($driver, 'tags', ...)` reflection
    /// helper used by GitHubDriverTest to seed private state.
    pub fn __set_tags(&mut self, tags: Option<IndexMap<BString, BString>>) {
        self.tags = tags;
    }

    /// For testing only. Mirrors the `setAttribute($driver, 'branches', ...)` reflection
    /// helper used by GitHubDriverTest to seed private state.
    pub fn __set_branches(&mut self, branches: Option<IndexMap<BString, BString>>) {
        self.branches = branches;
    }
}

impl crate::repository::vcs::VcsDriverInterface for GitHubDriver {
    fn initialize(&mut self) -> anyhow::Result<()> {
        GitHubDriver::initialize(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        GitHubDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        GitHubDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        GitHubDriver::get_change_date(self, identifier)
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<BString> {
        GitHubDriver::get_root_identifier(self)
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        GitHubDriver::get_branches(self)
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        GitHubDriver::get_tags(self)
    }

    fn get_dist(&self, identifier: &BStr) -> Option<IndexMap<BString, BString>> {
        GitHubDriver::get_dist(self, identifier).map(|m| {
            m.into_iter()
                .map(|(k, v)| (k.clone(), BString::from(v.as_string().unwrap_or_default())))
                .collect()
        })
    }

    fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        GitHubDriver::get_source(self, identifier)
            .into_iter()
            .map(|(k, v)| (k.clone(), BString::from(v.as_string().unwrap_or_default())))
            .collect()
    }

    fn get_url(&self) -> BString {
        GitHubDriver::get_url(self)
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
        GitHubDriver::supports(io, config, url, deep)
    }
}
