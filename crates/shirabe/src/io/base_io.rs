//! ref: composer/src/Composer/IO/BaseIO.php

use crate::config::Config;
use crate::io::IOInterface;
use crate::io::io_interface;
use crate::util::ProcessExecutor;
use crate::util::Silencer;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::psr::log::LogLevel;
use shirabe_php_shim::{
    JSON_INVALID_UTF8_IGNORE, JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE, PhpMixed,
    UnexpectedValueException, array_merge, in_array, json_encode_ex,
};

fn log_context(context: &[(&str, &str)]) -> IndexMap<String, Box<PhpMixed>> {
    context
        .iter()
        .map(|(k, v)| (k.to_string(), Box::new(PhpMixed::String(v.to_string()))))
        .collect()
}

pub trait BaseIO: IOInterface {
    fn authentications(&self) -> &IndexMap<String, IndexMap<String, Option<String>>>;
    fn authentications_mut(&mut self) -> &mut IndexMap<String, IndexMap<String, Option<String>>>;

    fn get_authentications(&self) -> IndexMap<String, IndexMap<String, Option<String>>> {
        self.authentications().clone()
    }

    fn reset_authentications(&mut self) {
        *self.authentications_mut() = IndexMap::new();
    }

    fn has_authentication(&self, repository_name: &str) -> bool {
        self.authentications().contains_key(repository_name)
    }

    fn get_authentication(&self, repository_name: &str) -> IndexMap<String, Option<String>> {
        if let Some(auth) = self.authentications().get(repository_name) {
            return auth.clone();
        }
        let mut result = IndexMap::new();
        result.insert("username".to_string(), None);
        result.insert("password".to_string(), None);
        result
    }

    fn set_authentication(
        &mut self,
        repository_name: String,
        username: String,
        password: Option<String>,
    ) {
        let mut auth = IndexMap::new();
        auth.insert("username".to_string(), Some(username));
        auth.insert("password".to_string(), password);
        self.authentications_mut().insert(repository_name, auth);
    }

