//! ref: composer/src/Composer/SelfUpdate/Versions.php

use crate::config::Config;
use crate::io::io_interface::IOInterface;
use crate::util::http_downloader::HttpDownloader;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, PHP_EOL, PHP_VERSION, PHP_VERSION_ID, PhpMixed,
    UnexpectedValueException,
};

pub struct Versions {
    pub channels: Vec<String>,
    http_downloader: HttpDownloader,
    config: Config,
    channel: Option<String>,
    versions_data: Option<PhpMixed>,
}

impl std::fmt::Debug for Versions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Versions")
            .field("channel", &self.channel)
            .finish()
    }
}

impl Versions {
    pub const CHANNELS: &'static [&'static str] =
        &["stable", "preview", "snapshot", "1", "2", "2.2"];

    pub fn new(config: Config, http_downloader: HttpDownloader) -> Self {
        Self {
            channels: Self::CHANNELS.iter().map(|s| s.to_string()).collect(),
            http_downloader,
            config,
            channel: None,
            versions_data: None,
        }
    }

    pub fn get_channel(&mut self) -> anyhow::Result<String> {
        if let Some(ref ch) = self.channel {
            return Ok(ch.clone());
        }

        let channel_file = format!("{}/update-channel", self.config.get("home"));
        if std::path::Path::new(&channel_file).exists() {
            let channel = std::fs::read_to_string(&channel_file)?.trim().to_string();
            if ["stable", "preview", "snapshot", "2.2"].contains(&channel.as_str()) {
                self.channel = Some(channel.clone());
                return Ok(channel);
            }
        }

        self.channel = Some("stable".to_string());
        Ok("stable".to_string())
    }

    pub fn set_channel(
        &mut self,
        channel: String,
        io: Option<&dyn IOInterface>,
    ) -> anyhow::Result<Result<(), InvalidArgumentException>> {
        if !Self::CHANNELS.contains(&channel.as_str()) {
            return Ok(Err(InvalidArgumentException {
                message: format!(
                    "Invalid channel {}, must be one of: {}",
                    channel,
                    Self::CHANNELS.join(", ")
                ),
                code: 0,
            }));
        }

        let channel_file = format!("{}/update-channel", self.config.get("home"));
        self.channel = Some(channel.clone());

        // rewrite '2' and '1' channels to stable for future self-updates, but LTS ones like '2.2' remain pinned
        let stored_channel = if Preg::is_match(r"^\d+$", &channel) {
            "stable".to_string()
        } else {
            channel.clone()
        };

        let previously_stored: Option<String> = if std::path::Path::new(&channel_file).exists() {
            Some(std::fs::read_to_string(&channel_file)?.trim().to_string())
        } else {
            None
        };
        std::fs::write(&channel_file, format!("{}{}", stored_channel, PHP_EOL))?;

        if let Some(io) = io {
            if previously_stored.as_deref() != Some(&stored_channel) {
                io.write_error(&format!(
                    "Storing \"<info>{}</info>\" as default update channel for the next self-update run.",
                    stored_channel
                ));
            }
        }

        Ok(Ok(()))
    }

    pub fn get_latest(
        &mut self,
        channel: Option<&str>,
    ) -> anyhow::Result<Result<IndexMap<String, PhpMixed>, UnexpectedValueException>> {
        let versions = self.get_versions_data()?;
        let effective_channel = match channel {
            Some(c) => c.to_string(),
            None => self.get_channel()?,
        };

        if let PhpMixed::Array(ref map) = versions {
            if let Some(channel_versions) = map.get(&effective_channel) {
                if let PhpMixed::List(ref list) = **channel_versions {
                    for version in list {
                        if let PhpMixed::Array(ref v) = **version {
                            let min_php = v.get("min-php").and_then(|p| p.as_int()).unwrap_or(0);
                            if min_php <= PHP_VERSION_ID {
                                return Ok(Ok(v
                                    .iter()
                                    .map(|(k, val)| (k.clone(), *val.clone()))
                                    .collect()));
                            }
                        }
                    }
                }
            }
        }

        Ok(Err(UnexpectedValueException {
            message: format!(
                "There is no version of Composer available for your PHP version ({})",
                PHP_VERSION
            ),
            code: 0,
        }))
    }

    fn get_versions_data(&mut self) -> anyhow::Result<PhpMixed> {
        if self.versions_data.is_none() {
            let protocol = if self.config.get("disable-tls").as_bool() == Some(true) {
                "http"
            } else {
                "https"
            };

            self.versions_data = Some(
                self.http_downloader
                    .get(&format!("{}://getcomposer.org/versions", protocol))?
                    .decode_json()?,
            );
        }

        Ok(self.versions_data.clone().unwrap())
    }
}
