//! ref: composer/src/Composer/Util/AuthHelper.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    E_USER_DEPRECATED, PHP_URL_HOST, PHP_URL_PATH, PHP_URL_SCHEME, PhpMixed, base64_encode,
    explode, in_array, is_array, is_string, json_decode, parse_url, sprintf, str_replace, strpos,
    strtolower, substr, trigger_error, trim,
};

use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::util::bitbucket::Bitbucket;
use crate::util::github::GitHub;
use crate::util::gitlab::GitLab;

pub struct AuthHelper {
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) config: Config,
    /// @var array<string, string> Map of origins to message displayed
    displayed_origin_authentications: IndexMap<String, String>,
    /// @var array<string, bool> Map of URLs and whether they already retried with authentication from Bitbucket
    bitbucket_retry: IndexMap<String, bool>,
}

#[derive(Debug)]
pub struct PromptAuthResult {
    pub retry: bool,
    /// @phpstan-var 'prompt'|bool
    pub store_auth: StoreAuth,
}

#[derive(Debug, Clone)]
pub enum StoreAuth {
    Bool(bool),
    Prompt,
}

impl AuthHelper {
    pub fn new(io: Box<dyn IOInterface>, config: Config) -> Self {
        Self {
            io,
            config,
            displayed_origin_authentications: IndexMap::new(),
            bitbucket_retry: IndexMap::new(),
        }
    }

    /// @param 'prompt'|bool $storeAuth
    pub fn store_auth(&self, origin: &str, store_auth: StoreAuth) -> Result<()> {
        // TODO(phase-b): config.get_auth_config_source() and ConfigSource methods are stubs
        let mut store: Option<()> = None;
        let config_source = self.config.get_auth_config_source();
        if matches!(store_auth, StoreAuth::Bool(true)) {
            store = Some(());
        } else if matches!(store_auth, StoreAuth::Prompt) {
            let answer = self.io.ask_and_validate(
                format!(
                    "Do you want to store credentials for {} in {} ? [Yn] ",
                    origin,
                    config_source.get_name(),
                ),
                Box::new(|value: PhpMixed| -> PhpMixed {
                    let input = strtolower(&substr(
                        &trim(value.as_string().unwrap_or(""), None),
                        0,
                        Some(1),
                    ));
                    if in_array(
                        PhpMixed::String(input.clone()),
                        &PhpMixed::List(vec![
                            Box::new(PhpMixed::String("y".to_string())),
                            Box::new(PhpMixed::String("n".to_string())),
                        ]),
                        false,
                    ) {
                        return PhpMixed::String(input);
                    }
                    // PHP: throw new \RuntimeException('Please answer (y)es or (n)o');
                    // TODO(phase-b): validator should return a recoverable error rather than panic
                    panic!("Please answer (y)es or (n)o");
                }),
                None,
                PhpMixed::String("y".to_string()),
            );

            if answer.as_string() == Some("y") {
                store = Some(());
            }
        }
        if store.is_some() {
            config_source.add_config_setting(
                &format!("http-basic.{}", origin),
                // TODO(phase-b): convert IOInterface auth IndexMap into PhpMixed
                todo!("IOInterface.get_authentication(origin) as PhpMixed"),
            )?;
        }
        Ok(())
    }

