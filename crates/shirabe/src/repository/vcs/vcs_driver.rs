//! ref: composer/src/Composer/Repository/Vcs/VcsDriver.php

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE, PhpMixed, extension_loaded,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::repository::vcs::vcs_driver_interface::VcsDriverInterface;
use crate::util::filesystem::Filesystem;
use crate::util::http::response::Response;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct VcsDriverBase {
    pub url: String,
    pub origin_url: String,
    pub repo_config: IndexMap<String, PhpMixed>,
    pub io: Box<dyn IOInterface>,
    pub config: std::rc::Rc<std::cell::RefCell<Config>>,
    pub process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    pub http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    pub info_cache: IndexMap<String, Option<IndexMap<String, PhpMixed>>>,
    pub cache: Option<Cache>,
}

impl VcsDriverBase {
    pub fn new(
        repo_config: IndexMap<String, PhpMixed>,
        io: Box<dyn IOInterface>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    ) -> Self {
        let url = repo_config
            .get("url")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        let origin_url = url.clone();
        Self {
            url,
            origin_url,
            repo_config,
            io,
            config,
            process,
            http_downloader,
            info_cache: IndexMap::new(),
            cache: None,
        }
    }

    pub fn should_cache(&self, identifier: &str) -> bool {
        self.cache.is_some() && Preg::is_match("{^[a-f0-9]{40}$}iD", identifier).unwrap_or(false)
    }

    pub fn get_scheme(&self) -> &str {
        if extension_loaded("openssl") {
            return "https";
        }
        "http"
    }

    pub fn get_contents(&self, url: &str) -> anyhow::Result<Response, TransportException> {
        let options_mixed = self
            .repo_config
            .get("options")
            .cloned()
            .unwrap_or(PhpMixed::Array(IndexMap::new()));
        // TODO(phase-b): convert PhpMixed::Array options into IndexMap<String, PhpMixed> properly.
        let options: IndexMap<String, PhpMixed> = match options_mixed {
            PhpMixed::Array(a) => a.into_iter().map(|(k, v)| (k, *v)).collect(),
            _ => IndexMap::new(),
        };
        // TODO(phase-b): map anyhow::Error from HttpDownloader::get into TransportException.
        self.http_downloader
            .borrow_mut()
            .get(url, options)
            .map_err(|e| TransportException::new(e.to_string(), 0))
    }

    // Helper for concrete drivers: produces the same value as the trait default
    // `get_base_composer_information`, but receives a pre-fetched composer.json
    // body and a lazy change-date callback. Concrete drivers in the Rust port
    // wrap `VcsDriverBase` as `self.inner` instead of inheriting from it, so
    // they cannot dispatch back into a base method that calls `get_file_content`
    // / `get_change_date` hooks; the caller threads those calls in itself.
    pub fn finish_base_composer_information(
        identifier: &str,
        composer_file_content: Option<String>,
        change_date: impl FnOnce() -> anyhow::Result<Option<DateTime<Utc>>>,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        let content = match composer_file_content {
            None => return Ok(None),
            Some(c) if c.is_empty() => return Ok(None),
            Some(c) => c,
        };

        let parsed = JsonFile::parse_json(
            Some(&content),
            Some(&format!("{}:composer.json", identifier)),
        )?;

        let array = match parsed {
            PhpMixed::Array(a) if !a.is_empty() => a,
            _ => return Ok(None),
        };

        // PHP arrays own their nested values; the Rust representation wraps them
        // in Box<PhpMixed>. Unbox the outer level so callers can mutate keys.
        let mut composer: IndexMap<String, PhpMixed> =
            array.into_iter().map(|(k, v)| (k, *v)).collect();

        if !composer.contains_key("time")
            || composer
                .get("time")
                .map_or(true, |v| v.as_string().map_or(true, |s| s.is_empty()))
        {
            if let Some(d) = change_date()? {
                composer.insert("time".to_string(), PhpMixed::String(d.to_rfc3339()));
            }
        }

        Ok(Some(composer))
    }
}

