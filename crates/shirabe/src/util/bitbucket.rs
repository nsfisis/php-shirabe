//! ref: composer/src/Composer/Util/Bitbucket.php

use indexmap::IndexMap;
use shirabe_php_shim::{LogicException, PhpMixed, time};

use crate::config::Config;
use crate::config::config_source_interface::ConfigSourceInterface;
use crate::downloader::transport_exception::TransportException;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct Bitbucket {
    io: Box<dyn IOInterface>,
    config: Config,
    process: ProcessExecutor,
    http_downloader: HttpDownloader,
    token: Option<IndexMap<String, PhpMixed>>,
    time: Option<i64>,
}

impl Bitbucket {
    pub const OAUTH2_ACCESS_TOKEN_URL: &'static str =
        "https://bitbucket.org/site/oauth2/access_token";

    pub fn new(
        io: Box<dyn IOInterface>,
        config: Config,
        process: Option<ProcessExecutor>,
        http_downloader: Option<HttpDownloader>,
        time: Option<i64>,
    ) -> anyhow::Result<Self> {
        let process = process.unwrap_or_else(|| ProcessExecutor::new(&*io));
        let http_downloader = match http_downloader {
            Some(h) => h,
            None => Factory::create_http_downloader(&*io, &config)?,
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

        let mut output = String::new();
        if self.process.execute(
            &[
                "git".to_string(),
                "config".to_string(),
                "bitbucket.accesstoken".to_string(),
            ],
            &mut output,
            None,
        ) == 0
        {
            self.io.set_authentication(
                origin_url.to_string(),
                "x-token-auth".to_string(),
                Some(output.trim().to_string()),
            );
            return true;
        }

        false
    }

    fn request_access_token(&mut self) -> anyhow::Result<bool> {
        let mut http = IndexMap::new();
        http.insert(
            "method".to_string(),
            Box::new(PhpMixed::String("POST".to_string())),
        );
        http.insert(
            "content".to_string(),
            Box::new(PhpMixed::String(
                "grant_type=client_credentials".to_string(),
            )),
        );
        let mut options = IndexMap::new();
        options.insert(
            "retry-auth-failure".to_string(),
            Box::new(PhpMixed::Bool(false)),
        );
        options.insert("http".to_string(), Box::new(PhpMixed::Array(http)));
        let options = PhpMixed::Array(options);

        let response = match self
            .http_downloader
            .get(Self::OAUTH2_ACCESS_TOKEN_URL, &options)
        {
            Ok(r) => r,
            Err(te) => {
                if te.code == 400 {
                    self.io.write_error(
                        PhpMixed::String(
                            "<error>Invalid OAuth consumer provided.</error>".to_string(),
                        ),
                        true,
                        IOInterface::NORMAL,
                    );
                    self.io.write_error(
                        PhpMixed::String("This can have three reasons:".to_string()),
                        true,
                        IOInterface::NORMAL,
                    );
                    self.io.write_error(
                            PhpMixed::String(
                                "1. You are authenticating with a bitbucket username/password combination".to_string(),
                            ),
                            true,
                            IOInterface::NORMAL,
                        );
                    self.io.write_error(
                            PhpMixed::String(
                                "2. You are using an OAuth consumer, but didn't configure a (dummy) callback url".to_string(),
                            ),
                            true,
                            IOInterface::NORMAL,
                        );
                    self.io.write_error(
                            PhpMixed::String(
                                "3. You are using an OAuth consumer, but didn't configure it as private consumer".to_string(),
                            ),
                            true,
                            IOInterface::NORMAL,
                        );
                    return Ok(false);
                }
                if te.code == 403 || te.code == 401 {
                    self.io.write_error(
                        PhpMixed::String(
                            "<error>Invalid OAuth consumer provided.</error>".to_string(),
                        ),
                        true,
                        IOInterface::NORMAL,
                    );
                    self.io.write_error(
                            PhpMixed::String(
                                "You can also add it manually later by using \"composer config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"".to_string(),
                            ),
                            true,
                            IOInterface::NORMAL,
                        );
                    return Ok(false);
                }
                return Err(te.into());
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
        self.token = Some(token_map.into_iter().map(|(k, v)| (k, *v)).collect());

        Ok(true)
    }

    pub fn authorize_oauth_interactively(
        &mut self,
        origin_url: &str,
        message: Option<&str>,
    ) -> anyhow::Result<bool> {
        if let Some(msg) = message {
            self.io
                .write_error(PhpMixed::String(msg.to_string()), true, IOInterface::NORMAL);
        }

        let local_auth_config = self.config.get_local_auth_config_source();
        let url =
            "https://support.atlassian.com/bitbucket-cloud/docs/use-oauth-on-bitbucket-cloud/";
        self.io.write_error(
            PhpMixed::String("Follow the instructions here:".to_string()),
            true,
            IOInterface::NORMAL,
        );
        self.io
            .write_error(PhpMixed::String(url.to_string()), true, IOInterface::NORMAL);
        let auth_config_source_name = self.config.get_auth_config_source().get_name();
        let local_name_prefix = local_auth_config
            .as_ref()
            .map(|c| format!("{} OR ", c.get_name()))
            .unwrap_or_default();
        self.io.write_error(
            PhpMixed::String(format!(
                "to create a consumer. It will be stored in \"{}\" for future use by Composer.",
                local_name_prefix + &auth_config_source_name
            )),
            true,
            IOInterface::NORMAL,
        );
        self.io.write_error(
            PhpMixed::String(
                "Ensure you enter a \"Callback URL\" (http://example.com is fine) or it will not be possible to create an Access Token (this callback url will not be used by composer)".to_string(),
            ),
            true,
            IOInterface::NORMAL,
        );

        let mut store_in_local_auth_config = false;
        if local_auth_config.is_some() {
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
            self.io.write_error(
                PhpMixed::String("<warning>No consumer key given, aborting.</warning>".to_string()),
                true,
                IOInterface::NORMAL,
            );
            self.io.write_error(
                PhpMixed::String(
                    "You can also add it manually later by using \"composer config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"".to_string(),
                ),
                true,
                IOInterface::NORMAL,
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
            self.io.write_error(
                PhpMixed::String(
                    "<warning>No consumer secret given, aborting.</warning>".to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
            self.io.write_error(
                PhpMixed::String(
                    "You can also add it manually later by using \"composer config --global --auth bitbucket-oauth.bitbucket.org <consumer-key> <consumer-secret>\"".to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
            return Ok(false);
        }

        self.io.set_authentication(
            origin_url.to_string(),
            consumer_key.clone(),
            Some(consumer_secret.clone()),
        );

        if !self.request_access_token()? {
            return Ok(false);
        }

        let use_local =
            store_in_local_auth_config && self.config.get_local_auth_config_source().is_some();
        if use_local {
            let mut auth_config_source = self.config.get_local_auth_config_source().unwrap();
            self.store_in_auth_config(
                &mut *auth_config_source,
                origin_url,
                &consumer_key,
                &consumer_secret,
            )?;
        } else {
            let mut auth_config_source = self.config.get_auth_config_source();
            self.store_in_auth_config(
                &mut *auth_config_source,
                origin_url,
                &consumer_key,
                &consumer_secret,
            )?;
        }

        self.config
            .get_auth_config_source()
            .remove_config_setting(&format!("http-basic.{}", origin_url))?;

        self.io.write_error(
            PhpMixed::String("<info>Consumer stored successfully.</info>".to_string()),
            true,
            IOInterface::NORMAL,
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

        self.io.set_authentication(
            origin_url.to_string(),
            consumer_key.to_string(),
            Some(consumer_secret.to_string()),
        );
        if !self.request_access_token()? {
            return Ok(String::new());
        }

        let use_local = self.config.get_local_auth_config_source().is_some();
        if use_local {
            let mut auth_config_source = self.config.get_local_auth_config_source().unwrap();
            self.store_in_auth_config(
                &mut *auth_config_source,
                origin_url,
                consumer_key,
                consumer_secret,
            )?;
        } else {
            let mut auth_config_source = self.config.get_auth_config_source();
            self.store_in_auth_config(
                &mut *auth_config_source,
                origin_url,
                consumer_key,
                consumer_secret,
            )?;
        }

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

    fn store_in_auth_config(
        &mut self,
        auth_config_source: &mut dyn ConfigSourceInterface,
        origin_url: &str,
        consumer_key: &str,
        consumer_secret: &str,
    ) -> anyhow::Result<()> {
        self.config
            .get_config_source()
            .remove_config_setting(&format!("bitbucket-oauth.{}", origin_url))?;

        let token = self.token.as_ref().ok_or_else(|| LogicException {
            message: format!("Expected a token configured with expires_in present, got null",),
            code: 0,
        })?;
        let expires_in = token
            .get("expires_in")
            .and_then(|v| v.as_int())
            .ok_or_else(|| {
                let token_mixed = PhpMixed::Array(
                    token
                        .iter()
                        .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                        .collect(),
                );
                LogicException {
                    message: format!(
                        "Expected a token configured with expires_in present, got {}",
                        shirabe_php_shim::json_encode(&token_mixed).unwrap_or_default()
                    ),
                    code: 0,
                }
            })?;

        let t = self.time.unwrap_or_else(time);
        let mut consumer: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        consumer.insert(
            "consumer-key".to_string(),
            Box::new(PhpMixed::String(consumer_key.to_string())),
        );
        consumer.insert(
            "consumer-secret".to_string(),
            Box::new(PhpMixed::String(consumer_secret.to_string())),
        );
        consumer.insert(
            "access-token".to_string(),
            Box::new(token.get("access_token").cloned().unwrap_or(PhpMixed::Null)),
        );
        consumer.insert(
            "access-token-expiration".to_string(),
            Box::new(PhpMixed::Int(t + expires_in)),
        );

        self.config.get_auth_config_source().add_config_setting(
            &format!("bitbucket-oauth.{}", origin_url),
            PhpMixed::Array(consumer),
        )?;

        Ok(())
    }

    fn get_token_from_config(&mut self, origin_url: &str) -> bool {
        let auth_config = self.config.get("bitbucket-oauth");

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

        let access_token = match origin_config.get("access-token").map(|v| *v.clone()) {
            Some(t) => t,
            None => return false,
        };
        let mut token = IndexMap::new();
        token.insert("access_token".to_string(), access_token);
        self.token = Some(token);

        true
    }
}