    fn check_and_set_authentication(
        &mut self,
        repository_name: String,
        username: String,
        password: Option<String>,
    ) {
        if BaseIO::has_authentication(self, &repository_name) {
            let auth = BaseIO::get_authentication(self, &repository_name);
            if auth.get("username").and_then(|v| v.as_deref()) == Some(username.as_str())
                && *auth.get("password").unwrap_or(&None) == password
            {
                return;
            }
            self.write_error3(
                &format!(
                    "<warning>Warning: You should avoid overwriting already defined auth settings for {}.</warning>",
                    repository_name
                ),
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
                    let consumer_key = cred_map
                        .get("consumer-key")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    let consumer_secret = cred_map
                        .get("consumer-secret")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string());
                    self.check_and_set_authentication(domain, consumer_key, consumer_secret);
                }
            }
        }

        if let Some(map) = github_oauth.as_opt().and_then(|v| v.as_array()) {
            for (domain, token) in map.clone() {
                let token_str = token.as_string().unwrap_or("").to_string();
                let github_domains = config.get("github-domains");
                if domain != "github.com"
                    && !in_array(
                        PhpMixed::String(domain.clone()),
                        &github_domains.clone().unwrap_or(PhpMixed::List(vec![])),
                        true,
                    )
                {
                    <Self as BaseIO>::debug(
                        self,
                        &format!(
                            "{} is not in the configured github-domains, adding it implicitly as authentication is configured for this domain",
                            domain
                        ),
                        &[],
                    );
                    let merged = array_merge(
                        github_domains.unwrap_or(PhpMixed::List(vec![])),
                        PhpMixed::List(vec![Box::new(PhpMixed::String(domain.clone()))]),
                    );
                    let mut inner = IndexMap::new();
                    inner.insert("github-domains".to_string(), Box::new(merged));
                    let mut config_outer: IndexMap<String, PhpMixed> = IndexMap::new();
                    config_outer.insert("config".to_string(), PhpMixed::Array(inner));
                    config.merge(&config_outer, "implicit-due-to-auth");
                }

                if !Preg::is_match(r"^[.A-Za-z0-9_]+$", &token_str) {
                    return Err(anyhow::anyhow!(UnexpectedValueException {
                        message: format!(
                            "Your github oauth token for {} contains invalid characters: \"{}\"",
                            domain, token_str
                        ),
                        code: 0,
                    }));
                }
                self.check_and_set_authentication(
                    domain,
                    token_str,
                    Some("x-oauth-basic".to_string()),
                );
            }
        }

        if let Some(map) = gitlab_oauth.as_opt().and_then(|v| v.as_array()) {
            for (domain, token) in map.clone() {
                let gitlab_domains = config.get("gitlab-domains");
                if domain != "gitlab.com"
                    && !in_array(
                        PhpMixed::String(domain.clone()),
                        &gitlab_domains.clone().unwrap_or(PhpMixed::List(vec![])),
                        true,
                    )
                {
                    <Self as BaseIO>::debug(
                        self,
                        &format!(
                            "{} is not in the configured gitlab-domains, adding it implicitly as authentication is configured for this domain",
                            domain
                        ),
                        &[],
                    );
                    let merged = array_merge(
                        gitlab_domains.unwrap_or(PhpMixed::List(vec![])),
                        PhpMixed::List(vec![Box::new(PhpMixed::String(domain.clone()))]),
                    );
                    let mut inner = IndexMap::new();
                    inner.insert("gitlab-domains".to_string(), Box::new(merged));
                    let mut config_outer: IndexMap<String, PhpMixed> = IndexMap::new();
                    config_outer.insert("config".to_string(), PhpMixed::Array(inner));
                    config.merge(&config_outer, "implicit-due-to-auth");
                }

                let token_str = if let Some(arr) = token.as_array() {
                    arr.get("token")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string()
                } else {
                    token.as_string().unwrap_or("").to_string()
                };
                self.check_and_set_authentication(domain, token_str, Some("oauth2".to_string()));
            }
        }

        if let Some(map) = gitlab_token.as_opt().and_then(|v| v.as_array()) {
            for (domain, token) in map.clone() {
                let gitlab_domains = config.get("gitlab-domains");
                if domain != "gitlab.com"
                    && !in_array(
                        PhpMixed::String(domain.clone()),
                        &gitlab_domains.clone().unwrap_or(PhpMixed::List(vec![])),
                        true,
                    )
                {
                    <Self as BaseIO>::debug(
                        self,
                        &format!(
                            "{} is not in the configured gitlab-domains, adding it implicitly as authentication is configured for this domain",
                            domain
                        ),
                        &[],
                    );
                    let merged = array_merge(
                        gitlab_domains.unwrap_or(PhpMixed::List(vec![])),
                        PhpMixed::List(vec![Box::new(PhpMixed::String(domain.clone()))]),
                    );
                    let mut inner = IndexMap::new();
                    inner.insert("gitlab-domains".to_string(), Box::new(merged));
                    let mut config_outer: IndexMap<String, PhpMixed> = IndexMap::new();
                    config_outer.insert("config".to_string(), PhpMixed::Array(inner));
                    config.merge(&config_outer, "implicit-due-to-auth");
                }

                let (username, password) = if let Some(arr) = token.as_array() {
                    (
                        arr.get("username")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string(),
                        arr.get("token")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string(),
                    )
                } else {
                    (
                        token.as_string().unwrap_or("").to_string(),
                        "private-token".to_string(),
                    )
                };
                self.check_and_set_authentication(domain, username, Some(password));
            }
        }

        if let Some(map) = forgejo_token.as_opt().and_then(|v| v.as_array()) {
            for (domain, cred) in map.clone() {
                let forgejo_domains = config.get("forgejo-domains");
                if !in_array(
                    PhpMixed::String(domain.clone()),
                    &forgejo_domains.clone().unwrap_or(PhpMixed::List(vec![])),
                    true,
                ) {
                    <Self as BaseIO>::debug(
                        self,
                        &format!(
                            "{} is not in the configured forgejo-domains, adding it implicitly as authentication is configured for this domain",
                            domain
                        ),
                        &[],
                    );
                    let merged = array_merge(
                        forgejo_domains.unwrap_or(PhpMixed::List(vec![])),
                        PhpMixed::List(vec![Box::new(PhpMixed::String(domain.clone()))]),
                    );
                    let mut inner = IndexMap::new();
                    inner.insert("forgejo-domains".to_string(), Box::new(merged));
                    let mut config_outer: IndexMap<String, PhpMixed> = IndexMap::new();
                    config_outer.insert("config".to_string(), PhpMixed::Array(inner));
                    config.merge(&config_outer, "implicit-due-to-auth");
                }

                if let Some(cred_map) = cred.as_array() {
                    let username = cred_map
                        .get("username")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    let token = cred_map
                        .get("token")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string());
                    self.check_and_set_authentication(domain, username, token);
                }
            }
        }

        if let Some(map) = http_basic.as_opt().and_then(|v| v.as_array()) {
            for (domain, cred) in map.clone() {
                if let Some(cred_map) = cred.as_array() {
                    let username = cred_map
                        .get("username")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    let password = cred_map
                        .get("password")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string());
                    self.check_and_set_authentication(domain, username, password);
                }
            }
        }

        if let Some(map) = bearer_token.as_opt().and_then(|v| v.as_array()) {
            for (domain, token) in map.clone() {
                let token_str = token.as_string().unwrap_or("").to_string();
                self.check_and_set_authentication(domain, token_str, Some("bearer".to_string()));
            }
        }

        if let Some(map) = custom_headers.as_opt().and_then(|v| v.as_array()) {
            for (domain, headers) in map.clone() {
                if !headers.is_null() {
                    let json_str = json_encode_ex(&headers, 0).unwrap_or_default();
                    self.check_and_set_authentication(
                        domain,
                        json_str,
                        Some("custom-headers".to_string()),
                    );
                }
            }
        }

        if let Some(map) = client_certificate.as_opt().and_then(|v| v.as_array()) {
            for (domain, cred) in map.clone() {
                if let Some(cred_map) = cred.as_array() {
                    let local_cert = cred_map
                        .get("local_cert")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string());
                    let local_pk = cred_map
                        .get("local_pk")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string());
                    let passphrase = cred_map
                        .get("passphrase")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string());

                    let mut ssl_options: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                    if let Some(cert) = local_cert {
                        ssl_options
                            .insert("local_cert".to_string(), Box::new(PhpMixed::String(cert)));
                    }
                    if let Some(pk) = local_pk {
                        ssl_options.insert("local_pk".to_string(), Box::new(PhpMixed::String(pk)));
                    }
                    if let Some(pass) = passphrase {
                        ssl_options
                            .insert("passphrase".to_string(), Box::new(PhpMixed::String(pass)));
                    }

                    if !ssl_options.contains_key("local_cert") {
                        self.write_error3(&format!(
                            "<warning>Warning: Client certificate configuration is missing key `local_cert` for {}.</warning>",
                            domain
                        ), true, io_interface::NORMAL);
                        continue;
                    }

                    let json_str =
                        json_encode_ex(&PhpMixed::Array(ssl_options), 0).unwrap_or_default();
                    self.check_and_set_authentication(
                        domain,
                        "client-certificate".to_string(),
                        Some(json_str),
                    );
                }
            }
        }

        ProcessExecutor::set_timeout(config.get("process-timeout"));

        Ok(())
    }

    fn emergency(&self, message: &str, context: &[(&str, &str)]) {
        self.log(
            PhpMixed::String(LogLevel::EMERGENCY.to_string()),
            message,
            context,
        );
    }

    fn alert(&self, message: &str, context: &[(&str, &str)]) {
        self.log(
            PhpMixed::String(LogLevel::ALERT.to_string()),
            message,
            context,
        );
    }

    fn critical(&self, message: &str, context: &[(&str, &str)]) {
        self.log(
            PhpMixed::String(LogLevel::CRITICAL.to_string()),
            message,
            context,
        );
    }

    fn error(&self, message: &str, context: &[(&str, &str)]) {
        self.log(
            PhpMixed::String(LogLevel::ERROR.to_string()),
            message,
            context,
        );
    }

    fn warning(&self, message: &str, context: &[(&str, &str)]) {
        self.log(
            PhpMixed::String(LogLevel::WARNING.to_string()),
            message,
            context,
        );
    }

    fn notice(&self, message: &str, context: &[(&str, &str)]) {
        self.log(
            PhpMixed::String(LogLevel::NOTICE.to_string()),
            message,
            context,
        );
    }

    fn info(&self, message: &str, context: &[(&str, &str)]) {
        self.log(
            PhpMixed::String(LogLevel::INFO.to_string()),
            message,
            context,
        );
    }

    fn debug(&self, message: &str, context: &[(&str, &str)]) {
        self.log(
            PhpMixed::String(LogLevel::DEBUG.to_string()),
            message,
            context,
        );
    }

    fn log(&self, level: PhpMixed, message: &str, context: &[(&str, &str)]) {
        let mut message_str = message.to_string();

        if !context.is_empty() {
            let json: anyhow::Result<Option<String>> = Silencer::call(|| {
                Ok(json_encode_ex(
                    &PhpMixed::Array(log_context(context)),
                    JSON_INVALID_UTF8_IGNORE | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
                ))
            });
            if let Ok(Some(json_str)) = json {
                message_str += " ";
                message_str += &json_str;
            }
        }

        let level_str = level.as_string().unwrap_or("");
        if in_array(
            level.clone(),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::String(LogLevel::EMERGENCY.to_string())),
                Box::new(PhpMixed::String(LogLevel::ALERT.to_string())),
                Box::new(PhpMixed::String(LogLevel::CRITICAL.to_string())),
                Box::new(PhpMixed::String(LogLevel::ERROR.to_string())),
            ]),
            false,
        ) {
            self.write_error3(
                &format!("<error>{}</error>", message_str),
                true,
                io_interface::NORMAL,
            );
        } else if level_str == LogLevel::WARNING {
            self.write_error3(
                &format!("<warning>{}</warning>", message_str),
                true,
                io_interface::NORMAL,
            );
        } else if level_str == LogLevel::NOTICE {
            self.write_error3(
                &format!("<info>{}</info>", message_str),
                true,
                io_interface::VERBOSE,
            );
        } else if level_str == LogLevel::INFO {
            self.write_error3(
                &format!("<info>{}</info>", message_str),
                true,
                io_interface::VERY_VERBOSE,
            );
        } else {
            self.write_error3(&message_str, true, io_interface::DEBUG);
        }
    }
}
