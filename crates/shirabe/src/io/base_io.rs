//! ref: composer/src/Composer/IO/BaseIO.php

use crate::config::Config;
use crate::io::{IOInterface, IOInterfaceWrite, io_interface};
use crate::util::ProcessExecutor;
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    JSON_INVALID_UTF8_IGNORE, JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE, PhpMixed,
    UnexpectedValueException, array_merge, bconcat, in_array_strict, json_encode_ex, php_regex,
    php_str, preg_is_match,
};

/// ref: composer/vendor/psr/log/Psr/Log/LogLevel.php
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLevel;

impl LogLevel {
    pub const EMERGENCY: &'static BStr = php_str(b"emergency");
    pub const ALERT: &'static BStr = php_str(b"alert");
    pub const CRITICAL: &'static BStr = php_str(b"critical");
    pub const ERROR: &'static BStr = php_str(b"error");
    pub const WARNING: &'static BStr = php_str(b"warning");
    pub const NOTICE: &'static BStr = php_str(b"notice");
    pub const INFO: &'static BStr = php_str(b"info");
    pub const DEBUG: &'static BStr = php_str(b"debug");
}

fn log_context(context: &[(&BStr, &BStr)]) -> IndexMap<BString, PhpMixed> {
    context
        .iter()
        .map(|(k, v)| (BString::from(*k), PhpMixed::string(*v)))
        .collect()
}

pub trait BaseIO: IOInterface {
    fn authentications(&self) -> &IndexMap<BString, IndexMap<BString, Option<BString>>>;
    fn authentications_mut(&mut self)
    -> &mut IndexMap<BString, IndexMap<BString, Option<BString>>>;

    fn get_authentications(&self) -> IndexMap<BString, IndexMap<BString, Option<BString>>> {
        self.authentications().clone()
    }

    fn reset_authentications(&mut self) {
        *self.authentications_mut() = IndexMap::new();
    }

    fn has_authentication(&self, repository_name: &BStr) -> bool {
        self.authentications().contains_key(repository_name)
    }

    fn get_authentication(&self, repository_name: &BStr) -> IndexMap<BString, Option<BString>> {
        if let Some(auth) = self.authentications().get(repository_name) {
            return auth.clone();
        }
        let mut result = IndexMap::new();
        result.insert(BString::from("username"), None);
        result.insert(BString::from("password"), None);
        result
    }

    fn set_authentication(
        &mut self,
        repository_name: BString,
        username: BString,
        password: Option<BString>,
    ) {
        let mut auth = IndexMap::new();
        auth.insert(BString::from("username"), Some(username));
        auth.insert(BString::from("password"), password);
        self.authentications_mut().insert(repository_name, auth);
    }

    fn check_and_set_authentication(
        &mut self,
        repository_name: BString,
        username: BString,
        password: Option<BString>,
    ) {
        if BaseIO::has_authentication(self, repository_name.as_ref()) {
            let auth = BaseIO::get_authentication(self, repository_name.as_ref());
            if *auth.get(bstr::B("username")).unwrap_or(&None) == Some(username.clone())
                && *auth.get(bstr::B("password")).unwrap_or(&None) == password
            {
                return;
            }
            self.write_error3(
                bconcat!("<warning>Warning: You should avoid overwriting already defined auth settings for ", repository_name, ".</warning>"),
                true,
                io_interface::NORMAL,
            );
        }
        BaseIO::set_authentication(self, repository_name, username, password);
    }

