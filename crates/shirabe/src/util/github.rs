//! ref: composer/src/Composer/Util/GitHub.php

use crate::io::io_interface;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{PhpMixed, date, stripos, strtolower};

use crate::config::Config;
use crate::factory::Factory;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::util::HttpDownloader;
use crate::util::ProcessExecutor;

#[derive(Debug)]
pub struct GitHub {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
}

impl GitHub {
    pub const GITHUB_TOKEN_REGEX: &'static str =
        r"{^([a-f0-9]{12,}|gh[a-z]_[a-zA-Z0-9_]+|github_pat_[a-zA-Z0-9_]+)$}";

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

    pub fn authorize_oauth(&mut self, origin_url: &str) -> bool {
        let github_domains = self.config.borrow_mut().get("github-domains");
        let domains = match github_domains.as_array() {
            Some(arr) => arr.clone(),
            None => return false,
        };
        let origin_in_domains = domains.values().any(|v| v.as_string() == Some(origin_url));
        if !origin_in_domains {
            return false;
        }

        let mut output = String::new();
        if self.process.borrow_mut().execute_args(
            &[
                "git".to_string(),
                "config".to_string(),
                "github.accesstoken".to_string(),
            ],
            &mut output,
            None,
        ) == 0
        {
            self.io.borrow_mut().set_authentication(
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
            self.io.write_error3(msg, true, io_interface::NORMAL);
        }

        let mut note = "Composer".to_string();
        let expose_hostname = self
            .config
            .borrow_mut()
            .get("github-expose-hostname")
            .as_bool()
            .unwrap_or(false);
        if expose_hostname {
            let mut output = String::new();
            if self
                .process
                .borrow_mut()
                .execute_args(&["hostname".to_string()], &mut output, None)
                == 0
            {
                note += &format!(" on {}", output.trim());
            }
        }
        note += &format!(" {}", date("Y-m-d Hi", None));

        let (local_name, auth_name): (Option<String>, String) = {
            let cfg = self.config.borrow();
            (
                cfg.get_local_auth_config_source()
                    .map(|c| c.get_name().to_string()),
                cfg.get_auth_config_source().get_name().to_string(),
            )
        };
        let prefix = local_name
            .as_ref()
            .map(|n| format!("{} OR ", n))
            .unwrap_or_default();
        let lines = [
            "You need to provide a GitHub access token.".to_string(),
            format!(
                "Tokens will be stored in plain text in \"{}{}\" for future use by Composer.",
                prefix, auth_name
            ),
            "Due to the security risk of tokens being exfiltrated, use tokens with short expiration times and only the minimum permissions necessary.".to_string(),
            String::new(),
            "Carefully consider the following options in order:".to_string(),
            String::new(),
        ];
        self.io
            .write_error3(&lines.join("\n"), true, io_interface::NORMAL);

        let encoded_note = shirabe_php_shim::rawurlencode(&note).replace("%20", "+");
        let lines = [
            "1. When you don't use 'vcs'  type 'repositories'  in composer.json and do not need to clone source or download dist files".to_string(),
            "from private GitHub repositories over HTTPS, use a fine-grained token with read-only access to public information.".to_string(),
            "Use the following URL to create such a token:".to_string(),
            format!(
                "https://{}/settings/personal-access-tokens/new?name={}",
                origin_url, encoded_note
            ),
            String::new(),
        ];
        self.io
            .write_error3(&lines.join("\n"), true, io_interface::NORMAL);

        let lines = [
            "2. When all relevant _private_ GitHub repositories belong to a single user or organisation, use a fine-grained token with".to_string(),
            "repository \"content\" read-only permissions. You can start with the following URL, but you may need to change the resource owner".to_string(),
            "to the right user or organisation. Additionally, you can scope permissions down to apply only to selected repositories.".to_string(),
            format!(
                "https://{}/settings/personal-access-tokens/new?contents=read&name={}",
                origin_url, encoded_note
            ),
            String::new(),
        ];
        self.io
            .write_error3(&lines.join("\n"), true, io_interface::NORMAL);

        let mut lines3 = vec![
            "3. A \"classic\" token grants broad permissions on your behalf to all repositories accessible by you.".to_string(),
            "This may include write permissions, even though not needed by Composer. Use it only when you need to access".to_string(),
            "private repositories across multiple organisations at the same time and using directory-specific authentication sources".to_string(),
            "is not an option. You can generate a classic token here:".to_string(),
            format!(
                "https://{}/settings/tokens/new?scopes=repo&description={}",
                origin_url, encoded_note
            ),
            String::new(),
        ];
        let _ = &mut lines3;
        self.io
            .write_error3(&lines3.join("\n"), true, io_interface::NORMAL);

        self.io.write_error3(
            "For additional information, check https://getcomposer.org/doc/articles/authentication-for-private-packages.md#github-oauth",
            true,
            io_interface::NORMAL,
        );

        let mut store_in_local_auth_config = false;
        if local_name.is_some() {
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
            self.io.write_error3(
                "<warning>No token given, aborting.</warning>",
                true,
                io_interface::NORMAL,
            );
            self.io.write_error3(
                "You can also add it manually later by using \"composer config --global --auth github-oauth.github.com <token>\"",
                true,
                io_interface::NORMAL,
            );
            return Ok(false);
        }

        self.io.borrow_mut().set_authentication(
            origin_url.to_string(),
            token.clone(),
            Some("x-oauth-basic".to_string()),
        );

        let api_url = if origin_url == "github.com" {
            "api.github.com/".to_string()
        } else {
            format!("{}/api/v3/", origin_url)
        };

        let mut http_options: indexmap::IndexMap<String, PhpMixed> = indexmap::IndexMap::new();
        http_options.insert("retry-auth-failure".to_string(), PhpMixed::Bool(false));

        match self
            .http_downloader
            .borrow_mut()
            .get(&format!("https://{}", api_url), http_options)
        {
            Ok(_) => {}
            Err(te) => {
                let code = te
                    .downcast_ref::<crate::downloader::TransportException>()
                    .and_then(|t| t.get_status_code())
                    .unwrap_or(0);
                if code == 403 || code == 401 {
                    self.io.write_error3(
                        "<error>Invalid token provided.</error>",
                        true,
                        io_interface::NORMAL,
                    );
                    self.io.write_error3(
                        "You can also add it manually later by using \"composer config --global --auth github-oauth.github.com <token>\"",
                        true,
                        io_interface::NORMAL,
                    );
                    return Ok(false);
                }
                return Err(te);
            }
        }

        let use_local = store_in_local_auth_config
            && self
                .config
                .borrow()
                .get_local_auth_config_source()
                .is_some();
        let key = format!("github-oauth.{}", origin_url);
        {
            let mut cfg = self.config.borrow_mut();
            cfg.get_config_source_mut().remove_config_setting(&key)?;
        }
        if use_local {
            let mut cfg = self.config.borrow_mut();
            if let Some(local) = cfg.get_local_auth_config_source_mut() {
                local.add_config_setting(&key, PhpMixed::String(token))?;
            }
        } else {
            let mut cfg = self.config.borrow_mut();
            cfg.get_auth_config_source_mut()
                .add_config_setting(&key, PhpMixed::String(token))?;
        }

        self.io.write_error3(
            "<info>Token stored successfully.</info>",
            true,
            io_interface::NORMAL,
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
            let mut caps: IndexMap<CaptureKey, String> = IndexMap::new();
            if Preg::match3(r"{\burl=(?P<url>[^\s;]+)}", header, Some(&mut caps)) {
                return caps.get(&CaptureKey::ByName("url".to_string())).cloned();
            }
        }

        None
    }

    pub fn is_rate_limited(&self, headers: &[String]) -> bool {
        for header in headers {
            if Preg::is_match(r"{^x-ratelimit-remaining: *0$}i", header.trim()) {
                return true;
            }
        }

        false
    }

    pub fn requires_sso(&self, headers: &[String]) -> bool {
        for header in headers {
            if Preg::is_match(r"{^x-github-sso: required}i", header.trim()) {
                return true;
            }
        }

        false
    }
}
