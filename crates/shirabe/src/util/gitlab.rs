//! ref: composer/src/Composer/Util/GitLab.php

use crate::config::Config;
use crate::downloader::TransportException;
use crate::factory::Factory;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::util::{HttpDownloader, ProcessExecutor};
use bstr::{B, BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, PhpMixed, RuntimeException, bconcat, http_build_query, in_array_strict,
    json_decode_assoc, legacy_lossy, php_regex, preg_replace, time, trim,
};

#[derive(Debug)]
pub struct GitLab {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
}

impl GitLab {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
        http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
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
        })
    }

    pub fn authorize_oauth(&mut self, origin_url: &BStr) -> anyhow::Result<bool> {
        // before composer 1.9, origin URLs had no port number in them
        let bc_origin_url = preg_replace(php_regex!("{:\\d+}"), "", origin_url);

        let gitlab_domains = self.config.borrow_mut().get("gitlab-domains");
        if !in_array_strict(PhpMixed::string(origin_url), gitlab_domains.values())
            && !in_array_strict(
                PhpMixed::string(bc_origin_url.clone()),
                gitlab_domains.values(),
            )
        {
            return Ok(false);
        }

        // if available use token from git config
        let mut output = BString::default();
        if self.process.borrow_mut().execute(
            &[
                "git".to_string(),
                "config".to_string(),
                "gitlab.accesstoken".to_string(),
            ],
            &mut output,
            None,
        )? == 0
        {
            self.io.borrow_mut().set_authentication(
                origin_url.into(),
                trim(&output),
                Some(BString::from("oauth2")),
            );
            return Ok(true);
        }

        // if available use deploy token from git config
        let mut token_user = BString::default();
        let mut token_password = BString::default();
        if self.process.borrow_mut().execute(
            &[
                "git".to_string(),
                "config".to_string(),
                "gitlab.deploytoken.user".to_string(),
            ],
            &mut token_user,
            None,
        )? == 0
            && self.process.borrow_mut().execute(
                &[
                    "git".to_string(),
                    "config".to_string(),
                    "gitlab.deploytoken.token".to_string(),
                ],
                &mut token_password,
                None,
            )? == 0
        {
            self.io.borrow_mut().set_authentication(
                origin_url.into(),
                trim(&token_user),
                Some(trim(&token_password)),
            );
            return Ok(true);
        }

        // if available use token from composer config
        let auth_tokens = self.config.borrow_mut().get("gitlab-token");

        let mut token: Option<PhpMixed> = None;

        if let Some(map) = auth_tokens.as_array() {
            if let Some(t) = map.get(origin_url) {
                token = Some(t.clone());
            }
            if let Some(t) = map.get(&bc_origin_url) {
                token = Some(t.clone());
            }
        }

        if let Some(token) = token {
            let (username, password) = match &token {
                PhpMixed::Array(arr) => {
                    let username = BString::from(
                        arr.get(B("username"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                    );
                    let password = BString::from(
                        arr.get(B("token"))
                            .and_then(|v| v.as_string())
                            .unwrap_or("private-token".into()),
                    );
                    (username, password)
                }
                _ => {
                    let username = BString::from(token.as_string().unwrap_or_default());
                    let password = BString::from("private-token");
                    (username, password)
                }
            };

            // Composer expects the GitLab token to be stored as username and 'private-token' or
            // 'gitlab-ci-token' to be stored as password. Detect cases where this is reversed
            // and automatically resolve it.
            if [B("private-token"), B("gitlab-ci-token"), B("oauth2")]
                .contains(&username.as_slice())
            {
                self.io.borrow_mut().set_authentication(
                    origin_url.into(),
                    password,
                    Some(username),
                );
            } else {
                self.io.borrow_mut().set_authentication(
                    origin_url.into(),
                    username,
                    Some(password),
                );
            }

            return Ok(true);
        }

        Ok(false)
    }

    pub fn authorize_oauth_interactively(
        &mut self,
        scheme: &BStr,
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
        let personal_access_token_link = format!(
            "{}://{}/-/user_settings/personal_access_tokens",
            scheme.display_lossy(),
            origin_url.display_lossy()
        );
        let revoke_link = format!(
            "{}://{}/-/user_settings/applications",
            scheme.display_lossy(),
            origin_url.display_lossy()
        );
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
            )?;
        }

        let mut attempt_counter = 0;

        while attempt_counter < 5 {
            attempt_counter += 1;
            let response = match self.create_token(scheme, origin_url) {
                Ok(r) => r,
                Err(e) => {
                    // 401 is bad credentials,
                    // 403 is max login attempts exceeded
                    match e.catch::<TransportException>() {
                        Some(te) if te.get_code() == 403 || te.get_code() == 401 => {
                            if te.get_code() == 401 {
                                let response =
                                    te.get_response().and_then(|r| json_decode_assoc(r).ok());
                                let is_invalid_grant = response
                                    .as_ref()
                                    .and_then(|r| r.as_array())
                                    .and_then(|arr| arr.get(B("error")))
                                    .and_then(|v| v.as_string())
                                    == Some("invalid_grant".into());
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
                                    "Add it using \"shirabe config --global --auth gitlab-token.{} <token>\"",
                                    origin_url.display_lossy()
                                ),
                                true,
                                io_interface::NORMAL,
                            );

                            continue;
                        }
                        _ => return Err(e),
                    }
                }
            };

            let access_token = BString::from(
                response
                    .as_array()
                    .and_then(|arr| arr.get(B("access_token")))
                    .and_then(|v| v.as_string())
                    .unwrap_or_default(),
            );

            self.io.borrow_mut().set_authentication(
                origin_url.into(),
                access_token.clone(),
                Some(BString::from("oauth2")),
            );

            // store value in user config in auth file
            let use_local = store_in_local_auth_config && has_local_auth_config;
            let has_expires_in = response
                .as_array()
                .map(|arr| arr.contains_key(B("expires_in")))
                .unwrap_or(false);

            if use_local {
                let mut config = self.config.borrow_mut();
                let auth_config_source = config.get_local_auth_config_source_mut().unwrap();
                if has_expires_in {
                    auth_config_source.add_config_setting(
                        &legacy_lossy(&bconcat!("gitlab-oauth.", origin_url)),
                        Self::build_oauth_config(&response, access_token.as_ref()),
                    )?;
                } else {
                    auth_config_source.add_config_setting(
                        &legacy_lossy(&bconcat!("gitlab-oauth.", origin_url)),
                        PhpMixed::string(access_token),
                    )?;
                }
            } else {
                let mut config = self.config.borrow_mut();
                let auth_config_source = config.get_auth_config_source_mut();
                if has_expires_in {
                    auth_config_source.add_config_setting(
                        &legacy_lossy(&bconcat!("gitlab-oauth.", origin_url)),
                        Self::build_oauth_config(&response, access_token.as_ref()),
                    )?;
                } else {
                    auth_config_source.add_config_setting(
                        &legacy_lossy(&bconcat!("gitlab-oauth.", origin_url)),
                        PhpMixed::string(access_token),
                    )?;
                }
            }

            return Ok(true);
        }

        Err(RuntimeException::new(
            "Invalid GitLab credentials 5 times in a row, aborting.".to_string(),
        )
        .into())
    }

    pub fn authorize_oauth_refresh(
        &mut self,
        scheme: &BStr,
        origin_url: &BStr,
    ) -> anyhow::Result<bool> {
        let response = match self.refresh_token(scheme, origin_url) {
            Ok(r) => r,
            Err(e) => match e.catch::<TransportException>() {
                Some(te) => {
                    let message = te.get_message().to_string();
                    self.io.write_error3(
                        &format!("Couldn't refresh access token: {}", message),
                        true,
                        io_interface::NORMAL,
                    );
                    return Ok(false);
                }
                None => return Err(e),
            },
        };

        let access_token = BString::from(
            response
                .as_array()
                .and_then(|arr| arr.get(B("access_token")))
                .and_then(|v| v.as_string())
                .unwrap_or_default(),
        );

        self.io.borrow_mut().set_authentication(
            origin_url.into(),
            access_token.clone(),
            Some(BString::from("oauth2")),
        );

        // store value in user config in auth file
        self.config
            .borrow_mut()
            .get_auth_config_source_mut()
            .add_config_setting(
                &legacy_lossy(&bconcat!("gitlab-oauth.", origin_url)),
                Self::build_oauth_config(&response, access_token.as_ref()),
            )?;

        Ok(true)
    }

    fn create_token(&mut self, scheme: &BStr, origin_url: &BStr) -> anyhow::Result<PhpMixed> {
        let username = match self.io.ask("Username: ".into(), PhpMixed::Null)? {
            PhpMixed::String(s) => s,
            _ => BString::default(),
        };
        let password = self
            .io
            .ask_and_hide_answer("Password: ".to_string())?
            .unwrap_or_default();

        let headers = vec!["Content-Type: application/x-www-form-urlencoded".to_string()];

        let api_url = origin_url;
        let data = http_build_query(&[
            (B("username"), username.as_slice()),
            (B("password"), password.as_bytes()),
            (B("grant_type"), B("password")),
        ]);
        let mut http_inner: IndexMap<BString, PhpMixed> = IndexMap::new();
        http_inner.insert("method".into(), PhpMixed::string("POST".to_string()));
        http_inner.insert(
            "header".into(),
            PhpMixed::List(headers.into_iter().map(PhpMixed::string).collect()),
        );
        http_inner.insert("content".into(), PhpMixed::string(data));
        let mut options: IndexMap<BString, PhpMixed> = IndexMap::new();
        options.insert("retry-auth-failure".into(), PhpMixed::Bool(false));
        options.insert("http".into(), PhpMixed::Array(http_inner));

        let token = self
            .http_downloader
            .borrow_mut()
            .get(
                &legacy_lossy(&bconcat!(scheme, "://", api_url, "/oauth/token")),
                options.into_iter().collect(),
            )?
            .decode_json()?;

        self.io
            .write_error3("Token successfully created", true, io_interface::NORMAL);

        Ok(token)
    }

    pub fn is_oauth_expired(&self, origin_url: &BStr) -> bool {
        let auth_tokens = self.config.borrow_mut().get("gitlab-oauth");
        if let Some(map) = auth_tokens.as_array()
            && let Some(token_info) = map.get(origin_url)
            && let Some(token_map) = token_info.as_array()
            && let Some(expires_at) = token_map.get(B("expires-at"))
            && let Some(expires_at_int) = expires_at.as_int()
            && expires_at_int < time()
        {
            return true;
        }

        false
    }

    fn refresh_token(&mut self, scheme: &BStr, origin_url: &BStr) -> anyhow::Result<PhpMixed> {
        let auth_tokens = self.config.borrow_mut().get("gitlab-oauth");
        let refresh_token = auth_tokens
            .as_array()
            .and_then(|map| map.get(origin_url))
            .and_then(|v| v.as_array())
            .and_then(|token_map| token_map.get(B("refresh-token")))
            .and_then(|v| v.as_string())
            .map(BString::from);

        let refresh_token = match refresh_token {
            Some(t) => t,
            None => {
                return Err(RuntimeException::new(format!(
                    "No GitLab refresh token present for {}.",
                    origin_url.display_lossy()
                ))
                .into());
            }
        };

        let headers = vec!["Content-Type: application/x-www-form-urlencoded".to_string()];

        let data = http_build_query(&[
            (B("refresh_token"), refresh_token.as_slice()),
            (B("grant_type"), B("refresh_token")),
        ]);
        let mut http_inner = IndexMap::new();
        http_inner.insert("method".into(), PhpMixed::string("POST".to_string()));
        http_inner.insert(
            "header".into(),
            PhpMixed::List(headers.into_iter().map(PhpMixed::string).collect()),
        );
        http_inner.insert("content".into(), PhpMixed::string(data));
        let mut options: IndexMap<BString, PhpMixed> = IndexMap::new();
        options.insert("retry-auth-failure".into(), PhpMixed::Bool(false));
        options.insert("http".into(), PhpMixed::Array(http_inner));

        let token = self
            .http_downloader
            .borrow_mut()
            .get(
                &legacy_lossy(&bconcat!(scheme, "://", origin_url, "/oauth/token")),
                options.into_iter().collect(),
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
                scheme.display_lossy(),
                origin_url.display_lossy()
            ),
            true,
            io_interface::VERY_VERBOSE,
        );

        Ok(token)
    }

    fn build_oauth_config(response: &PhpMixed, access_token: &BStr) -> PhpMixed {
        let created_at = response
            .as_array()
            .and_then(|arr| arr.get(B("created_at")))
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        let expires_in = response
            .as_array()
            .and_then(|arr| arr.get(B("expires_in")))
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        let refresh_token = BString::from(
            response
                .as_array()
                .and_then(|arr| arr.get(B("refresh_token")))
                .and_then(|v| v.as_string())
                .unwrap_or_default(),
        );
        let mut setting = IndexMap::new();
        setting.insert("expires-at".into(), PhpMixed::Int(created_at + expires_in));
        setting.insert("refresh-token".into(), PhpMixed::string(refresh_token));
        setting.insert("token".into(), PhpMixed::string(access_token));
        PhpMixed::Array(setting)
    }
}