    /// @param  int         $statusCode HTTP status code that triggered this call
    /// @param  string|null $reason     a message/description explaining why this was called
    /// @param  string[]    $headers
    /// @param  int         $retryCount the amount of retries already done on this URL
    /// @return array       containing retry (bool) and storeAuth (string|bool) keys, if retry is true the request should be
    ///                                retried, if storeAuth is true then on a successful retry the authentication should be persisted to auth.json
    /// @phpstan-return array{retry: bool, storeAuth: 'prompt'|bool}
    pub fn prompt_auth_if_needed(
        &mut self,
        url: &str,
        origin: &str,
        status_code: i64,
        reason: Option<&str>,
        headers: Vec<String>,
        retry_count: i64,
        response_body: Option<&str>,
    ) -> Result<PromptAuthResult> {
        let mut store_auth: StoreAuth = StoreAuth::Bool(false);

        let github_domains = self.config.get("github-domains");
        let github_domain_list = match github_domains.as_array() {
            Some(arr) => arr.clone(),
            None => IndexMap::new(),
        };
        let in_github_domains = github_domain_list
            .values()
            .any(|v| v.as_string() == Some(origin));

        let gitlab_domains = self.config.get("gitlab-domains");
        let gitlab_domain_list = match gitlab_domains.as_array() {
            Some(arr) => arr.clone(),
            None => IndexMap::new(),
        };
        let in_gitlab_domains = gitlab_domain_list
            .values()
            .any(|v| v.as_string() == Some(origin));

        if in_github_domains {
            let mut git_hub_util = GitHub::new(
                // TODO(phase-b): clone or borrow io/config rather than moving
                todo!("io clone"),
                todo!("config clone"),
                None,
                None,
            )?;
            let mut message = "\n".to_string();

            let rate_limited = git_hub_util.is_rate_limited(&headers);
            let requires_sso = git_hub_util.requires_sso(&headers);

            if requires_sso {
                let sso_url = git_hub_util.get_sso_url(&headers);
                message = format!(
                    "GitHub API token requires SSO authorization. Authorize this token at {}\n",
                    sso_url,
                );
                self.io
                    .write_error(PhpMixed::String(message), true, IOInterface::NORMAL);
                if !self.io.is_interactive() {
                    return Err(TransportException::new(
                        format!("Could not authenticate against {}", origin),
                        403,
                    )
                    .into());
                }
                self.io.ask(
                    "After authorizing your token, confirm that you would like to retry the request"
                        .to_string(),
                    PhpMixed::Null,
                );

                return Ok(PromptAuthResult {
                    retry: true,
                    store_auth,
                });
            }

            if rate_limited {
                let rate_limit = git_hub_util.get_rate_limit(&headers);
                if self.io.has_authentication(origin) {
                    message = "Review your configured GitHub OAuth token or enter a new one to go over the API rate limit.".to_string();
                } else {
                    message =
                        "Create a GitHub OAuth token to go over the API rate limit.".to_string();
                }

                message = format!(
                    "{}\n",
                    sprintf(
                        &format!(
                            "GitHub API limit (%d calls/hr) is exhausted, could not fetch {}. {} You can also wait until %s for the rate limit to reset.",
                            url, message,
                        ),
                        &[
                            rate_limit.get("limit").cloned().unwrap_or(PhpMixed::Null),
                            rate_limit.get("reset").cloned().unwrap_or(PhpMixed::Null),
                        ],
                    ),
                );
            } else {
                // Try to extract a more specific error message from GitHub's API response
                let mut git_hub_api_message: Option<String> = None;
                if let Some(body) = response_body {
                    let decoded = json_decode(body, true)?;
                    if is_array(&decoded) {
                        if let Some(arr) = decoded.as_array() {
                            if let Some(msg) = arr.get("message") {
                                if is_string(msg) {
                                    git_hub_api_message = msg.as_string().map(|s| s.to_string());
                                }
                            }
                        }
                    }
                }

                if let Some(api_message) = git_hub_api_message {
                    message.push_str(&format!("Could not fetch {}: {}", url, api_message));
                } else {
                    message.push_str(&format!("Could not fetch {}, please ", url));
                    if self.io.has_authentication(origin) {
                        message.push_str(
                            "review your configured GitHub OAuth token or enter a new one to access private repos",
                        );
                    } else {
                        message.push_str("create a GitHub OAuth token to access private repos");
                    }
                }
            }

            if !git_hub_util.authorize_oauth(origin)
                && (!self.io.is_interactive()
                    || !git_hub_util.authorize_oauth_interactively(origin, &message))
            {
                return Err(TransportException::new(
                    format!("Could not authenticate against {}", origin),
                    401,
                )
                .into());
            }
        } else if in_gitlab_domains {
            let message = format!(
                "\nCould not fetch {}, enter your {} credentials {}",
                url,
                origin,
                if status_code == 401 {
                    "to access private repos"
                } else {
                    "to go over the API rate limit"
                },
            );
            let mut git_lab_util = GitLab::new(
                // TODO(phase-b): clone or borrow io/config rather than moving
                todo!("io clone"),
                todo!("config clone"),
                None,
                None,
            )?;

            let mut auth: Option<IndexMap<String, Option<String>>> = None;
            if self.io.has_authentication(origin) {
                auth = Some(self.io.get_authentication(origin));
                let password = auth
                    .as_ref()
                    .and_then(|a| a.get("password"))
                    .and_then(|v| v.clone())
                    .unwrap_or_default();
                if in_array(
                    PhpMixed::String(password),
                    &PhpMixed::List(vec![
                        Box::new(PhpMixed::String("gitlab-ci-token".to_string())),
                        Box::new(PhpMixed::String("private-token".to_string())),
                        Box::new(PhpMixed::String("oauth2".to_string())),
                    ]),
                    true,
                ) {
                    return Err(TransportException::new(
                        format!("Invalid credentials for '{}', aborting.", url),
                        status_code,
                    )
                    .into());
                }
            }

            let scheme = parse_url(url, PHP_URL_SCHEME);
            if !git_lab_util.authorize_oauth(origin)
                && (!self.io.is_interactive()
                    || !git_lab_util.authorize_oauth_interactively(
                        scheme.as_string().unwrap_or(""),
                        origin,
                        &message,
                    ))
            {
                return Err(TransportException::new(
                    format!("Could not authenticate against {}", origin),
                    401,
                )
                .into());
            }

            if let Some(prev_auth) = auth {
                if self.io.has_authentication(origin) {
                    let current_auth = self.io.get_authentication(origin);
                    // TODO(phase-b): IndexMap equality compares all entries by value
                    if prev_auth == current_auth {
                        return Err(TransportException::new(
                            format!("Invalid credentials for '{}', aborting.", url),
                            status_code,
                        )
                        .into());
                    }
                }
            }
        } else if origin == "bitbucket.org" || origin == "api.bitbucket.org" {
            let mut ask_for_oauth_token = true;
            let origin = "bitbucket.org".to_string();
            if self.io.has_authentication(&origin) {
                let auth = self.io.get_authentication(&origin);
                let username = auth
                    .get("username")
                    .and_then(|v| v.clone())
                    .unwrap_or_default();
                if username != "x-token-auth" {
                    let mut bitbucket_util = Bitbucket::new(
                        // TODO(phase-b): clone or borrow io/config rather than moving
                        todo!("io clone"),
                        todo!("config clone"),
                        None,
                        None,
                        None,
                    )?;
                    let password = auth
                        .get("password")
                        .and_then(|v| v.clone())
                        .unwrap_or_default();
                    let access_token =
                        bitbucket_util.request_token(&origin, &username, &password)?;
                    if !access_token.is_empty() {
                        self.io.set_authentication(
                            origin.clone(),
                            "x-token-auth".to_string(),
                            Some(access_token),
                        );
                        ask_for_oauth_token = false;
                    }
                } else if !self.bitbucket_retry.contains_key(url) {
                    // when multiple requests fire at the same time, they will all fail and the first one resets the token to be correct above but then the others
                    // reach the code path and without this fallback they would end up throwing below
                    // see https://github.com/composer/composer/pull/11464 for more details
                    ask_for_oauth_token = false;
                    self.bitbucket_retry.insert(url.to_string(), true);
                } else {
                    return Err(TransportException::new(
                        format!("Could not authenticate against {}", origin),
                        401,
                    )
                    .into());
                }
            }

            if ask_for_oauth_token {
                let message = format!(
                    "\nCould not fetch {}, please create a bitbucket OAuth token to {}",
                    url,
                    if status_code == 401 || status_code == 403 {
                        "access private repos"
                    } else {
                        "go over the API rate limit"
                    },
                );
                let mut bit_bucket_util = Bitbucket::new(
                    // TODO(phase-b): clone or borrow io/config rather than moving
                    todo!("io clone"),
                    todo!("config clone"),
                    None,
                    None,
                    None,
                )?;
                if !bit_bucket_util.authorize_oauth(&origin)
                    && (!self.io.is_interactive()
                        || !bit_bucket_util.authorize_oauth_interactively(&origin, &message))
                {
                    return Err(TransportException::new(
                        format!("Could not authenticate against {}", origin),
                        401,
                    )
                    .into());
                }
            }
        } else {
            // 404s are only handled for github
            if status_code == 404 {
                return Ok(PromptAuthResult {
                    retry: false,
                    store_auth: StoreAuth::Bool(false),
                });
            }

            // fail if the console is not interactive
            if !self.io.is_interactive() {
                let message = if status_code == 401 {
                    format!(
                        "The '{}' URL required authentication (HTTP 401).\nYou must be using the interactive console to authenticate",
                        url,
                    )
                } else if status_code == 403 {
                    format!(
                        "The '{}' URL could not be accessed (HTTP 403): {}",
                        url,
                        reason.unwrap_or(""),
                    )
                } else {
                    format!(
                        "Unknown error code '{}', reason: {}",
                        status_code,
                        reason.unwrap_or(""),
                    )
                };

                return Err(TransportException::new(message, status_code).into());
            }

            // fail if we already have auth
            if self.io.has_authentication(origin) {
                // if two or more requests are started together for the same host, and the first
                // received authentication already, we let the others retry before failing them
                if retry_count == 0 {
                    return Ok(PromptAuthResult {
                        retry: true,
                        store_auth: StoreAuth::Bool(false),
                    });
                }

                return Err(TransportException::new(
                    format!(
                        "Invalid credentials (HTTP {}) for '{}', aborting.",
                        status_code, url,
                    ),
                    status_code,
                )
                .into());
            }

            self.io.write_error(
                PhpMixed::String(format!(
                    "    Authentication required (<info>{}</info>):",
                    origin,
                )),
                true,
                IOInterface::NORMAL,
            );
            let username = self.io.ask("      Username: ".to_string(), PhpMixed::Null);
            let password = self.io.ask_and_hide_answer("      Password: ".to_string());
            self.io.set_authentication(
                origin.to_string(),
                username.as_string().unwrap_or("").to_string(),
                password,
            );
            // PHP: $this->config->get('store-auths') returns 'prompt'|bool
            // TODO(phase-b): decode the PhpMixed result into StoreAuth
            store_auth = match self.config.get("store-auths") {
                PhpMixed::Bool(b) => StoreAuth::Bool(b),
                PhpMixed::String(ref s) if s == "prompt" => StoreAuth::Prompt,
                _ => StoreAuth::Bool(false),
            };
        }

        Ok(PromptAuthResult {
            retry: true,
            store_auth,
        })
    }

