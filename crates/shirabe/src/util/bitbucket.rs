//! ref: composer/src/Composer/Util/Bitbucket.php

use crate::io::io_interface;
use indexmap::IndexMap;
use shirabe_php_shim::{LogicException, PhpMixed, time};

use crate::config::Config;
use crate::downloader::TransportException;
use crate::factory::Factory;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::util::HttpDownloader;
use crate::util::ProcessExecutor;

fn transport_error_code(err: &anyhow::Error) -> Option<i64> {
    err.downcast_ref::<TransportException>().map(|te| te.code)
}

#[derive(Debug)]
pub struct Bitbucket {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    token: Option<IndexMap<String, PhpMixed>>,
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

    pub fn get_token(&self) -> String {
        match &self.token {
            Some(token) => token
                .get("access_token")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    pub fn authorize_oauth(&mut self, origin_url: &str) -> bool {
        if origin_url != "bitbucket.org" {
            return false;
        }

        let mut output = PhpMixed::Null;
        if self
            .process
            .borrow_mut()
            .execute(
                PhpMixed::from(vec!["git", "config", "bitbucket.accesstoken"]),
                &mut output,
                None,
            )
            .unwrap_or(1)
            == 0
        {
            let output_str = output.as_string().unwrap_or("").trim().to_string();
            self.io.borrow_mut().set_authentication(
                origin_url.to_string(),
                "x-token-auth".to_string(),
                Some(output_str),
            );
            return true;
        }

        false
    }

    fn request_access_token(&mut self) -> anyhow::Result<bool> {
        let mut http = IndexMap::new();
        http.insert("method".to_string(), PhpMixed::String("POST".to_string()));
        http.insert(
            "content".to_string(),
            PhpMixed::String("grant_type=client_credentials".to_string()),
        );
        let mut options: IndexMap<String, PhpMixed> = IndexMap::new();
        options.insert("retry-auth-failure".to_string(), PhpMixed::Bool(false));
        options.insert("http".to_string(), PhpMixed::Array(http));

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
                        "You can also add it manually later by using \"composer config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"",
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
                return Err(LogicException {
                    message: format!(
                        "Expected a token configured with expires_in and access_token present, got {}",
                        shirabe_php_shim::json_encode(&token).unwrap_or_default()
                    ),
                    code: 0,
                }
                .into());
            }
        };
        if !token_map.contains_key("expires_in") || !token_map.contains_key("access_token") {
            return Err(LogicException {
                message: format!(
                    "Expected a token configured with expires_in and access_token present, got {}",
                    shirabe_php_shim::json_encode(&token).unwrap_or_default()
                ),
                code: 0,
            }
            .into());
        }
        self.token = Some(token_map.into_iter().collect());

        Ok(true)
    }

