//! ref: composer/src/Composer/Repository/Vcs/VcsDriver.php

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
    fn http_downloader(&self) -> &HttpDownloader;
    fn http_downloader_mut(&mut self) -> &mut HttpDownloader;
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
                if let Some(res) = self.cache().and_then(|c| c.read(identifier)) {
                    let parsed = JsonFile::parse_json(&res, None)?;
                    self.info_cache_mut().insert(identifier.to_string(), parsed);
                    return Ok(self.info_cache().get(identifier).and_then(|v| v.clone()));
                }
            }

            let composer = self.get_base_composer_information(identifier)?;

            if self.should_cache(identifier) {
                if let Some(ref composer_map) = composer {
                    let encoded = JsonFile::encode_with_options(
                        composer_map,
                        JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES,
                    );
                    self.cache().map(|c| c.write(identifier, &encoded));
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
            &composer_file_content,
            Some(&format!("{}:composer.json", identifier)),
        )?;

        let mut composer = match composer {
            None => return Ok(None),
            Some(c) if c.is_empty() => return Ok(None),
            Some(c) => c,
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
        let options = self
            .repo_config()
            .get("options")
            .cloned()
            .unwrap_or(PhpMixed::Array(IndexMap::new()));
        self.http_downloader().get(url, &options)
    }

    fn cleanup(&self) {}
}
