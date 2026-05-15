//! ref: composer/src/Composer/Repository/Vcs/VcsDriver.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{extension_loaded, PhpMixed, JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE};

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::util::filesystem::Filesystem;
use crate::util::http::response::Response;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct VcsDriver {
    pub(crate) url: String,
    pub(crate) origin_url: String,
    pub(crate) repo_config: IndexMap<String, PhpMixed>,
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) config: Config,
    pub(crate) process: ProcessExecutor,
    pub(crate) http_downloader: HttpDownloader,
    pub(crate) info_cache: IndexMap<String, Option<IndexMap<String, PhpMixed>>>,
    pub(crate) cache: Option<Cache>,
}

impl VcsDriver {
    pub fn new(
        mut repo_config: IndexMap<String, PhpMixed>,
        io: Box<dyn IOInterface>,
        config: Config,
        http_downloader: HttpDownloader,
        process: ProcessExecutor,
    ) -> Self {
        if let Some(PhpMixed::String(url)) = repo_config.get("url").cloned() {
            if Filesystem::is_local_path(&url) {
                let platform_path = Filesystem::get_platform_path(&url);
                repo_config.insert("url".to_string(), PhpMixed::String(platform_path));
            }
        }

        let url = repo_config.get("url").and_then(|v| v.as_string()).unwrap_or("").to_string();

        Self {
            origin_url: url.clone(),
            url,
            repo_config,
            io,
            config,
            http_downloader,
            process,
            info_cache: IndexMap::new(),
            cache: None,
        }
    }

    pub(crate) fn should_cache(&self, identifier: &str) -> bool {
        self.cache.is_some() && Preg::is_match("{^[a-f0-9]{40}$}iD", identifier).unwrap_or(false)
    }

    pub fn get_composer_information(&mut self, identifier: &str) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        if !self.info_cache.contains_key(identifier) {
            if self.should_cache(identifier) {
                if let Some(res) = self.cache.as_ref().and_then(|c| c.read(identifier)) {
                    let parsed = JsonFile::parse_json(&res, None)?;
                    self.info_cache.insert(identifier.to_string(), parsed);
                    return Ok(self.info_cache.get(identifier).and_then(|v| v.clone()));
                }
            }

            let composer = self.get_base_composer_information(identifier)?;

            if self.should_cache(identifier) {
                if let Some(ref composer_map) = composer {
                    let encoded = JsonFile::encode_with_options(composer_map, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES);
                    self.cache.as_ref().map(|c| c.write(identifier, &encoded));
                }
            }

            self.info_cache.insert(identifier.to_string(), composer);
        }

        Ok(self.info_cache.get(identifier).and_then(|v| v.clone()))
    }

    pub(crate) fn get_base_composer_information(&mut self, identifier: &str) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        let composer_file_content = self.get_file_content("composer.json", identifier)?;

        let composer_file_content = match composer_file_content {
            None => return Ok(None),
            Some(c) if c.is_empty() => return Ok(None),
            Some(c) => c,
        };

        let composer = JsonFile::parse_json(&composer_file_content, Some(&format!("{}:composer.json", identifier)))?;

        let mut composer = match composer {
            None => return Ok(None),
            Some(c) if c.is_empty() => return Ok(None),
            Some(c) => c,
        };

        if !composer.contains_key("time") || composer.get("time").map_or(true, |v| v.as_string().map_or(true, |s| s.is_empty())) {
            if let Some(change_date) = self.get_change_date(identifier)? {
                composer.insert("time".to_string(), PhpMixed::String(change_date.to_rfc3339()));
            }
        }

        Ok(Some(composer))
    }

    pub fn has_composer_file(&mut self, identifier: &str) -> bool {
        match self.get_composer_information(identifier) {
            Ok(Some(_)) => true,
            _ => false,
        }
    }

    pub(crate) fn get_scheme(&self) -> &str {
        if extension_loaded("openssl") {
            return "https";
        }
        "http"
    }

    pub(crate) fn get_contents(&self, url: &str) -> anyhow::Result<Response, TransportException> {
        let options = self.repo_config.get("options").cloned().unwrap_or(PhpMixed::Array(IndexMap::new()));
        self.http_downloader.get(url, &options)
    }

    pub fn cleanup(&self) {}

    // abstract methods to be implemented by subclasses (via VcsDriverInterface trait)
    pub(crate) fn get_file_content(&self, file: &str, identifier: &str) -> anyhow::Result<Option<String>> {
        todo!()
    }

    pub(crate) fn get_change_date(&self, identifier: &str) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
        todo!()
    }
}