    pub fn authorize_oauth_interactively(
        &mut self,
        origin_url: &str,
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
            );
        }

        let consumer_key = self
            .io
            .ask_and_hide_answer("Consumer Key (hidden): ".to_string())
            .unwrap_or_default()
            .trim()
            .to_string();

        if consumer_key.is_empty() {
            self.io.write_error3(
                "<warning>No consumer key given, aborting.</warning>",
                true,
                io_interface::NORMAL,
            );
            self.io.write_error3(
                "You can also add it manually later by using \"composer config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"",
                true,
                io_interface::NORMAL,
            );
            return Ok(false);
        }

        let consumer_secret = self
            .io
            .ask_and_hide_answer("Consumer Secret (hidden): ".to_string())
            .unwrap_or_default()
            .trim()
            .to_string();

        if consumer_secret.is_empty() {
            self.io.write_error3(
                "<warning>No consumer secret given, aborting.</warning>",
                true,
                io_interface::NORMAL,
            );
            self.io.write_error3(
                "You can also add it manually later by using \"composer config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"",
                true,
                io_interface::NORMAL,
            );
            return Ok(false);
        }

        self.io.borrow_mut().set_authentication(
            origin_url.to_string(),
            consumer_key.clone(),
            Some(consumer_secret.clone()),
        );

        if !self.request_access_token()? {
            return Ok(false);
        }

        // TODO(phase-b): PHP $authConfigSource parameter is unused inside storeInAuthConfig
        //   (upstream Composer bug); the dispatch on local vs. global is dropped here too.
        let _ = store_in_local_auth_config;
        self.store_in_auth_config(origin_url, &consumer_key, &consumer_secret)?;

        self.config
            .borrow_mut()
            .get_auth_config_source_mut()
            .remove_config_setting(&format!("http-basic.{}", origin_url))?;

        self.io.write_error3(
            "<info>Consumer stored successfully.</info>",
            true,
            io_interface::NORMAL,
        );

        Ok(true)
    }

    pub fn request_token(
        &mut self,
        origin_url: &str,
        consumer_key: &str,
        consumer_secret: &str,
    ) -> anyhow::Result<String> {
        if self.token.is_some() || self.get_token_from_config(origin_url) {
            return Ok(self
                .token
                .as_ref()
                .unwrap()
                .get("access_token")
                .and_then(|v| v.as_string())
                .map(|s| s.to_string())
                .unwrap_or_default());
        }

        self.io.borrow_mut().set_authentication(
            origin_url.to_string(),
            consumer_key.to_string(),
            Some(consumer_secret.to_string()),
        );
        if !self.request_access_token()? {
            return Ok(String::new());
        }

        // TODO(phase-b): PHP $authConfigSource parameter is unused inside storeInAuthConfig
        //   (upstream Composer bug); the dispatch on local vs. global is dropped here too.
        self.store_in_auth_config(origin_url, consumer_key, consumer_secret)?;

        let access_token = self
            .token
            .as_ref()
            .and_then(|t| t.get("access_token"))
            .and_then(|v| v.as_string())
            .map(|s| s.to_string());

        match access_token {
            Some(t) => Ok(t),
            None => Err(LogicException {
                message: "Failed to initialize token above".to_string(),
                code: 0,
            }
            .into()),
        }
    }

    // TODO(phase-b): PHP $authConfigSource parameter dropped — unused in upstream Composer too.
    fn store_in_auth_config(
        &mut self,
        origin_url: &str,
        consumer_key: &str,
        consumer_secret: &str,
    ) -> anyhow::Result<()> {
        self.config
            .borrow_mut()
            .get_config_source_mut()
            .remove_config_setting(&format!("bitbucket-oauth.{}", origin_url))?;

        let token = self.token.as_ref().ok_or_else(|| LogicException {
            message: "Expected a token configured with expires_in present, got null".to_string(),
            code: 0,
        })?;
        let expires_in = token
            .get("expires_in")
            .and_then(|v| v.as_int())
            .ok_or_else(|| {
                let token_mixed =
                    PhpMixed::Array(token.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                LogicException {
                    message: format!(
                        "Expected a token configured with expires_in present, got {}",
                        shirabe_php_shim::json_encode(&token_mixed).unwrap_or_default()
                    ),
                    code: 0,
                }
            })?;

        let t = self.time.unwrap_or_else(time);
        let mut consumer = IndexMap::new();
        consumer.insert(
            "consumer-key".to_string(),
            PhpMixed::String(consumer_key.to_string()),
        );
        consumer.insert(
            "consumer-secret".to_string(),
            PhpMixed::String(consumer_secret.to_string()),
        );
        consumer.insert(
            "access-token".to_string(),
            token.get("access_token").cloned().unwrap_or(PhpMixed::Null),
        );
        consumer.insert(
            "access-token-expiration".to_string(),
            PhpMixed::Int(t + expires_in),
        );

        self.config
            .borrow_mut()
            .get_auth_config_source_mut()
            .add_config_setting(
                &format!("bitbucket-oauth.{}", origin_url),
                PhpMixed::Array(consumer),
            )?;

        Ok(())
    }

    fn get_token_from_config(&mut self, origin_url: &str) -> bool {
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

        if !origin_config.contains_key("access-token")
            || !origin_config.contains_key("access-token-expiration")
        {
            return false;
        }
        if let Some(expiration) = origin_config
            .get("access-token-expiration")
            .and_then(|v| v.as_int())
        {
            if time() > expiration {
                return false;
            }
        } else {
            return false;
        }

        let access_token = match origin_config.get("access-token").cloned() {
            Some(t) => t,
            None => return false,
        };
        let mut token = IndexMap::new();
        token.insert("access_token".to_string(), access_token);
        self.token = Some(token);

        true
    }
}
