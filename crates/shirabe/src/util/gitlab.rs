//! ref: composer/src/Composer/Util/GitLab.php

use crate::io::io_interface;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{PhpMixed, RuntimeException, http_build_query, json_decode, time};

use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct GitLab {
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) config: std::rc::Rc<std::cell::RefCell<Config>>,
    pub(crate) process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    pub(crate) http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
}

impl GitLab {
    pub fn new(
        io: Box<dyn IOInterface>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
        http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
    ) -> anyhow::Result<Self> {
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(&*io)))
        });
        let http_downloader = match http_downloader {
            Some(h) => h,
            None => std::rc::Rc::new(std::cell::RefCell::new(Factory::create_http_downloader(
                &*io,
                &config,
                IndexMap::new(),
            )?)),
        };
        Ok(Self {
            io,
            config,
            process,
            http_downloader,
        })
    }

    pub fn authorize_oauth(&mut self, origin_url: &str) -> bool {
        // before composer 1.9, origin URLs had no port number in them
        let bc_origin_url =
            Preg::replace("{:\\d+}", "", origin_url).unwrap_or_else(|_| origin_url.to_string());

        let gitlab_domains = self.config.borrow_mut().get("gitlab-domains");
        let domains = match gitlab_domains.as_array() {
            Some(arr) => arr.clone(),
            None => return false,
        };
        let origin_in_domains = domains.values().any(|v| v.as_string() == Some(origin_url));
        let bc_in_domains = domains
            .values()
            .any(|v| v.as_string() == Some(bc_origin_url.as_str()));
        if !origin_in_domains && !bc_in_domains {
            return false;
        }

        // if available use token from git config
        let mut output = String::new();
        if self.process.borrow_mut().execute_args(
            &[
                "git".to_string(),
                "config".to_string(),
                "gitlab.accesstoken".to_string(),
            ],
            &mut output,
            (),
        ) == 0
        {
            self.io.set_authentication(
                origin_url.to_string(),
                output.trim().to_string(),
                Some("oauth2".to_string()),
            );
            return true;
        }

        // if available use deploy token from git config
        let mut token_user = String::new();
        let mut token_password = String::new();
        if self.process.borrow_mut().execute_args(
            &[
                "git".to_string(),
                "config".to_string(),
                "gitlab.deploytoken.user".to_string(),
            ],
            &mut token_user,
            (),
        ) == 0
            && self.process.borrow_mut().execute_args(
                &[
                    "git".to_string(),
                    "config".to_string(),
                    "gitlab.deploytoken.token".to_string(),
                ],
                &mut token_password,
                (),
            ) == 0
        {
            self.io.set_authentication(
                origin_url.to_string(),
                token_user.trim().to_string(),
                Some(token_password.trim().to_string()),
            );
            return true;
        }

        // if available use token from composer config
        let auth_tokens = self.config.borrow_mut().get("gitlab-token");

        let mut token: Option<PhpMixed> = None;

        if let Some(map) = auth_tokens.as_array() {
            if let Some(t) = map.get(origin_url) {
                token = Some(*t.clone());
            }
            if let Some(t) = map.get(bc_origin_url.as_str()) {
                token = Some(*t.clone());
            }
        }

        if let Some(token) = token {
            let (username, password) = match &token {
                PhpMixed::Array(arr) => {
                    let username = arr
                        .get("username")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    let password = arr
                        .get("token")
                        .and_then(|v| v.as_string())
                        .unwrap_or("private-token")
                        .to_string();
                    (username, password)
                }
                _ => {
                    let username = token.as_string().unwrap_or("").to_string();
                    let password = "private-token".to_string();
                    (username, password)
                }
            };

            // Composer expects the GitLab token to be stored as username and 'private-token' or
            // 'gitlab-ci-token' to be stored as password. Detect cases where this is reversed
            // and automatically resolve it.
            if ["private-token", "gitlab-ci-token", "oauth2"].contains(&username.as_str()) {
                self.io
                    .set_authentication(origin_url.to_string(), password, Some(username));
            } else {
                self.io
                    .set_authentication(origin_url.to_string(), username, Some(password));
            }

            return true;
        }

        false
    }

    pub fn authorize_oauth_interactively(
        &mut self,
        scheme: &str,
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
        let personal_access_token_link = format!(
            "{}://{}/-/user_settings/personal_access_tokens",
            scheme, origin_url
        );
        let revoke_link = format!("{}://{}/-/user_settings/applications", scheme, origin_url);
        self.io.write_error3(
            &format!(
                "A token will be created and stored in \"{}\", your password will never be stored",
                local_auth_config_name
                    .as_ref()
                    .map(|name| format!("{} OR ", name))
                    .unwrap_or_default()
                    + &self.config.borrow().get_auth_config_source().get_name()
            ),
            true,
            io_interface::NORMAL,
        );
        self.io.write_error3(
            "To revoke access to this token you can visit:",
            true,
            io_interface::NORMAL,
        );
        self.io
            .write_error3(&revoke_link, true, io_interface::NORMAL);
        self.io.write_error3(
            "Alternatively you can setup an personal access token on:",
            true,
            io_interface::NORMAL,
        );
        self.io
            .write_error3(&personal_access_token_link, true, io_interface::NORMAL);
        self.io.write_error3(
            "and store it under \"gitlab-token\" see https://getcomposer.org/doc/articles/authentication-for-private-packages.md#gitlab-token for more details.",
            true,
            io_interface::NORMAL,
        );
        self.io.write_error3(
            "https://getcomposer.org/doc/articles/authentication-for-private-packages.md#gitlab-token",
            true,
            io_interface::NORMAL,
        );
        self.io
            .write_error3("for more details.", true, io_interface::NORMAL);

        let mut store_in_local_auth_config = false;
        if has_local_auth_config {
            store_in_local_auth_config = self.io.ask_confirmation(
                "A local auth config source was found, do you want to store the token there?"
                    .to_string(),
                true,
            );
        }

        let mut attempt_counter = 0;

        while attempt_counter < 5 {
            attempt_counter += 1;
            let response = match self.create_token(scheme, origin_url) {
                Ok(r) => r,
                Err(e) => {
                    // 401 is bad credentials,
                    // 403 is max login attempts exceeded
                    match e.downcast::<TransportException>() {
                        Ok(te) if te.code == 403 || te.code == 401 => {
                            if te.code == 401 {
                                let response =
                                    te.get_response().and_then(|r| json_decode(r, true).ok());
                                let is_invalid_grant = response
                                    .as_ref()
                                    .and_then(|r| r.as_array())
                                    .and_then(|arr| arr.get("error"))
                                    .and_then(|v| v.as_string())
                                    == Some("invalid_grant");
                                if is_invalid_grant {
                                    self.io.write_error3(
                                        "Bad credentials. If you have two factor authentication enabled you will have to manually create a personal access token",
                                        true,
                                        io_interface::NORMAL,
                                    );
                                } else {
                                    self.io.write_error3(
                                        "Bad credentials.",
                                        true,
                                        io_interface::NORMAL,
                                    );
                                }
                            } else {
                                self.io.write_error3(
                                    "Maximum number of login attempts exceeded. Please try again later.",
                                    true,
                                    io_interface::NORMAL,
                                );
                            }

                            self.io.write_error3(
                                "You can also manually create a personal access token enabling the \"read_api\" scope at:",
                                true,
                                io_interface::NORMAL,
                            );
                            self.io.write_error3(
                                &personal_access_token_link,
                                true,
                                io_interface::NORMAL,
                            );
                            self.io.write_error3(
                                &format!(
                                    "Add it using \"composer config --global --auth gitlab-token.{} <token>\"",
                                    origin_url
                                ),
                                true,
                                io_interface::NORMAL,
                            );

                            continue;
                        }
                        Ok(te) => return Err(te.into()),
                        Err(e) => return Err(e),
                    }
                }
            };

            let access_token = response
                .as_array()
                .and_then(|arr| arr.get("access_token"))
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();

            self.io.set_authentication(
                origin_url.to_string(),
                access_token.clone(),
                Some("oauth2".to_string()),
            );

            // store value in user config in auth file
            let use_local = store_in_local_auth_config && has_local_auth_config;
            let has_expires_in = response
                .as_array()
                .map(|arr| arr.contains_key("expires_in"))
                .unwrap_or(false);

            if use_local {
                let mut config = self.config.borrow_mut();
                let auth_config_source = config.get_local_auth_config_source_mut().unwrap();
                if has_expires_in {
                    auth_config_source.add_config_setting(
                        &format!("gitlab-oauth.{}", origin_url),
                        Self::build_oauth_config(&response, &access_token),
                    )?;
                } else {
                    auth_config_source.add_config_setting(
                        &format!("gitlab-oauth.{}", origin_url),
                        PhpMixed::String(access_token),
                    )?;
                }
            } else {
                let mut config = self.config.borrow_mut();
                let auth_config_source = config.get_auth_config_source_mut();
                if has_expires_in {
                    auth_config_source.add_config_setting(
                        &format!("gitlab-oauth.{}", origin_url),
                        Self::build_oauth_config(&response, &access_token),
                    )?;
                } else {
                    auth_config_source.add_config_setting(
                        &format!("gitlab-oauth.{}", origin_url),
                        PhpMixed::String(access_token),
                    )?;
                }
            }

            return Ok(true);
        }

        Err(RuntimeException {
            message: "Invalid GitLab credentials 5 times in a row, aborting.".to_string(),
            code: 0,
        }
        .into())
    }

    pub fn authorize_oauth_refresh(
        &mut self,
        scheme: &str,
        origin_url: &str,
    ) -> anyhow::Result<bool> {
        let response = match self.refresh_token(scheme, origin_url) {
            Ok(r) => r,
            Err(e) => match e.downcast::<TransportException>() {
                Ok(te) => {
                    self.io.write_error3(
                        &format!("Couldn't refresh access token: {}", te.message),
                        true,
                        io_interface::NORMAL,
                    );
                    return Ok(false);
                }
                Err(e) => return Err(e),
            },
        };

        let access_token = response
            .as_array()
            .and_then(|arr| arr.get("access_token"))
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();

        self.io.set_authentication(
            origin_url.to_string(),
            access_token.clone(),
            Some("oauth2".to_string()),
        );

        // store value in user config in auth file
        self.config
            .borrow_mut()
            .get_auth_config_source_mut()
            .add_config_setting(
                &format!("gitlab-oauth.{}", origin_url),
                Self::build_oauth_config(&response, &access_token),
            )?;

        Ok(true)
    }

    fn create_token(&mut self, scheme: &str, origin_url: &str) -> anyhow::Result<PhpMixed> {
        let username = match self.io.ask("Username: ".to_string(), PhpMixed::Null) {
            PhpMixed::String(s) => s,
            _ => String::new(),
        };
        let password = self
            .io
            .ask_and_hide_answer("Password: ".to_string())
            .unwrap_or_default();

        let headers = vec!["Content-Type: application/x-www-form-urlencoded".to_string()];

        let api_url = origin_url;
        let data = http_build_query(
            &[
                ("username", username.as_str()),
                ("password", password.as_str()),
                ("grant_type", "password"),
            ],
            "",
            "&",
        );
        let mut http_inner: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        http_inner.insert(
            "method".to_string(),
            Box::new(PhpMixed::String("POST".to_string())),
        );
        http_inner.insert(
            "header".to_string(),
            Box::new(PhpMixed::List(
                headers
                    .into_iter()
                    .map(|h| Box::new(PhpMixed::String(h)))
                    .collect(),
            )),
        );
        http_inner.insert("content".to_string(), Box::new(PhpMixed::String(data)));
        let mut options: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        options.insert(
            "retry-auth-failure".to_string(),
            Box::new(PhpMixed::Bool(false)),
        );
        options.insert("http".to_string(), Box::new(PhpMixed::Array(http_inner)));

        let token = self
            .http_downloader
            .borrow_mut()
            .get(
                &format!("{}://{}/oauth/token", scheme, api_url),
                options.into_iter().map(|(k, v)| (k, *v)).collect(),
            )?
            .decode_json()?;

        self.io
            .write_error3("Token successfully created", true, io_interface::NORMAL);

        Ok(token)
    }

    pub fn is_oauth_expired(&self, origin_url: &str) -> bool {
        let auth_tokens = self.config.borrow_mut().get("gitlab-oauth");
        if let Some(map) = auth_tokens.as_array() {
            if let Some(token_info) = map.get(origin_url) {
                if let Some(token_map) = token_info.as_array() {
                    if let Some(expires_at) = token_map.get("expires-at") {
                        if let Some(expires_at_int) = expires_at.as_int() {
                            if expires_at_int < time() {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        false
    }

    fn refresh_token(&mut self, scheme: &str, origin_url: &str) -> anyhow::Result<PhpMixed> {
        let auth_tokens = self.config.borrow_mut().get("gitlab-oauth");
        let refresh_token = auth_tokens
            .as_array()
            .and_then(|map| map.get(origin_url))
            .and_then(|v| v.as_array())
            .and_then(|token_map| token_map.get("refresh-token"))
            .and_then(|v| v.as_string())
            .map(|s| s.to_string());

        let refresh_token = match refresh_token {
            Some(t) => t,
            None => {
                return Err(RuntimeException {
                    message: format!("No GitLab refresh token present for {}.", origin_url),
                    code: 0,
                }
                .into());
            }
        };

        let headers = vec!["Content-Type: application/x-www-form-urlencoded".to_string()];

        let data = http_build_query(
            &[
                ("refresh_token", refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ],
            "",
            "&",
        );
        let mut http_inner: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        http_inner.insert(
            "method".to_string(),
            Box::new(PhpMixed::String("POST".to_string())),
        );
        http_inner.insert(
            "header".to_string(),
            Box::new(PhpMixed::List(
                headers
                    .into_iter()
                    .map(|h| Box::new(PhpMixed::String(h)))
                    .collect(),
            )),
        );
        http_inner.insert("content".to_string(), Box::new(PhpMixed::String(data)));
        let mut options: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        options.insert(
            "retry-auth-failure".to_string(),
            Box::new(PhpMixed::Bool(false)),
        );
        options.insert("http".to_string(), Box::new(PhpMixed::Array(http_inner)));

        let token = self
            .http_downloader
            .borrow_mut()
            .get(
                &format!("{}://{}/oauth/token", scheme, origin_url),
                options.into_iter().map(|(k, v)| (k, *v)).collect(),
            )?
            .decode_json()?;

        self.io.write_error3(
            "GitLab token successfully refreshed",
            true,
            io_interface::VERY_VERBOSE,
        );
        self.io.write_error3(
            &format!(
                "To revoke access to this token you can visit {}://{}/-/user_settings/applications",
                scheme, origin_url
            ),
            true,
            io_interface::VERY_VERBOSE,
        );

        Ok(token)
    }

    fn build_oauth_config(response: &PhpMixed, access_token: &str) -> PhpMixed {
        let created_at = response
            .as_array()
            .and_then(|arr| arr.get("created_at"))
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        let expires_in = response
            .as_array()
            .and_then(|arr| arr.get("expires_in"))
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        let refresh_token = response
            .as_array()
            .and_then(|arr| arr.get("refresh_token"))
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();
        let mut setting: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        setting.insert(
            "expires-at".to_string(),
            Box::new(PhpMixed::Int(created_at + expires_in)),
        );
        setting.insert(
            "refresh-token".to_string(),
            Box::new(PhpMixed::String(refresh_token)),
        );
        setting.insert(
            "token".to_string(),
            Box::new(PhpMixed::String(access_token.to_string())),
        );
        PhpMixed::Array(setting)
    }
}