    /// @deprecated use addAuthenticationOptions instead
    ///
    /// @param string[] $headers
    ///
    /// @return string[] updated headers array
    pub fn add_authentication_header(
        &mut self,
        headers: Vec<String>,
        origin: &str,
        url: &str,
    ) -> Result<Vec<String>> {
        trigger_error(
            "AuthHelper::addAuthenticationHeader is deprecated since Composer 2.9 use addAuthenticationOptions instead.",
            E_USER_DEPRECATED,
        );

        // PHP: $options = ['http' => ['header' => &$headers]];
        // PHP uses references so subsequent mutations affect $headers
        let mut options: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut http: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        http.insert(
            "header".to_string(),
            Box::new(PhpMixed::List(
                headers
                    .iter()
                    .map(|h| Box::new(PhpMixed::String(h.clone())))
                    .collect(),
            )),
        );
        options.insert("http".to_string(), PhpMixed::Array(http));

        let options = self.add_authentication_options(options, origin, url)?;

        let http = options.get("http").and_then(|v| v.as_array()).unwrap();
        let header = http.get("header").and_then(|v| v.as_list()).unwrap();
        Ok(header
            .iter()
            .filter_map(|v| v.as_string().map(|s| s.to_string()))
            .collect())
    }

    /// @param array<string, mixed> $options
    ///
    /// @return array<string, mixed> updated options
    pub fn add_authentication_options(
        &mut self,
        mut options: IndexMap<String, PhpMixed>,
        origin: &str,
        url: &str,
    ) -> Result<IndexMap<String, PhpMixed>> {
        if !options.contains_key("http") {
            options.insert("http".to_string(), PhpMixed::Array(IndexMap::new()));
        }
        // PHP: if (!isset($options['http']['header']))
        // TODO(phase-b): mutate nested PhpMixed in place rather than copying
        {
            let http_has_header = if let Some(PhpMixed::Array(http)) = options.get("http") {
                http.contains_key("header")
            } else {
                false
            };
            if !http_has_header {
                if let Some(PhpMixed::Array(http)) = options.get_mut("http") {
                    http.insert("header".to_string(), Box::new(PhpMixed::List(vec![])));
                }
            }
        }

        // PHP: $headers = &$options['http']['header'];
        // TODO(phase-b): captured by reference; pushes below modify the same list
        let mut headers: Vec<PhpMixed> = match options
            .get("http")
            .and_then(|v| v.as_array())
            .and_then(|h| h.get("header"))
            .and_then(|v| v.as_list())
        {
            Some(list) => list.iter().map(|b| (**b).clone()).collect(),
            None => vec![],
        };

        if self.io.has_authentication(origin) {
            let mut authentication_display_message: Option<String> = None;
            let auth = self.io.get_authentication(origin);
            let password = auth
                .get("password")
                .and_then(|v| v.clone())
                .unwrap_or_default();
            let username = auth
                .get("username")
                .and_then(|v| v.clone())
                .unwrap_or_default();
            if password == "bearer" {
                headers.push(PhpMixed::String(format!(
                    "Authorization: Bearer {}",
                    username,
                )));
            } else if password == "custom-headers" {
                // Handle custom HTTP headers from auth.json
                let mut custom_headers: PhpMixed = PhpMixed::Null;
                // PHP: if (is_string($auth['username']))
                // username field is always String in our IndexMap representation
                custom_headers = json_decode(&username, true)?;
                if is_array(&custom_headers) {
                    if let Some(arr) = custom_headers.as_array() {
                        for header in arr.values() {
                            headers.push((**header).clone());
                        }
                    } else if let Some(list) = custom_headers.as_list() {
                        for header in list {
                            headers.push((**header).clone());
                        }
                    }
                    authentication_display_message =
                        Some("Using custom HTTP headers for authentication".to_string());
                }
            } else if origin == "github.com" && password == "x-oauth-basic" {
                // only add the access_token if it is actually a github API URL
                if Preg::is_match(r"{^https?://api\.github\.com/}", url) {
                    headers.push(PhpMixed::String(format!(
                        "Authorization: token {}",
                        username,
                    )));
                    authentication_display_message =
                        Some("Using GitHub token authentication".to_string());
                }
            } else if in_array(
                PhpMixed::String(password.clone()),
                &PhpMixed::List(vec![
                    Box::new(PhpMixed::String("oauth2".to_string())),
                    Box::new(PhpMixed::String("private-token".to_string())),
                    Box::new(PhpMixed::String("gitlab-ci-token".to_string())),
                ]),
                true,
            ) && in_array(
                PhpMixed::String(origin.to_string()),
                &PhpMixed::List(
                    self.config
                        .get("gitlab-domains")
                        .as_array()
                        .map(|a| a.values().cloned().collect())
                        .unwrap_or_default(),
                ),
                true,
            ) {
                if password == "oauth2" {
                    headers.push(PhpMixed::String(format!(
                        "Authorization: Bearer {}",
                        username,
                    )));
                    authentication_display_message =
                        Some("Using GitLab OAuth token authentication".to_string());
                } else {
                    headers.push(PhpMixed::String(format!("PRIVATE-TOKEN: {}", username)));
                    authentication_display_message =
                        Some("Using GitLab private token authentication".to_string());
                }
            } else if origin == "bitbucket.org"
                && url != Bitbucket::OAUTH2_ACCESS_TOKEN_URL
                && username == "x-token-auth"
            {
                if !self.is_public_bit_bucket_download(url) {
                    headers.push(PhpMixed::String(format!(
                        "Authorization: Bearer {}",
                        password,
                    )));
                    authentication_display_message =
                        Some("Using Bitbucket OAuth token authentication".to_string());
                }
            } else if username == "client-certificate" {
                // PHP: $options['ssl'] = array_merge($options['ssl'] ?? [], json_decode((string) $auth['password'], true));
                let existing_ssl = options
                    .get("ssl")
                    .cloned()
                    .unwrap_or(PhpMixed::Array(IndexMap::new()));
                let decoded = json_decode(&password, true)?;
                options.insert(
                    "ssl".to_string(),
                    shirabe_php_shim::array_merge(existing_ssl, decoded),
                );
                authentication_display_message = Some("Using SSL client certificate".to_string());
            } else {
                let auth_str = base64_encode(&format!("{}:{}", username, password));
                headers.push(PhpMixed::String(format!(
                    "Authorization: Basic {}",
                    auth_str,
                )));
                authentication_display_message = Some(format!(
                    "Using HTTP basic authentication with username \"{}\"",
                    username,
                ));
            }

            if let Some(display_message) = &authentication_display_message {
                let already_displayed =
                    self.displayed_origin_authentications.get(origin) == Some(display_message);
                if !already_displayed {
                    self.io.write_error(
                        PhpMixed::String(display_message.clone()),
                        true,
                        IOInterface::DEBUG,
                    );
                    self.displayed_origin_authentications
                        .insert(origin.to_string(), display_message.clone());
                }
            }
        } else if in_array(
            PhpMixed::String(origin.to_string()),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::String("api.bitbucket.org".to_string())),
                Box::new(PhpMixed::String("api.github.com".to_string())),
            ]),
            true,
        ) {
            return self.add_authentication_options(options, &str_replace("api.", "", origin), url);
        }

        // write headers back into options['http']['header']
        if let Some(PhpMixed::Array(http)) = options.get_mut("http") {
            http.insert(
                "header".to_string(),
                Box::new(PhpMixed::List(headers.into_iter().map(Box::new).collect())),
            );
        }

        Ok(options)
    }

    /// @link https://github.com/composer/composer/issues/5584
    ///
    /// @param string $urlToBitBucketFile URL to a file at bitbucket.org.
    ///
    /// @return bool Whether the given URL is a public BitBucket download which requires no authentication.
    pub fn is_public_bit_bucket_download(&self, url_to_bit_bucket_file: &str) -> bool {
        let domain = parse_url(url_to_bit_bucket_file, PHP_URL_HOST);
        let domain_str = domain.as_string().unwrap_or("");
        if strpos(domain_str, "bitbucket.org").is_none() {
            // Bitbucket downloads are hosted on amazonaws.
            // We do not need to authenticate there at all
            return true;
        }

        let path = parse_url(url_to_bit_bucket_file, PHP_URL_PATH);
        let path_str = path.as_string().unwrap_or("");

        // Path for a public download follows this pattern /{user}/{repo}/downloads/{whatever}
        // {@link https://blog.bitbucket.org/2009/04/12/new-feature-downloads/}
        let path_parts = explode("/", path_str);

        path_parts.len() as i64 >= 4 && path_parts.get(3).map(|s| s.as_str()) == Some("downloads")
    }
}
