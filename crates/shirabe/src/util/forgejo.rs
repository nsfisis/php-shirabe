//! ref: composer/src/Composer/Util/Forgejo.php

use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::IOInterface;
use crate::io::io_interface;
use crate::util::HttpDownloader;

#[derive(Debug)]
pub struct Forgejo {
    io: Box<dyn IOInterface>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
}

impl Forgejo {
    pub fn new(
        io: Box<dyn IOInterface>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    ) -> Self {
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
            self.io.write_error3(message, true, io_interface::NORMAL);
        }

        let url = format!("https://{}/user/settings/applications", origin_url);
        self.io.write_error3(
            "Setup a personal access token with repository:read permissions on:",
            true,
            io_interface::NORMAL,
        );
        self.io.write_error3(&url, true, io_interface::NORMAL);
        let (local_auth_name, has_local_auth, auth_name): (String, bool, String) = {
            let cfg = self.config.borrow();
            let local = cfg
                .get_local_auth_config_source()
                .map(|s| s.get_name().to_string());
            let auth = cfg.get_auth_config_source().get_name().to_string();
            (local.clone().unwrap_or_default(), local.is_some(), auth)
        };
        let local_prefix = if has_local_auth {
            format!("{} OR ", local_auth_name)
        } else {
            String::new()
        };
        self.io.write_error3(
            &format!(
                "Tokens will be stored in plain text in \"{}{}\" for future use by Composer.",
                local_prefix, auth_name
            ),
            true,
            io_interface::NORMAL,
        );
        self.io.write_error3(
            "For additional information, check https://getcomposer.org/doc/articles/authentication-for-private-packages.md#forgejo-token",
            true,
            io_interface::NORMAL,
        );

        let mut store_in_local_auth_config = false;
        if has_local_auth {
            store_in_local_auth_config = self.io.ask_confirmation(
                "A local auth config source was found, do you want to store the token there?"
                    .to_string(),
                true,
            );
        }

        let username = self
            .io
            .ask("Username: ".to_string(), shirabe_php_shim::PhpMixed::Null)
            .as_string()
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let token = self
            .io
            .ask_and_hide_answer("Token (hidden): ".to_string())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        let add_token_manually = format!(
            "You can also add it manually later by using \"composer config --global --auth forgejo-token.{} <username> <token>\"",
            origin_url
        );
        if token.is_empty() || username.is_empty() {
            self.io.write_error3(
                "<warning>No username/token given, aborting.</warning>",
                true,
                io_interface::NORMAL,
            );
            self.io
                .write_error3(&add_token_manually, true, io_interface::NORMAL);

            return Ok(Ok(false));
        }

        self.io.set_authentication(
            origin_url.to_string(),
            username.clone(),
            Some(token.clone()),
        );

        match self.http_downloader.borrow_mut().get(
            &format!("https://{}/api/v1/version", origin_url),
            indexmap::indexmap! {
                "retry-auth-failure".to_string() => false.into(),
            },
        ) {
            Ok(_) => {}
            Err(e) => {
                // TODO(phase-b): anyhow::Error has no get_code(); HTTP status codes come from
                // TransportException::get_status_code().
                let code = e
                    .downcast_ref::<crate::downloader::TransportException>()
                    .and_then(|te| te.get_status_code())
                    .unwrap_or(0);
                if [403, 401, 404].contains(&code) {
                    self.io.write_error3(
                        "<error>Invalid access token provided.</error>",
                        true,
                        io_interface::NORMAL,
                    );
                    self.io
                        .write_error3(&add_token_manually, true, io_interface::NORMAL);

                    return Ok(Ok(false));
                }

                // TODO(phase-b): downcast anyhow::Error to TransportException for the inner Err
                return Err(e);
            }
        }

        // store value in local/user config
        // TODO(phase-b): Config getters return references; cross-borrows of self.config.borrow()
        // cannot live across method calls. Needs Rc<RefCell<dyn ConfigSourceInterface>> shape.
        let setting_key = format!("forgejo-token.{}", origin_url);
        {
            let mut cfg = self.config.borrow_mut();
            cfg.get_config_source_mut()
                .remove_config_setting(&setting_key)?;
        }
        let value: shirabe_php_shim::PhpMixed =
            shirabe_php_shim::PhpMixed::Array(indexmap::indexmap! {
                "username".to_string() => Box::new(username.clone().into()),
                "token".to_string() => Box::new(token.clone().into()),
            });
        if store_in_local_auth_config && has_local_auth {
            let mut cfg = self.config.borrow_mut();
            if let Some(local) = cfg.get_local_auth_config_source_mut() {
                local.add_config_setting(&setting_key, value)?;
            }
        } else {
            let mut cfg = self.config.borrow_mut();
            cfg.get_auth_config_source_mut()
                .add_config_setting(&setting_key, value)?;
        }

        self.io.write_error3(
            "<info>Token stored successfully.</info>",
            true,
            io_interface::NORMAL,
        );

        Ok(Ok(true))
    }
}