// TODO(phase-b): the constructor is `final` in PHP; concrete implementations must replicate the
// initialization logic (local-path normalization etc.) from the original new() body.
pub trait VcsDriver: VcsDriverInterface {
    fn url(&self) -> &str;
    fn url_mut(&mut self) -> &mut String;
    fn origin_url(&self) -> &str;
    fn origin_url_mut(&mut self) -> &mut String;
    fn repo_config(&self) -> &IndexMap<String, PhpMixed>;
    fn repo_config_mut(&mut self) -> &mut IndexMap<String, PhpMixed>;
    fn io(&self) -> &dyn IOInterface;
    fn io_mut(&mut self) -> &mut dyn IOInterface;
    fn config(&self) -> &Config;
    fn config_mut(&mut self) -> &mut Config;
    fn process(&self) -> &ProcessExecutor;
    fn process_mut(&mut self) -> &mut ProcessExecutor;
    fn http_downloader(&self) -> &std::rc::Rc<std::cell::RefCell<HttpDownloader>>;
    fn info_cache(&self) -> &IndexMap<String, Option<IndexMap<String, PhpMixed>>>;
    fn info_cache_mut(&mut self) -> &mut IndexMap<String, Option<IndexMap<String, PhpMixed>>>;
    fn cache(&self) -> Option<&Cache>;
    fn cache_mut(&mut self) -> Option<&mut Cache>;

    fn should_cache(&self, identifier: &str) -> bool {
        self.cache().is_some() && Preg::is_match("{^[a-f0-9]{40}$}iD", identifier).unwrap_or(false)
    }

    fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        if !self.info_cache().contains_key(identifier) {
            if self.should_cache(identifier) {
                if let Some(res) = self.cache_mut().and_then(|c| c.read(identifier)) {
                    let parsed = JsonFile::parse_json(Some(&res), None)?;
                    // TODO(phase-b): unwrap PhpMixed::Array into IndexMap<String, PhpMixed>.
                    let parsed_map: Option<IndexMap<String, PhpMixed>> = match parsed {
                        PhpMixed::Array(a) => Some(a.into_iter().map(|(k, v)| (k, *v)).collect()),
                        _ => None,
                    };
                    self.info_cache_mut()
                        .insert(identifier.to_string(), parsed_map);
                    return Ok(self.info_cache().get(identifier).and_then(|v| v.clone()));
                }
            }

            let composer = self.get_base_composer_information(identifier)?;

            if self.should_cache(identifier) {
                if let Some(ref composer_map) = composer {
                    // TODO(phase-b): use a dedicated encode-with-options helper; reuse encode for now.
                    let composer_mixed = PhpMixed::Array(
                        composer_map
                            .iter()
                            .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                            .collect(),
                    );
                    let encoded = JsonFile::encode(
                        &composer_mixed,
                        JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES,
                    );
                    self.cache_mut().map(|c| c.write(identifier, &encoded));
                }
            }

            self.info_cache_mut()
                .insert(identifier.to_string(), composer);
        }

        Ok(self.info_cache().get(identifier).and_then(|v| v.clone()))
    }

    fn get_base_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        let composer_file_content = self.get_file_content("composer.json", identifier)?;

        let composer_file_content = match composer_file_content {
            None => return Ok(None),
            Some(c) if c.is_empty() => return Ok(None),
            Some(c) => c,
        };

        let composer = JsonFile::parse_json(
            Some(&composer_file_content),
            Some(&format!("{}:composer.json", identifier)),
        )?;

        // TODO(phase-b): unwrap PhpMixed::Array into IndexMap<String, PhpMixed>.
        let mut composer: IndexMap<String, PhpMixed> = match composer {
            PhpMixed::Array(a) if !a.is_empty() => a.into_iter().map(|(k, v)| (k, *v)).collect(),
            _ => return Ok(None),
        };

        if !composer.contains_key("time")
            || composer
                .get("time")
                .map_or(true, |v| v.as_string().map_or(true, |s| s.is_empty()))
        {
            if let Some(change_date) = self.get_change_date(identifier)? {
                composer.insert(
                    "time".to_string(),
                    PhpMixed::String(change_date.to_rfc3339()),
                );
            }
        }

        Ok(Some(composer))
    }

    fn has_composer_file(&mut self, identifier: &str) -> bool {
        match self.get_composer_information(identifier) {
            Ok(Some(_)) => true,
            _ => false,
        }
    }

    fn get_scheme(&self) -> &str {
        if extension_loaded("openssl") {
            return "https";
        }
        "http"
    }

    fn get_contents(&self, url: &str) -> anyhow::Result<Response, TransportException> {
        let options_mixed = self
            .repo_config()
            .get("options")
            .cloned()
            .unwrap_or(PhpMixed::Array(IndexMap::new()));
        // TODO(phase-b): convert PhpMixed::Array options into IndexMap<String, PhpMixed> properly.
        let options: IndexMap<String, PhpMixed> = match options_mixed {
            PhpMixed::Array(a) => a.into_iter().map(|(k, v)| (k, *v)).collect(),
            _ => IndexMap::new(),
        };
        // TODO(phase-b): map anyhow::Error from HttpDownloader::get into TransportException.
        self.http_downloader()
            .borrow_mut()
            .get(url, options)
            .map_err(|e| TransportException::new(e.to_string(), 0))
    }

    fn cleanup(&self) {}
}
