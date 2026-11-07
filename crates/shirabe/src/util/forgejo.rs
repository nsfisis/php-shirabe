//! ref: composer/src/Composer/Util/Forgejo.php sha1:f5ff8636de3de676034cbdc6307dd2b53bb34611

use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::{IOInterface, IOInterfaceWrite};
use crate::util::{HttpDownloader, StreamContextOptions};
use bstr::{BStr, BString};
use shirabe_php_shim::{Catch as _, PhpMixed, bformat, trim};
use shirabe_symfony_console::input::InputValue;

#[derive(Debug)]
pub struct Forgejo {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
}

impl Forgejo {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
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
        origin_url: &BStr,
        message: Option<&BStr>,
    ) -> anyhow::Result<Result<bool, TransportException>> {
        if let Some(message) = message {
            self.io.write_error(message);
        }

        let url = bformat!("https://{origin_url}/user/settings/applications");
        self.io
            .write_error("Setup a personal access token with repository:read permissions on:");
        self.io.write_error(&url);
        let (local_auth_name, has_local_auth, auth_name): (BString, bool, BString) = {
            let cfg = self.config.borrow();
            let local = cfg.get_local_auth_config_source().map(|s| s.get_name());
            let auth = cfg.get_auth_config_source().get_name();
            (local.clone().unwrap_or_default(), local.is_some(), auth)
        };
        let local_prefix = if has_local_auth {
            bformat!("{local_auth_name} OR ")
        } else {
            BString::default()
        };
        self.io.write_error(bformat!("Tokens will be stored in plain text in \"{local_prefix}{auth_name}\" for future use by Composer."));
        self.io.write_error("For additional information, check https://getcomposer.org/doc/articles/authentication-for-private-packages.md#forgejo-token");

        let mut store_in_local_auth_config = false;
        if has_local_auth {
            store_in_local_auth_config = self.io.ask_confirmation(
                "A local auth config source was found, do you want to store the token there?",
                true,
            )?;
        }

        let username = self
            .io
            .ask("Username: ", InputValue::Null)?
            .as_string()
            .map(trim)
            .unwrap_or_default();
        let token = self
            .io
            .ask_and_hide_answer("Token (hidden): ")?
            .map(trim)
            .unwrap_or_default();

        let add_token_manually = bformat!(
            "You can also add it manually later by using \"shirabe config --global --auth forgejo-token.{origin_url} <username> <token>\""
        );
        if token.is_empty() || username.is_empty() {
            self.io
                .write_error("<warning>No username/token given, aborting.</warning>");
            self.io.write_error(&add_token_manually);

            return Ok(Ok(false));
        }

        self.io.borrow_mut().set_authentication(
            origin_url.into(),
            username.clone(),
            Some(token.clone()),
        );

        match self.http_downloader.borrow_mut().get(
            bformat!("https://{origin_url}/api/v1/version"),
            {
                let mut options = StreamContextOptions::new();
                options.set_retry_auth_failure(false);
                options
            },
        ) {
            Ok(_) => {}
            Err(e) => {
                let code = e
                    .catch::<TransportException>()
                    .map(|te| te.get_code())
                    .unwrap_or(0);
                if [403, 401, 404].contains(&code) {
                    self.io
                        .write_error("<error>Invalid access token provided.</error>");
                    self.io.write_error(&add_token_manually);

                    return Ok(Ok(false));
                }

                return Err(e);
            }
        }

        // store value in local/user config
        let setting_key = bformat!("forgejo-token.{origin_url}");
        {
            let mut cfg = self.config.borrow_mut();
            cfg.get_config_source_mut()
                .remove_config_setting(bstr::BStr::new(&setting_key))?;
        }
        let value = PhpMixed::Array(indexmap::indexmap! {
            "username".into() => PhpMixed::string(username),
            "token".into() => PhpMixed::string(token),
        });
        if store_in_local_auth_config && has_local_auth {
            let mut cfg = self.config.borrow_mut();
            if let Some(local) = cfg.get_local_auth_config_source_mut() {
                local.add_config_setting(bstr::BStr::new(&setting_key), value)?;
            }
        } else {
            let mut cfg = self.config.borrow_mut();
            cfg.get_auth_config_source_mut()
                .add_config_setting(bstr::BStr::new(&setting_key), value)?;
        }

        self.io
            .write_error("<info>Token stored successfully.</info>");

        Ok(Ok(true))
    }
}