    fn load_configuration(&mut self, config: &mut Config) -> anyhow::Result<()> {
        let bitbucket_oauth = config.get("bitbucket-oauth");
        let github_oauth = config.get("github-oauth");
        let gitlab_oauth = config.get("gitlab-oauth");
        let gitlab_token = config.get("gitlab-token");
        let forgejo_token = config.get("forgejo-token");
        let http_basic = config.get("http-basic");
        let bearer_token = config.get("bearer");
        let custom_headers = config.get("custom-headers");
        let client_certificate = config.get("client-certificate");

        if let Some(map) = bitbucket_oauth.as_opt().and_then(|v| v.as_array()) {
            for (domain, cred) in map.clone() {
                if let Some(cred_map) = cred.as_array() {
                    let consumer_key = BString::from(
                        cred_map
                            .get(B("consumer-key"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                    );
                    let consumer_secret = cred_map
                        .get(B("consumer-secret"))
                        .and_then(|v| v.as_string())
                        .map(BString::from);
                    self.check_and_set_authentication(domain, consumer_key, consumer_secret);
                }
            }
        }

        if let Some(map) = github_oauth.as_opt().and_then(|v| v.as_array()) {
            for (domain, token) in map.clone() {
                let token_str = BString::from(token.as_string().unwrap_or_default());
                let github_domains = config.get("github-domains");
                if domain != "github.com"
                    && !in_array_strict(
                        PhpMixed::string(domain.clone()),
                        github_domains
                            .clone()
                            .unwrap_or(PhpMixed::List(vec![]))
                            .values(),
                    )
                {
                    <Self as BaseIO>::debug(
                        self,
                        BStr::new(&bconcat!(
                            domain,
                            " is not in the configured github-domains, adding it implicitly as authentication is configured for this domain"
                        )),
                        &[],
                    );
                    let merged = array_merge(
                        github_domains.unwrap_or(PhpMixed::List(vec![])),
                        PhpMixed::List(vec![PhpMixed::string(domain.clone())]),
                    );
                    let mut inner = IndexMap::new();
                    inner.insert("github-domains".into(), merged);
                    let mut config_outer = IndexMap::new();
                    config_outer.insert("config".into(), PhpMixed::Array(inner));
                    config.merge(&config_outer, "implicit-due-to-auth");
                }

                if !preg_is_match(php_regex!(r"{^[.A-Za-z0-9_]+$}"), token_str.as_bstr()) {
                    return Err(UnexpectedValueException::new(bconcat!(
                        "Your github oauth token for ",
                        domain,
                        " contains invalid characters: \"",
                        token_str,
                        "\""
                    ))
                    .into());
                }
                self.check_and_set_authentication(
                    domain,
                    token_str,
                    Some(BString::from("x-oauth-basic")),
                );
            }
        }

        if let Some(map) = gitlab_oauth.as_opt().and_then(|v| v.as_array()) {
            for (domain, token) in map.clone() {
                let gitlab_domains = config.get("gitlab-domains");
                if domain != "gitlab.com"
                    && !in_array_strict(
                        PhpMixed::string(domain.clone()),
                        gitlab_domains
                            .clone()
                            .unwrap_or(PhpMixed::List(vec![]))
                            .values(),
                    )
                {
                    <Self as BaseIO>::debug(
                        self,
                        BStr::new(&bconcat!(
                            domain,
                            " is not in the configured gitlab-domains, adding it implicitly as authentication is configured for this domain"
                        )),
                        &[],
                    );
                    let merged = array_merge(
                        gitlab_domains.unwrap_or(PhpMixed::List(vec![])),
                        PhpMixed::List(vec![PhpMixed::string(domain.clone())]),
                    );
                    let mut inner = IndexMap::new();
                    inner.insert("gitlab-domains".into(), merged);
                    let mut config_outer: IndexMap<BString, PhpMixed> = IndexMap::new();
                    config_outer.insert("config".into(), PhpMixed::Array(inner));
                    config.merge(&config_outer, "implicit-due-to-auth");
                }

                let token_str = if let Some(arr) = token.as_array() {
                    BString::from(
                        arr.get(B("token"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                    )
                } else {
                    BString::from(token.as_string().unwrap_or_default())
                };
                self.check_and_set_authentication(domain, token_str, Some(BString::from("oauth2")));
            }
        }

        if let Some(map) = gitlab_token.as_opt().and_then(|v| v.as_array()) {
            for (domain, token) in map.clone() {
                let gitlab_domains = config.get("gitlab-domains");
                if domain != "gitlab.com"
                    && !in_array_strict(
                        PhpMixed::string(domain.clone()),
                        gitlab_domains
                            .clone()
                            .unwrap_or(PhpMixed::List(vec![]))
                            .values(),
                    )
                {
                    <Self as BaseIO>::debug(
                        self,
                        BStr::new(&bconcat!(
                            domain,
                            " is not in the configured gitlab-domains, adding it implicitly as authentication is configured for this domain"
                        )),
                        &[],
                    );
                    let merged = array_merge(
                        gitlab_domains.unwrap_or(PhpMixed::List(vec![])),
                        PhpMixed::List(vec![PhpMixed::string(domain.clone())]),
                    );
                    let mut inner = IndexMap::new();
                    inner.insert("gitlab-domains".into(), merged);
                    let mut config_outer: IndexMap<BString, PhpMixed> = IndexMap::new();
                    config_outer.insert("config".into(), PhpMixed::Array(inner));
                    config.merge(&config_outer, "implicit-due-to-auth");
                }

                let (username, password) = if let Some(arr) = token.as_array() {
                    (
                        BString::from(
                            arr.get(B("username"))
                                .and_then(|v| v.as_string())
                                .unwrap_or_default(),
                        ),
                        BString::from(
                            arr.get(B("token"))
                                .and_then(|v| v.as_string())
                                .unwrap_or_default(),
                        ),
                    )
                } else {
                    (
                        BString::from(token.as_string().unwrap_or_default()),
                        BString::from("private-token"),
                    )
                };
                self.check_and_set_authentication(domain, username, Some(password));
            }
        }

        if let Some(map) = forgejo_token.as_opt().and_then(|v| v.as_array()) {
            for (domain, cred) in map.clone() {
                let forgejo_domains = config.get("forgejo-domains");
                if !in_array_strict(
                    PhpMixed::string(domain.clone()),
                    forgejo_domains
                        .clone()
                        .unwrap_or(PhpMixed::List(vec![]))
                        .values(),
                ) {
                    <Self as BaseIO>::debug(
                        self,
                        BStr::new(&bconcat!(
                            domain,
                            " is not in the configured forgejo-domains, adding it implicitly as authentication is configured for this domain"
                        )),
                        &[],
                    );
                    let merged = array_merge(
                        forgejo_domains.unwrap_or(PhpMixed::List(vec![])),
                        PhpMixed::List(vec![PhpMixed::string(domain.clone())]),
                    );
                    let mut inner = IndexMap::new();
                    inner.insert("forgejo-domains".into(), merged);
                    let mut config_outer: IndexMap<BString, PhpMixed> = IndexMap::new();
                    config_outer.insert("config".into(), PhpMixed::Array(inner));
                    config.merge(&config_outer, "implicit-due-to-auth");
                }

                if let Some(cred_map) = cred.as_array() {
                    let username = BString::from(
                        cred_map
                            .get(B("username"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                    );
                    let token = cred_map
                        .get(B("token"))
                        .and_then(|v| v.as_string())
                        .map(BString::from);
                    self.check_and_set_authentication(domain, username, token);
                }
            }
        }

        if let Some(map) = http_basic.as_opt().and_then(|v| v.as_array()) {
            for (domain, cred) in map.clone() {
                if let Some(cred_map) = cred.as_array() {
                    let username = BString::from(
                        cred_map
                            .get(B("username"))
                            .and_then(|v| v.as_string())
                            .unwrap_or_default(),
                    );
                    let password = cred_map
                        .get(B("password"))
                        .and_then(|v| v.as_string())
                        .map(BString::from);
                    self.check_and_set_authentication(domain, username, password);
                }
            }
        }

        if let Some(map) = bearer_token.as_opt().and_then(|v| v.as_array()) {
            for (domain, token) in map.clone() {
                let token_str = BString::from(token.as_string().unwrap_or_default());
                self.check_and_set_authentication(domain, token_str, Some(BString::from("bearer")));
            }
        }

        if let Some(map) = custom_headers.as_opt().and_then(|v| v.as_array()) {
            for (domain, headers) in map.clone() {
                if !headers.is_null() {
                    let json_str = json_encode_ex(&headers, 0).unwrap_or_default();
                    self.check_and_set_authentication(
                        domain,
                        json_str,
                        Some(BString::from("custom-headers")),
                    );
                }
            }
        }

        if let Some(map) = client_certificate.as_opt().and_then(|v| v.as_array()) {
            for (domain, cred) in map.clone() {
                if let Some(cred_map) = cred.as_array() {
                    let local_cert = cred_map
                        .get(B("local_cert"))
                        .and_then(|v| v.as_string())
                        .map(BString::from);
                    let local_pk = cred_map
                        .get(B("local_pk"))
                        .and_then(|v| v.as_string())
                        .map(BString::from);
                    let passphrase = cred_map
                        .get(B("passphrase"))
                        .and_then(|v| v.as_string())
                        .map(BString::from);

                    let mut ssl_options = IndexMap::new();
                    if let Some(cert) = local_cert {
                        ssl_options.insert("local_cert".into(), PhpMixed::string(cert));
                    }
                    if let Some(pk) = local_pk {
                        ssl_options.insert("local_pk".into(), PhpMixed::string(pk));
                    }
                    if let Some(pass) = passphrase {
                        ssl_options.insert("passphrase".into(), PhpMixed::string(pass));
                    }

                    if !ssl_options.contains_key(B("local_cert")) {
                        self.write_error3(bconcat!("<warning>Warning: Client certificate configuration is missing key `local_cert` for ", domain, ".</warning>"), true, io_interface::NORMAL);
                        continue;
                    }

                    let json_str =
                        json_encode_ex(&PhpMixed::Array(ssl_options), 0).unwrap_or_default();
                    self.check_and_set_authentication(
                        domain,
                        BString::from("client-certificate"),
                        Some(json_str),
                    );
                }
            }
        }

        ProcessExecutor::set_timeout(config.get("process-timeout"));

        Ok(())
    }

    fn emergency(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.log(LogLevel::EMERGENCY, message, context);
    }

    fn alert(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.log(LogLevel::ALERT, message, context);
    }

    fn critical(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.log(LogLevel::CRITICAL, message, context);
    }

    fn error(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.log(LogLevel::ERROR, message, context);
    }

    fn warning(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.log(LogLevel::WARNING, message, context);
    }

    fn notice(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.log(LogLevel::NOTICE, message, context);
    }

    fn info(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.log(LogLevel::INFO, message, context);
    }

    fn debug(&self, message: &BStr, context: &[(&BStr, &BStr)]) {
        self.log(LogLevel::DEBUG, message, context);
    }

    fn log(&self, level: &BStr, message: &BStr, context: &[(&BStr, &BStr)]) {
        let mut message_str = BString::from(message);

        if !context.is_empty() {
            let json = json_encode_ex(
                &PhpMixed::Array(log_context(context)),
                JSON_INVALID_UTF8_IGNORE | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
            );
            if let Ok(json_str) = json {
                message_str = bconcat!(message_str, " ", json_str);
            }
        }

        if [
            LogLevel::EMERGENCY,
            LogLevel::ALERT,
            LogLevel::CRITICAL,
            LogLevel::ERROR,
        ]
        .contains(&level)
        {
            self.write_error3(
                bconcat!("<error>", message_str, "</error>"),
                true,
                io_interface::NORMAL,
            );
        } else if level == LogLevel::WARNING {
            self.write_error3(
                bconcat!("<warning>", message_str, "</warning>"),
                true,
                io_interface::NORMAL,
            );
        } else if level == LogLevel::NOTICE {
            self.write_error3(
                bconcat!("<info>", message_str, "</info>"),
                true,
                io_interface::VERBOSE,
            );
        } else if level == LogLevel::INFO {
            self.write_error3(
                bconcat!("<info>", message_str, "</info>"),
                true,
                io_interface::VERY_VERBOSE,
            );
        } else {
            self.write_error3(&message_str, true, io_interface::DEBUG);
        }
    }
}
