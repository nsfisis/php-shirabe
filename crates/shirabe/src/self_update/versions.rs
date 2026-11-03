//! ref: composer/src/Composer/SelfUpdate/Versions.php

use crate::config::Config;
use crate::io::{IOInterface, IOInterfaceWrite};
use crate::util::HttpDownloader;
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    InvalidArgumentException, PHP_EOL, PhpMixed, UnexpectedValueException, bconcat, implode,
    path_from_bytes, php_regex, php_str, preg_is_match, trim,
};

pub struct Versions {
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    channel: Option<BString>,
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
    pub const CHANNELS: &'static [&'static BStr] = &[
        php_str(b"stable"),
        php_str(b"preview"),
        php_str(b"snapshot"),
        php_str(b"1"),
        php_str(b"2"),
        php_str(b"2.2"),
    ];

    pub fn new(
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    ) -> Self {
        Self {
            http_downloader,
            config,
            channel: None,
            versions_data: None,
        }
    }

    pub fn get_channel(&mut self) -> anyhow::Result<BString> {
        if let Some(ref ch) = self.channel {
            return Ok(ch.clone());
        }

        let channel_file = bconcat!(
            self.config
                .borrow_mut()
                .get("home")
                .as_string()
                .unwrap_or_default(),
            "/update-channel"
        );
        let channel_file = path_from_bytes(&channel_file);
        if channel_file.exists() {
            let channel = trim(std::fs::read(channel_file)?);
            if let Some(channel) = ["stable", "preview", "snapshot", "2.2"]
                .into_iter()
                .find(|known| channel == *known)
            {
                self.channel = Some(bconcat!(channel));
                return Ok(bconcat!(channel));
            }
        }

        self.channel = Some(BString::from("stable"));
        Ok(BString::from("stable"))
    }

    pub fn set_channel(
        &mut self,
        channel: BString,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    ) -> anyhow::Result<Result<(), InvalidArgumentException>> {
        if !Self::CHANNELS.contains(&channel.as_bstr()) {
            return Ok(Err(InvalidArgumentException::new(bconcat!(
                "Invalid channel ",
                channel,
                ", must be one of: ",
                implode(", ", Self::CHANNELS)
            ))));
        }

        let channel_file = bconcat!(
            self.config
                .borrow_mut()
                .get("home")
                .as_string()
                .unwrap_or_default(),
            "/update-channel"
        );
        let channel_file = path_from_bytes(&channel_file);
        self.channel = Some(channel.clone());

        // rewrite '2' and '1' channels to stable for future self-updates, but LTS ones like '2.2' remain pinned
        let stored_channel = if preg_is_match(php_regex!(r"{^\d+$}D"), &channel) {
            BString::from("stable")
        } else {
            channel
        };

        let previously_stored: Option<BString> = if channel_file.exists() {
            Some(trim(std::fs::read(channel_file)?))
        } else {
            None
        };
        std::fs::write(channel_file, bconcat!(stored_channel, PHP_EOL))?;

        if let Some(io) = io
            && previously_stored
                .as_ref()
                .is_none_or(|stored| *stored != stored_channel)
        {
            io.write_error(bconcat!(
                "Storing \"<info>",
                stored_channel,
                "</info>\" as default update channel for the next self-update run."
            ));
        }

        Ok(Ok(()))
    }

    pub fn get_latest(
        &mut self,
        channel: Option<&BStr>,
    ) -> anyhow::Result<Result<IndexMap<BString, PhpMixed>, UnexpectedValueException>> {
        let versions = self.get_versions_data()?;
        let effective_channel = match channel {
            Some(c) => c.to_owned(),
            None => self.get_channel()?,
        };

        if let PhpMixed::Array(ref map) = versions
            && let Some(channel_versions) = map.get(B(&effective_channel))
            && let PhpMixed::List(ref list) = *channel_versions
        {
            for version in list {
                if let PhpMixed::Array(ref v) = *version {
                    let min_php = v.get(B("min-php")).and_then(|p| p.as_int()).unwrap_or(0);
                    if min_php <= shirabe_php_rpc::get_php_version().version_id {
                        return Ok(Ok(v
                            .iter()
                            .map(|(k, val)| (k.clone(), val.clone()))
                            .collect()));
                    }
                }
            }
        }

        Ok(Err(UnexpectedValueException::new(bconcat!(
            "There is no version of Composer available for your PHP version (",
            shirabe_php_rpc::get_php_version().version,
            ")"
        ))))
    }

    fn get_versions_data(&mut self) -> anyhow::Result<PhpMixed> {
        if self.versions_data.is_none() {
            let protocol = if self.config.borrow_mut().get("disable-tls").as_bool() == Some(true) {
                "http"
            } else {
                "https"
            };

            self.versions_data = Some(
                self.http_downloader
                    .borrow_mut()
                    .get(
                        bconcat!(protocol, "://getcomposer.org/versions"),
                        IndexMap::new(),
                    )?
                    .decode_json()?,
            );
        }

        Ok(self.versions_data.clone().unwrap())
    }
}
