//! ref: composer/src/Composer/Util/GitHub.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{PhpMixed, date, stripos, strtolower};

use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct GitHub {
    io: Box<dyn IOInterface>,
    config: Config,
    process: ProcessExecutor,
    http_downloader: HttpDownloader,
}

impl GitHub {
    pub const GITHUB_TOKEN_REGEX: &'static str =
        r"{^([a-f0-9]{12,}|gh[a-z]_[a-zA-Z0-9_]+|github_pat_[a-zA-Z0-9_]+)$}";

    pub fn new(
        io: Box<dyn IOInterface>,
        config: Config,
        process: Option<ProcessExecutor>,
        http_downloader: Option<HttpDownloader>,
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
        })
    }

    pub fn authorize_oauth(&mut self, origin_url: &str) -> bool {
        let github_domains = self.config.get("github-domains");
        let domains = match github_domains.as_array() {
            Some(arr) => arr.clone(),
            None => return false,
        };
        let origin_in_domains = domains.values().any(|v| v.as_string() == Some(origin_url));
        if !origin_in_domains {
            return false;
        }

        let mut output = String::new();
        if self.process.execute(
            &[
                "git".to_string(),
                "config".to_string(),
                "github.accesstoken".to_string(),
            ],
            &mut output,
            None,
        ) == 0
        {
            self.io.set_authentication(
                origin_url.to_string(),
                output.trim().to_string(),
                Some("x-oauth-basic".to_string()),
            );
            return true;
        }

        false
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

        let mut note = "Composer".to_string();
        let expose_hostname = self
            .config
            .get("github-expose-hostname")
            .as_bool()
            .unwrap_or(false);
        if expose_hostname {
            let mut output = String::new();
            if self
                .process
                .execute(&["hostname".to_string()], &mut output, None)
                == 0
            {
                note += &format!(" on {}", output.trim());
            }
        }
        note += &format!(" {}", date("Y-m-d Hi", None));

        let local_auth_config = self.config.get_local_auth_config_source();

        self.io.write_error(
            PhpMixed::List(vec![
                Box::new(PhpMixed::String(
                    "You need to provide a GitHub access token.".to_string(),
                )),
                Box::new(PhpMixed::String(format!(
                    "Tokens will be stored in plain text in \"{}\" for future use by Composer.",
                    local_auth_config
                        .as_ref()
                        .map(|c| format!("{} OR ", c.get_name()))
                        .unwrap_or_default()
                        + &self.config.get_auth_config_source().get_name()
                ))),
                Box::new(PhpMixed::String(
                    "Due to the security risk of tokens being exfiltrated, use tokens with short expiration times and only the minimum permissions necessary.".to_string(),
                )),
                Box::new(PhpMixed::String(String::new())),
                Box::new(PhpMixed::String(
                    "Carefully consider the following options in order:".to_string(),
                )),
                Box::new(PhpMixed::String(String::new())),
            ]),
            true,
            IOInterface::NORMAL,
        );

        let encoded_note = shirabe_php_shim::rawurlencode(&note).replace("%20", "+");
        self.io.write_error(
            PhpMixed::List(vec![
                Box::new(PhpMixed::String(
                    "1. When you don't use 'vcs'  type 'repositories'  in composer.json and do not need to clone source or download dist files".to_string(),
                )),
                Box::new(PhpMixed::String(
                    "from private GitHub repositories over HTTPS, use a fine-grained token with read-only access to public information.".to_string(),
                )),
                Box::new(PhpMixed::String(
                    "Use the following URL to create such a token:".to_string(),
                )),
                Box::new(PhpMixed::String(format!(
                    "https://{}/settings/personal-access-tokens/new?name={}",
                    origin_url, encoded_note
                ))),
                Box::new(PhpMixed::String(String::new())),
            ]),
            true,
            IOInterface::NORMAL,
        );

        self.io.write_error(
            PhpMixed::List(vec![
                Box::new(PhpMixed::String(
                    "2. When all relevant _private_ GitHub repositories belong to a single user or organisation, use a fine-grained token with".to_string(),
                )),
                Box::new(PhpMixed::String(
                    "repository \"content\" read-only permissions. You can start with the following URL, but you may need to change the resource owner".to_string(),
                )),
                Box::new(PhpMixed::String(
                    "to the right user or organisation. Additionally, you can scope permissions down to apply only to selected repositories.".to_string(),
                )),
                Box::new(PhpMixed::String(format!(
                    "https://{}/settings/personal-access-tokens/new?contents=read&name={}",
                    origin_url, encoded_note
                ))),
                Box::new(PhpMixed::String(String::new())),
            ]),
            true,
            IOInterface::NORMAL,
        );

        self.io.write_error(
            PhpMixed::List(vec![
                Box::new(PhpMixed::String(
                    "3. A \"classic\" token grants broad permissions on your behalf to all repositories accessible by you.".to_string(),
                )),
                Box::new(PhpMixed::String(
                    "This may include write permissions, even though not needed by Composer. Use it only when you need to access".to_string(),
                )),
                Box::new(PhpMixed::String(
                    "private repositories across multiple organisations at the same time and using directory-specific authentication sources".to_string(),
                )),
                Box::new(PhpMixed::String(
                    "is not an option. You can generate a classic token here:".to_string(),
                )),
                Box::new(PhpMixed::String(format!(
                    "https://{}/settings/tokens/new?scopes=repo&description={}",
                    origin_url, encoded_note
                ))),
                Box::new(PhpMixed::String(String::new())),
            ]),
            true,
            IOInterface::NORMAL,
        );

        self.io.write_error(
            PhpMixed::String(
                "For additional information, check https://getcomposer.org/doc/articles/authentication-for-private-packages.md#github-oauth".to_string(),
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

        let token = self
            .io
            .ask_and_hide_answer("Token (hidden): ".to_string())
            .unwrap_or_default()
            .trim()
            .to_string();

        if token.is_empty() {
            self.io.write_error(
                PhpMixed::String("<warning>No token given, aborting.</warning>".to_string()),
                true,
                IOInterface::NORMAL,
            );
            self.io.write_error(
                PhpMixed::String(
                    "You can also add it manually later by using \"composer config --global --auth github-oauth.github.com <token>\"".to_string(),
                ),
                true,
                IOInterface::NORMAL,
            );
            return Ok(false);
        }

        self.io.set_authentication(
            origin_url.to_string(),
            token.clone(),
            Some("x-oauth-basic".to_string()),
        );

        let api_url = if origin_url == "github.com" {
            "api.github.com/".to_string()
        } else {
            format!("{}/api/v3/", origin_url)
        };

        let mut http_options = indexmap::IndexMap::new();
        http_options.insert(
            "retry-auth-failure".to_string(),
            Box::new(PhpMixed::Bool(false)),
        );
        let http_options = PhpMixed::Array(http_options);

        match self
            .http_downloader
            .get(&format!("https://{}", api_url), &http_options)
        {
            Ok(_) => {}
            Err(te) => {
                if te.code == 403 || te.code == 401 {
                    self.io.write_error(
                        PhpMixed::String("<error>Invalid token provided.</error>".to_string()),
                        true,
                        IOInterface::NORMAL,
                    );
                    self.io.write_error(
                        PhpMixed::String(
                            "You can also add it manually later by using \"composer config --global --auth github-oauth.github.com <token>\"".to_string(),
                        ),
                        true,
                        IOInterface::NORMAL,
                    );
                    return Ok(false);
                }
                return Err(te.into());
            }
        }

        let use_local =
            store_in_local_auth_config && self.config.get_local_auth_config_source().is_some();
        let auth_config_source_name;
        if use_local {
            let mut auth_config_source = self.config.get_local_auth_config_source().unwrap();
            self.config
                .get_config_source()
                .remove_config_setting(&format!("github-oauth.{}", origin_url))?;
            auth_config_source.add_config_setting(
                &format!("github-oauth.{}", origin_url),
                PhpMixed::String(token),
            )?;
        } else {
            let mut auth_config_source = self.config.get_auth_config_source();
            self.config
                .get_config_source()
                .remove_config_setting(&format!("github-oauth.{}", origin_url))?;
            auth_config_source.add_config_setting(
                &format!("github-oauth.{}", origin_url),
                PhpMixed::String(token),
            )?;
        }

        self.io.write_error(
            PhpMixed::String("<info>Token stored successfully.</info>".to_string()),
            true,
            IOInterface::NORMAL,
        );

        Ok(true)
    }

    pub fn get_rate_limit(&self, headers: &[String]) -> indexmap::IndexMap<String, PhpMixed> {
        let mut rate_limit = indexmap::IndexMap::new();
        rate_limit.insert("limit".to_string(), PhpMixed::String("?".to_string()));
        rate_limit.insert("reset".to_string(), PhpMixed::String("?".to_string()));

        for header in headers {
            let header = header.trim();
            if stripos(header, "x-ratelimit-").is_none() {
                continue;
            }
            let parts: Vec<&str> = header.splitn(2, ':').collect();
            if parts.len() < 2 {
                continue;
            }
            let (r#type, value) = (parts[0], parts[1]);
            match strtolower(r#type).as_str() {
                "x-ratelimit-limit" => {
                    let v: i64 = value.trim().parse().unwrap_or(0);
                    rate_limit.insert("limit".to_string(), PhpMixed::Int(v));
                }
                "x-ratelimit-reset" => {
                    let ts: i64 = value.trim().parse().unwrap_or(0);
                    rate_limit.insert(
                        "reset".to_string(),
                        PhpMixed::String(date("Y-m-d H:i:s", Some(ts))),
                    );
                }
                _ => {}
            }
        }

        rate_limit
    }

    pub fn get_sso_url(&self, headers: &[String]) -> Option<String> {
        for header in headers {
            let header = header.trim();
            if stripos(header, "x-github-sso: required").is_none() {
                continue;
            }
            if let Some(caps) = Preg::match_strict_groups(r"{\burl=(?P<url>[^\s;]+)}", header) {
                return caps.get("url").cloned();
            }
        }

        None
    }

    pub fn is_rate_limited(&self, headers: &[String]) -> bool {
        for header in headers {
            if Preg::is_match(r"{^x-ratelimit-remaining: *0$}i", header.trim()).unwrap_or(false) {
                return true;
            }
        }

        false
    }

    pub fn requires_sso(&self, headers: &[String]) -> bool {
        for header in headers {
            if Preg::is_match(r"{^x-github-sso: required}i", header.trim()).unwrap_or(false) {
                return true;
            }
        }

        false
    }
}
