//! ref: composer/src/Composer/Util/Forgejo.php

use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::util::http_downloader::HttpDownloader;

#[derive(Debug)]
pub struct Forgejo {
    io: Box<dyn IOInterface>,
    config: Config,
    http_downloader: HttpDownloader,
}

impl Forgejo {
    pub fn new(io: Box<dyn IOInterface>, config: Config, http_downloader: HttpDownloader) -> Self {
        Self {
            io,
            config,
            http_downloader,
        }
    }

    /// Authorizes a Forgejo domain interactively
    pub fn authorize_o_auth_interactively(
        &mut self,
        origin_url: &str,
        message: Option<&str>,
    ) -> anyhow::Result<Result<bool, TransportException>> {
        if let Some(message) = message {
            self.io.write_error(message, true, IOInterface::NORMAL);
        }

        let url = format!("https://{}/user/settings/applications", origin_url);
        self.io.write_error(
            "Setup a personal access token with repository:read permissions on:",
            true,
            IOInterface::NORMAL,
        );
        self.io.write_error(&url, true, IOInterface::NORMAL);
        let local_auth_config = self.config.get_local_auth_config_source();
        self.io.write_error(
            &format!(
                "Tokens will be stored in plain text in \"{}\" for future use by Composer.",
                local_auth_config
                    .as_ref()
                    .map(|s| format!("{} OR ", s.get_name()))
                    .unwrap_or_default()
                    + self.config.get_auth_config_source().get_name()
            ),
            true,
            IOInterface::NORMAL,
        );
        self.io.write_error(
            "For additional information, check https://getcomposer.org/doc/articles/authentication-for-private-packages.md#forgejo-token",
            true,
            IOInterface::NORMAL,
        );

        let mut store_in_local_auth_config = false;
        if local_auth_config.is_some() {
            store_in_local_auth_config = self.io.ask_confirmation(
                "A local auth config source was found, do you want to store the token there?",
                true,
            );
        }

        let username = self.io.ask("Username: ", None).trim().to_string();
        let token = self
            .io
            .ask_and_hide_answer("Token (hidden): ")
            .trim()
            .to_string();

        let add_token_manually = format!(
            "You can also add it manually later by using \"composer config --global --auth forgejo-token.{} <username> <token>\"",
            origin_url
        );
        if token.is_empty() || username.is_empty() {
            self.io.write_error(
                "<warning>No username/token given, aborting.</warning>",
                true,
                IOInterface::NORMAL,
            );
            self.io
                .write_error(&add_token_manually, true, IOInterface::NORMAL);

            return Ok(Ok(false));
        }

        self.io
            .set_authentication(origin_url.to_string(), username.clone(), token.clone());

        match self.http_downloader.get(
            &format!("https://{}/api/v1/version", origin_url),
            indexmap::indexmap! {
                "retry-auth-failure".to_string() => false.into(),
            },
        ) {
            Ok(_) => {}
            Err(e) => {
                if [403, 401, 404].contains(&e.get_code()) {
                    self.io.write_error(
                        "<error>Invalid access token provided.</error>",
                        true,
                        IOInterface::NORMAL,
                    );
                    self.io
                        .write_error(&add_token_manually, true, IOInterface::NORMAL);

                    return Ok(Ok(false));
                }

                return Ok(Err(e));
            }
        }

        // store value in local/user config
        let local_auth_config = self.config.get_local_auth_config_source();
        let auth_config_source = if store_in_local_auth_config {
            local_auth_config
                .as_ref()
                .unwrap_or_else(|| self.config.get_auth_config_source())
        } else {
            self.config.get_auth_config_source()
        };
        self.config
            .get_config_source()
            .remove_config_setting(&format!("forgejo-token.{}", origin_url));
        auth_config_source.add_config_setting(
            &format!("forgejo-token.{}", origin_url),
            indexmap::indexmap! {
                "username".to_string() => username.into(),
                "token".to_string() => token.into(),
            },
        );

        self.io
            .write_error("<info>Token stored successfully.</info>", true, IOInterface::NORMAL);

        Ok(Ok(true))
    }
}
