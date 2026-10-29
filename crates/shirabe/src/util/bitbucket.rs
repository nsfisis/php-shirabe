//! ref: composer/src/Composer/Util/Bitbucket.php

use crate::config::Config;
use crate::downloader::TransportException;
use crate::factory::Factory;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::util::{HttpDownloader, ProcessExecutor};
use bstr::{B, BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, LogicException, PhpMixed, bconcat, json_encode, legacy_lossy, time, trim,
};

fn transport_error_code(err: &anyhow::Error) -> Option<i64> {
    err.catch::<TransportException>().map(|te| te.get_code())
}

#[derive(Debug)]
pub struct Bitbucket {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    token: Option<IndexMap<BString, PhpMixed>>,
    time: Option<i64>,
}

impl Bitbucket {
    pub const OAUTH2_ACCESS_TOKEN_URL: &'static str =
        "https://bitbucket.org/site/oauth2/access_token";

    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
        http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
        time: Option<i64>,
    ) -> anyhow::Result<Self> {
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone(),
            ))))
        });
        let http_downloader = match http_downloader {
            Some(h) => h,
            None => std::rc::Rc::new(std::cell::RefCell::new(Factory::create_http_downloader(
                io.clone(),
                &config,
                IndexMap::new(),
            )?)),
        };
        Ok(Self {
            io,
            config,
            process,
            http_downloader,
            token: None,
            time,
        })
    }

    pub fn get_token(&self) -> BString {
        match &self.token {
            Some(token) => token
                .get(B("access_token"))
                .and_then(|v| v.as_string())
                .map(BString::from)
                .unwrap_or_default(),
            None => BString::default(),
        }
    }

    pub fn authorize_oauth(&mut self, origin_url: &BStr) -> anyhow::Result<bool> {
        if origin_url != "bitbucket.org" {
            return Ok(false);
        }

        let mut output = PhpMixed::Null;
        if self.process.borrow_mut().execute(
            &["git", "config", "bitbucket.accesstoken"],
            &mut output,
            None,
        )? == 0
        {
            let output_str = trim(output.as_string().unwrap_or_default());
            self.io.borrow_mut().set_authentication(
                origin_url.into(),
                BString::from("x-token-auth"),
                Some(output_str),
            );
            return Ok(true);
        }

        Ok(false)
    }

    fn request_access_token(&mut self) -> anyhow::Result<bool> {
        let mut http = IndexMap::new();
        http.insert("method".into(), PhpMixed::string("POST".to_string()));
        http.insert(
            "content".into(),
            PhpMixed::string("grant_type=client_credentials".to_string()),
        );
        let mut options: IndexMap<BString, PhpMixed> = IndexMap::new();
        options.insert("retry-auth-failure".into(), PhpMixed::Bool(false));
        options.insert("http".into(), PhpMixed::Array(http));

        let response = match self
            .http_downloader
            .borrow_mut()
            .get(Self::OAUTH2_ACCESS_TOKEN_URL, options)
        {
            Ok(r) => r,
            Err(te) => {
                let code = transport_error_code(&te).unwrap_or(0);
                if code == 400 {
                    self.io.write_error3(
                        "<error>Invalid OAuth consumer provided.</error>",
                        true,
                        io_interface::NORMAL,
                    );
                    self.io.write_error3(
                        "This can have three reasons:",
                        true,
                        io_interface::NORMAL,
                    );
                    self.io.write_error3(
                        "1. You are authenticating with a bitbucket username/password combination",
                        true,
                        io_interface::NORMAL,
                    );
                    self.io.write_error3(
                        "2. You are using an OAuth consumer, but didn't configure a (dummy) callback url",
                        true,
                        io_interface::NORMAL,
                    );
                    self.io.write_error3(
                        "3. You are using an OAuth consumer, but didn't configure it as private consumer",
                        true,
                        io_interface::NORMAL,
                    );
                    return Ok(false);
                }
                if code == 403 || code == 401 {
                    self.io.write_error3(
                        "<error>Invalid OAuth consumer provided.</error>",
                        true,
                        io_interface::NORMAL,
                    );
                    self.io.write_error3(
                        "You can also add it manually later by using \"shirabe config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"",
                        true,
                        io_interface::NORMAL,
                    );
                    return Ok(false);
                }
                return Err(te);
            }
        };

        let token = response.decode_json()?;
        let token_map = match token {
            PhpMixed::Array(ref m) => m.clone(),
            _ => {
                return Err(LogicException::new(format!(
                    "Expected a token configured with expires_in and access_token present, got {}",
                    json_encode(&token).unwrap_or_default()
                ))
                .into());
            }
        };
        if !token_map.contains_key(B("expires_in")) || !token_map.contains_key(B("access_token")) {
            return Err(LogicException::new(format!(
                "Expected a token configured with expires_in and access_token present, got {}",
                json_encode(&token).unwrap_or_default()
            ))
            .into());
        }
        self.token = Some(token_map.into_iter().collect());

        Ok(true)
    }

    pub fn authorize_oauth_interactively(
        &mut self,
        origin_url: &BStr,
        message: Option<&str>,
    ) -> anyhow::Result<bool> {
        if let Some(msg) = message {
            self.io.write_error3(msg, true, io_interface::NORMAL);
        }

        let local_auth_config_name: Option<String> = self
            .config
            .borrow()
            .get_local_auth_config_source()
            .map(|c| c.get_name());
        let has_local_auth_config = local_auth_config_name.is_some();
        let auth_config_source_name = self.config.borrow().get_auth_config_source().get_name();
        let url =
            "https://support.atlassian.com/bitbucket-cloud/docs/use-oauth-on-bitbucket-cloud/";
        self.io
            .write_error3("Follow the instructions here:", true, io_interface::NORMAL);
        self.io.write_error3(url, true, io_interface::NORMAL);
        let local_name_prefix = local_auth_config_name
            .as_ref()
            .map(|name| format!("{} OR ", name))
            .unwrap_or_default();
        self.io.write_error3(
            &format!(
                "to create a consumer. It will be stored in \"{}\" for future use by Composer.",
                local_name_prefix + &auth_config_source_name
            ),
            true,
            io_interface::NORMAL,
        );
        self.io.write_error3(
            "Ensure you enter a \"Callback URL\" (http://example.com is fine) or it will not be possible to create an Access Token (this callback url will not be used by composer)",
            true,
            io_interface::NORMAL,
        );

        let mut store_in_local_auth_config = false;
        if has_local_auth_config {
            store_in_local_auth_config = self.io.ask_confirmation(
                "A local auth config source was found, do you want to store the token there?"
                    .to_string(),
                true,
            )?;
        }

        let consumer_key = self
            .io
            .ask_and_hide_answer("Consumer Key (hidden): ".to_string())?
            .unwrap_or_default();
        let consumer_key = trim(consumer_key);

        if consumer_key.is_empty() {
            self.io.write_error3(
                "<warning>No consumer key given, aborting.</warning>",
                true,
                io_interface::NORMAL,
            );
            self.io.write_error3(
                "You can also add it manually later by using \"shirabe config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"",
                true,
                io_interface::NORMAL,
            );
            return Ok(false);
        }

        let consumer_secret = self
            .io
            .ask_and_hide_answer("Consumer Secret (hidden): ".to_string())?
            .unwrap_or_default();
        let consumer_secret = trim(consumer_secret);

        if consumer_secret.is_empty() {
            self.io.write_error3(
                "<warning>No consumer secret given, aborting.</warning>",
                true,
                io_interface::NORMAL,
            );
            self.io.write_error3(
                "You can also add it manually later by using \"shirabe config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"",
                true,
                io_interface::NORMAL,
            );
            return Ok(false);
        }

        self.io.borrow_mut().set_authentication(
            origin_url.into(),
            consumer_key.clone(),
            Some(consumer_secret.clone()),
        );

        if !self.request_access_token()? {
            return Ok(false);
        }

        // NOTE: PHP $authConfigSource parameter is unused inside storeInAuthConfig
        //   (upstream Composer bug); the dispatch on local vs. global is dropped here too.
        let _ = store_in_local_auth_config;
        self.store_in_auth_config(origin_url, consumer_key.as_ref(), consumer_secret.as_ref())?;

        self.config
            .borrow_mut()
            .get_auth_config_source_mut()
            .remove_config_setting(&legacy_lossy(&bconcat!("http-basic.", origin_url)))?;

        self.io.write_error3(
            "<info>Consumer stored successfully.</info>",
            true,
            io_interface::NORMAL,
        );

        Ok(true)
    }

    pub fn request_token(
        &mut self,
        origin_url: &BStr,
        consumer_key: &BStr,
        consumer_secret: &BStr,
    ) -> anyhow::Result<BString> {
        if self.token.is_some() || self.get_token_from_config(origin_url) {
            return Ok(self
                .token
                .as_ref()
                .unwrap()
                .get(B("access_token"))
                .and_then(|v| v.as_string())
                .map(BString::from)
                .unwrap_or_default());
        }

        self.io.borrow_mut().set_authentication(
            origin_url.into(),
            consumer_key.into(),
            Some(consumer_secret.into()),
        );
        if !self.request_access_token()? {
            return Ok(BString::default());
        }

        // NOTE: PHP $authConfigSource parameter is unused inside storeInAuthConfig
        //   (upstream Composer bug); the dispatch on local vs. global is dropped here too.
        self.store_in_auth_config(origin_url, consumer_key, consumer_secret)?;

        let access_token = self
            .token
            .as_ref()
            .and_then(|t| t.get(B("access_token")))
            .and_then(|v| v.as_string())
            .map(BString::from);

        match access_token {
            Some(t) => Ok(t),
            None => Err(LogicException::new("Failed to initialize token above".to_string()).into()),
        }
    }

    // NOTE: PHP $authConfigSource parameter dropped — unused in upstream Composer too.
    fn store_in_auth_config(
        &mut self,
        origin_url: &BStr,
        consumer_key: &BStr,
        consumer_secret: &BStr,
    ) -> anyhow::Result<()> {
        self.config
            .borrow_mut()
            .get_config_source_mut()
            .remove_config_setting(&legacy_lossy(&bconcat!("bitbucket-oauth.", origin_url)))?;

        let token = self.token.as_ref().ok_or_else(|| {
            LogicException::new(
                "Expected a token configured with expires_in present, got null".to_string(),
            )
        })?;
        let expires_in = token
            .get(B("expires_in"))
            .and_then(|v| v.as_int())
            .ok_or_else(|| {
                let token_mixed =
                    PhpMixed::Array(token.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                LogicException::new(format!(
                    "Expected a token configured with expires_in present, got {}",
                    json_encode(&token_mixed).unwrap_or_default()
                ))
            })?;

        let t = self.time.unwrap_or_else(time);
        let mut consumer = IndexMap::new();
        consumer.insert("consumer-key".into(), PhpMixed::string(consumer_key));
        consumer.insert("consumer-secret".into(), PhpMixed::string(consumer_secret));
        consumer.insert(
            "access-token".into(),
            token
                .get(B("access_token"))
                .cloned()
                .unwrap_or(PhpMixed::Null),
        );
        consumer.insert(
            "access-token-expiration".into(),
            PhpMixed::Int(t + expires_in),
        );

        self.config
            .borrow_mut()
            .get_auth_config_source_mut()
            .add_config_setting(
                &legacy_lossy(&bconcat!("bitbucket-oauth.", origin_url)),
                PhpMixed::Array(consumer),
            )?;

        Ok(())
    }

    fn get_token_from_config(&mut self, origin_url: &BStr) -> bool {
        let auth_config = self.config.borrow_mut().get("bitbucket-oauth");

        let auth_map = match auth_config.as_array() {
            Some(m) => m.clone(),
            None => return false,
        };
        let origin_config = match auth_map.get(origin_url) {
            Some(v) => match v.as_array() {
                Some(m) => m.clone(),
                None => return false,
            },
            None => return false,
        };

        if !origin_config.contains_key(B("access-token"))
            || !origin_config.contains_key(B("access-token-expiration"))
        {
            return false;
        }
        if let Some(expiration) = origin_config
            .get(B("access-token-expiration"))
            .and_then(|v| v.as_int())
        {
            if time() > expiration {
                return false;
            }
        } else {
            return false;
        }

        let access_token = match origin_config.get(B("access-token")).cloned() {
            Some(t) => t,
            None => return false,
        };
        let mut token = IndexMap::new();
        token.insert("access_token".into(), access_token);
        self.token = Some(token);

        true
    }
}
