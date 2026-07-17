//! ref: composer/src/Composer/Util/Http/CurlDownloader.php
//!
//! reqwest-based re-implementation. Unlike the libcurl multi-handle event loop of the PHP
//! original, `download()` is a single `async fn`: it sends the request, hands the result to
//! `decide()` (retry / redirect / fail / succeed), and loops until it resolves. There is no job
//! table or `tick()` driver anymore; the caller `.await`s `download()` directly and gets the
//! final `Response` (or `TransportException`) back.
//!
//! The PHP control flow (insecure-URL check, redirect following, transport/status retries,
//! authenticated-retry detection, max_file_size enforcement, atomic rename of the `~` temp file)
//! is preserved. Per-request TLS/proxy/IP-resolve settings that reqwest only exposes per-Client
//! are simplified to a single default Client; see the TODOs below.
//!
//! TODO(phase-c): `abortRequest()` (PHP `CurlDownloader::abortRequest`, called from
//! `HttpDownloader.php:275` when a React\Promise consumer cancels a download) has no equivalent
//! here: shirabe has never ported the Promise/canceler machinery (`HttpDownloader::STATUS_ABORTED`
//! is likewise unused), and there is no job table left to cancel now that `download()` runs to
//! completion in one `.await`. Re-adding cancellation support belongs to whichever future task
//! ports that Promise-cancellation flow.

use crate::config::Config;
use crate::downloader::MaxFileSizeExceededException;
use crate::downloader::TransportException;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::util::HttpDownloader;
use crate::util::Platform;
use crate::util::Url;
use crate::util::http::CurlResponse;
use crate::util::http::ProxyManager;
use crate::util::http::Response;
use crate::util::{AuthHelper, PromptAuthResult, StoreAuth};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    PhpMixed, in_array, parse_url, preg_quote, rename, strpos, substr, unlink_silent,
};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug)]
pub struct CurlDownloader {
    /// Connection pool / cookie / TLS-session sharing — reqwest::Client handles this internally,
    /// replacing the PHP multiHandle + shareHandle. Redirects are disabled because we follow them
    /// manually (to control auth-header re-attachment), matching CURLOPT_FOLLOWLOCATION = false.
    client: reqwest::Client,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    auth_helper: std::rc::Rc<std::cell::RefCell<AuthHelper>>,
    max_redirects: i64,
    max_retries: i64,
}

/// Function-static `$timeoutWarning` from `tick()`.
static TIMEOUT_WARNING: AtomicBool = AtomicBool::new(false);

/// The outcome of one send attempt, decided by `decide()`. Replaces the PHP promise
/// resolve/reject callbacks (and this crate's former restart_job/resolve_job/reject_job side
/// effects): `download()`'s loop matches on this instead.
enum Decision {
    Retry { url: String, delay_ms: Option<u64> },
    Done(Response),
    Failed(TransportException),
}

impl CurlDownloader {
    /// @param mixed[] $options
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        _options: IndexMap<String, PhpMixed>,
        _disable_tls: bool,
    ) -> Self {
        // PHP set up the multi/share handle here (CURLMOPT_PIPELINING, MAX_HOST_CONNECTIONS,
        // CURLSHOPT_SHARE). reqwest folds all of that into the Client builder:
        //   - pool_max_idle_per_host(8) ~ CURLMOPT_MAX_HOST_CONNECTIONS = 8
        //   - cookie_store(true)        ~ CURL_LOCK_DATA_COOKIE
        //   - redirect(none)            ~ CURLOPT_FOLLOWLOCATION = false (we follow manually)
        // The libcurl version-specific multiplexing / accept-encoding workarounds are not needed.
        // TODO: a brand-new reqwest client is created per CurlDownloader; that is acceptable here
        // (one HttpDownloader owns one CurlDownloader) but not pooled across them.
        // TODO: cookie sharing (CURL_LOCK_DATA_COOKIE) would need reqwest's `cookies` feature
        // (.cookie_store(true)); omitted as it is not required for package downloads.
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(8)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            // The default builder cannot realistically fail; if the TLS backend is unavailable we
            // cannot proceed, mirroring PHP aborting when curl is missing.
            .expect("failed to build reqwest client for CurlDownloader");

        let auth_helper = std::rc::Rc::new(std::cell::RefCell::new(AuthHelper::new(
            io.clone(),
            config.clone(),
        )));

        Self {
            client,
            io,
            config,
            auth_helper,
            max_redirects: 20,
            max_retries: 3,
        }
    }

    /// @param mixed[]  $options
    /// @param non-empty-string $url
    ///
    /// Runs the request through the redirect/retry/status state machine until it resolves,
    /// mirroring what the PHP promise resolver + `tick()` loop used to do together.
    pub async fn download(
        &self,
        origin: &str,
        url: &str,
        mut options: IndexMap<String, PhpMixed>,
        copy_to: Option<&str>,
    ) -> anyhow::Result<Result<Response, TransportException>> {
        let mut attributes: IndexMap<String, PhpMixed> = {
            let mut m = IndexMap::new();
            m.insert("retryAuthFailure".to_string(), PhpMixed::Bool(true));
            m.insert("redirects".to_string(), PhpMixed::Int(0));
            m.insert("retries".to_string(), PhpMixed::Int(0));
            m.insert("storeAuth".to_string(), PhpMixed::Bool(false));
            m.insert("ipResolve".to_string(), PhpMixed::Null);
            m
        };
        if options.contains_key("retry-auth-failure") {
            attributes.insert(
                "retryAuthFailure".to_string(),
                options
                    .get("retry-auth-failure")
                    .cloned()
                    .unwrap_or(PhpMixed::Null),
            );
            options.shift_remove("retry-auth-failure");
        }

        if attributes
            .get("ipResolve")
            .map(|v| v.is_null())
            .unwrap_or(true)
            && Platform::get_env("COMPOSER_IPRESOLVE").as_deref() == Some("4")
        {
            attributes.insert("ipResolve".to_string(), PhpMixed::Int(4));
        } else if attributes
            .get("ipResolve")
            .map(|v| v.is_null())
            .unwrap_or(true)
            && Platform::get_env("COMPOSER_IPRESOLVE").as_deref() == Some("6")
        {
            attributes.insert("ipResolve".to_string(), PhpMixed::Int(6));
        }

        // check URL can be accessed (i.e. is not insecure), but allow insecure Packagist calls to
        // $hashed providers as file integrity is verified with sha256
        if !Preg::is_match(r"{^http://(repo\.)?packagist\.org/p/}", url)
            || (strpos(url, "$").is_none() && strpos(url, "%24").is_none())
        {
            self.config.borrow_mut().prohibit_url_by_config(
                url,
                Some(self.io.clone()),
                &options,
            )?;
        }

        // PHP added the auth options and ran StreamContextFactory::initOptions here, and would fail
        // up-front if the body temp file could not be opened. reqwest opens no file until
        // send_once(), so the auth/stream-context merge happens once per attempt inside the loop.

        let header_strings = Self::header_list(&options);
        let if_modified = if shirabe_php_shim::stripos(
            &shirabe_php_shim::implode(",", &header_strings),
            "if-modified-since:",
        )
        .is_some()
        {
            " if modified"
        } else {
            ""
        };
        // PHP logs the proxy in the "Downloading" line; resolving it here keeps that message
        // faithful even though reqwest does not yet apply the proxy (see send_once TODO).
        let using_proxy = ProxyManager::get_instance()
            .as_ref()
            .map(|pm| pm.get_proxy_for_request(url))
            .transpose()
            .map_err(|e| anyhow::anyhow!(e.message))?
            .and_then(|p| p.get_status(Some(" using proxy (%s)")).ok())
            .unwrap_or_default();
        // `attributes.redirects == 0 && attributes.retries == 0` in PHP is always true here since
        // this runs exactly once, before the retry loop below has a chance to advance either.
        self.io.write_error3(
            &format!(
                "Downloading {}{}{}",
                Url::sanitize(url.to_string()),
                using_proxy,
                if_modified
            ),
            true,
            crate::io::DEBUG,
        );

        let mut current_url = url.to_string();
        let mut current_origin = origin.to_string();
        loop {
            // PHP merges auth options + stream-context options at curl_setopt time. We need the
            // resulting header/method/content/timeout/ssl/max_file_size, so do it here per send.
            let send_options = self.auth_helper.borrow_mut().add_authentication_options(
                options.clone(),
                &current_origin,
                &current_url,
            )?;
            let send_options =
                crate::util::StreamContextFactory::init_options(&current_url, send_options, true)
                    .map_err(|e| anyhow::anyhow!(e.message))?;

            let send_result = self
                .send_once(&current_url, &send_options, copy_to, &attributes)
                .await;

            match self
                .decide(
                    send_result,
                    &current_url,
                    &current_origin,
                    copy_to,
                    &options,
                    &mut attributes,
                )
                .await?
            {
                Decision::Retry {
                    url: new_url,
                    delay_ms,
                } => {
                    if let Some(ms) = delay_ms {
                        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                    }
                    current_url = new_url;
                    current_origin = Url::get_origin(&self.config.borrow(), &current_url);
                    continue;
                }
                Decision::Done(response) => return Ok(Ok(response)),
                Decision::Failed(e) => return Ok(Err(e)),
            }
        }
    }

    /// Decides what to do with one send attempt's result: retry (transport error, auth-retry,
    /// redirect, retryable status code), fail, or succeed. Mirrors the body of the former
    /// `run_job` loop, minus the resolve/reject/restart side effects, which are now expressed as
    /// the returned `Decision`.
    async fn decide(
        &self,
        send_result: Result<RawResponse, TransportError>,
        url: &str,
        origin: &str,
        filename: Option<&str>,
        options: &IndexMap<String, PhpMixed>,
        attributes: &mut IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Decision> {
        let response = match send_result {
            Ok(resp) => resp,
            Err(transport_err) => {
                // CURLE_OPERATION_TIMEDOUT one-time warning (errno 28). reqwest cannot report
                // the curl errno, so this fires on the is_timeout() branch instead.
                if transport_err.was_timeout && !TIMEOUT_WARNING.load(Ordering::Relaxed) {
                    TIMEOUT_WARNING.store(true, Ordering::Relaxed);
                    self.io.write_error3(
                        "<warning>A connection timeout was encountered. If you intend to run Composer without connecting to the internet, run the command again prefixed with COMPOSER_DISABLE_NETWORK=1 to make Composer run in offline mode.</warning>",
                        true,
                        crate::io::NORMAL,
                    );
                }

                // PHP retried on a set of curl errnos (7/16/92/6/28 and 56/35 with "Connection
                // reset by peer"); reqwest does not expose those errnos, so approximate with
                // is_connect/is_timeout/is_request on GET requests.
                let retries = attributes
                    .get("retries")
                    .and_then(|v| v.as_int())
                    .unwrap_or(0);
                if transport_err.retryable
                    && Self::method_is_get(options)
                    && retries < self.max_retries
                {
                    attributes.insert("retries".to_string(), PhpMixed::Int(retries + 1));
                    // CURLE_COULDNT_CONNECT analogue: force IPv4 if no IP stack chosen.
                    if transport_err.is_connect && !attributes.contains_key("ipResolve") {
                        attributes.insert("ipResolve".to_string(), PhpMixed::Int(4));
                    }
                    self.io.write_error3(
                        &format!(
                            "Retrying ({}) {} due to connection error",
                            retries + 1,
                            Url::sanitize(url.to_string())
                        ),
                        true,
                        crate::io::DEBUG,
                    );
                    if let Some(filename) = filename {
                        unlink_silent(&format!("{}~", filename));
                    }
                    return Ok(Decision::Retry {
                        url: url.to_string(),
                        delay_ms: Self::retry_delay_ms(retries + 1),
                    });
                }

                if let Some(filename) = filename {
                    unlink_silent(&format!("{}~", filename));
                }
                // PHP throws a MaxFileSizeExceededException (a TransportException subclass) with
                // the raw "Maximum allowed download size reached..." message verbatim rather than
                // wrapping it in the generic curl-error text.
                if transport_err.is_max_file_size {
                    return Ok(Decision::Failed(
                        MaxFileSizeExceededException(TransportException::new(
                            transport_err.message,
                            0,
                        ))
                        .0,
                    ));
                }
                return Ok(Decision::Failed(TransportException::new(
                    format!(
                        "curl error while downloading {}: {}",
                        Url::sanitize(url.to_string()),
                        transport_err.message
                    ),
                    0,
                )));
            }
        };

        let status_code = response.status;
        let curl_response = response.into_curl_response(url);

        self.io.write_error3(
            &format!("[{}] {}", status_code, Url::sanitize(url.to_string())),
            true,
            crate::io::DEBUG,
        );

        // Output JSON warnings (PHP HttpDownloader::outputWarnings) for >= 300 JSON bodies.
        if curl_response.inner.get_status_code() >= 300
            && curl_response.inner.get_header("content-type").as_deref() == Some("application/json")
            && let Some(body) = curl_response.inner.get_body()
        {
            let decoded = shirabe_php_shim::json_decode(body, true)?;
            if let PhpMixed::Array(a) = decoded {
                HttpDownloader::output_warnings(self.io.clone(), origin, &a)?;
            }
        }

        // Authenticated-retry detection (401/403, Bitbucket login page, GitLab archive 404).
        let auth_result =
            self.is_authenticated_retry_needed(url, origin, filename, attributes, &curl_response)?;
        match auth_result {
            Ok(prompt) if prompt.retry => {
                attributes.insert(
                    "storeAuth".to_string(),
                    match prompt.store_auth {
                        StoreAuth::Bool(b) => PhpMixed::Bool(b),
                        StoreAuth::Prompt => PhpMixed::String("prompt".to_string()),
                    },
                );
                let retries = attributes
                    .get("retries")
                    .and_then(|v| v.as_int())
                    .unwrap_or(0);
                attributes.insert("retries".to_string(), PhpMixed::Int(retries + 1));
                if let Some(filename) = filename {
                    unlink_silent(&format!("{}~", filename));
                }
                return Ok(Decision::Retry {
                    url: url.to_string(),
                    delay_ms: None,
                });
            }
            Ok(_) => {}
            Err(e) => return Ok(Decision::Failed(e)),
        }

        // Handle 3xx redirects, 304 Not Modified excluded.
        let redirects = attributes
            .get("redirects")
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        if (300..=399).contains(&status_code)
            && status_code != 304
            && redirects < self.max_redirects
        {
            match self.handle_redirect(url, attributes, &curl_response)? {
                Ok(location) if !location.is_empty() => {
                    attributes.insert("redirects".to_string(), PhpMixed::Int(redirects + 1));
                    if let Some(filename) = filename {
                        unlink_silent(&format!("{}~", filename));
                    }
                    return Ok(Decision::Retry {
                        url: location,
                        delay_ms: None,
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    if let Some(filename) = filename {
                        unlink_silent(&format!("{}~", filename));
                    }
                    return Ok(Decision::Failed(e));
                }
            }
        }

        // Fail 4xx and 5xx responses (some are retried on GET).
        if (400..=599).contains(&status_code) {
            let retries = attributes
                .get("retries")
                .and_then(|v| v.as_int())
                .unwrap_or(0);
            if Self::method_is_get(options)
                && in_array(
                    PhpMixed::Int(status_code),
                    &PhpMixed::List(
                        [423, 425, 500, 502, 503, 504, 507, 510]
                            .iter()
                            .map(|c| PhpMixed::Int(*c))
                            .collect(),
                    ),
                    true,
                )
                && retries < self.max_retries
            {
                self.io.write_error3(
                    &format!(
                        "Retrying ({}) {} due to status code {}",
                        retries + 1,
                        Url::sanitize(url.to_string()),
                        status_code
                    ),
                    true,
                    crate::io::DEBUG,
                );
                attributes.insert("retries".to_string(), PhpMixed::Int(retries + 1));
                if let Some(filename) = filename {
                    unlink_silent(&format!("{}~", filename));
                }
                return Ok(Decision::Retry {
                    url: url.to_string(),
                    delay_ms: Self::retry_delay_ms(retries + 1),
                });
            }

            let status_msg = curl_response.inner.get_status_message().unwrap_or_default();
            // Enrich with the response headers/status/body, mirroring PHP's catch block that
            // calls setHeaders/setStatusCode/setResponse before reject().
            let mut e = self.fail_response(url, filename, &curl_response, &status_msg);
            e.set_headers(curl_response.inner.get_headers().clone());
            e.set_status_code(Some(curl_response.inner.get_status_code()));
            e.set_response(curl_response.inner.get_body().map(|s| s.to_string()));
            return Ok(Decision::Failed(e));
        }

        // storeAuth on success.
        let store_auth = attributes.get("storeAuth").cloned();
        if !matches!(store_auth, Some(PhpMixed::Bool(false))) {
            let store_auth = match store_auth {
                Some(PhpMixed::String(ref s)) if s == "prompt" => StoreAuth::Prompt,
                Some(PhpMixed::Bool(b)) => StoreAuth::Bool(b),
                _ => StoreAuth::Bool(false),
            };
            self.auth_helper.borrow().store_auth(origin, store_auth)?;
        }

        // Atomic rename of the `~` temp file to its final name (file mode).
        if let Some(filename) = filename {
            rename(format!("{}~", filename), filename);
        }

        Ok(Decision::Done(curl_response.inner))
    }

    /// Performs one non-blocking HTTP request via reqwest, enforcing max_file_size and streaming
    /// the body to the `~` temp file when in file mode. Replaces PHP's curl_setopt block + curl I/O.
    async fn send_once(
        &self,
        url: &str,
        options: &IndexMap<String, PhpMixed>,
        filename: Option<&str>,
        attributes: &IndexMap<String, PhpMixed>,
    ) -> Result<RawResponse, TransportError> {
        let http = options.get("http").and_then(|v| v.as_array());
        let method = http
            .and_then(|h| h.get("method"))
            .and_then(|v| v.as_string())
            .unwrap_or("GET")
            .to_string();
        let body = http
            .and_then(|h| h.get("content"))
            .and_then(|v| v.as_string())
            .map(|s| s.as_bytes().to_vec());
        let timeout_secs = http
            .and_then(|h| h.get("timeout"))
            .and_then(|v| v.as_int())
            .unwrap_or(0)
            .max(300);
        let headers = Self::header_list(options);
        let max_file_size = options
            .get("max_file_size")
            .and_then(|v| v.as_int())
            .map(|n| n as u64);

        // TODO: per-request ssl (cafile/verify_peer/local_cert) and proxy settings are reqwest
        // Client-level, not request-level. They are not applied here yet; a ConnectionOptions-keyed
        // Client cache (as in the design sketch) is required to honor them.
        // TODO: CURLOPT_IPRESOLVE (force IPv4/IPv6) has no direct reqwest API.
        let _ = attributes;

        let reqwest_method =
            reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET);

        let mut builder = self.client.request(reqwest_method, url);
        for header in &headers {
            if let Some((name, value)) = header.split_once(':') {
                // Skip the Connection header reqwest manages itself.
                let name = name.trim();
                if name.eq_ignore_ascii_case("connection") {
                    continue;
                }
                builder = builder.header(name, value.trim());
            }
        }
        builder = builder.timeout(std::time::Duration::from_secs(timeout_secs as u64));
        if let Some(body) = body {
            builder = builder.body(body);
        }

        let resp = builder.send().await.map_err(|e| TransportError {
            message: e.to_string(),
            retryable: e.is_timeout() || e.is_connect() || e.is_request(),
            is_connect: e.is_connect(),
            was_timeout: e.is_timeout(),
            is_max_file_size: false,
        })?;

        let status = resp.status().as_u16() as i64;
        let status_line = format!(
            "HTTP/{} {}",
            match resp.version() {
                reqwest::Version::HTTP_09 => "0.9",
                reqwest::Version::HTTP_10 => "1.0",
                reqwest::Version::HTTP_11 => "1.1",
                reqwest::Version::HTTP_2 => "2",
                reqwest::Version::HTTP_3 => "3",
                _ => "1.1",
            },
            resp.status()
        );
        // Header lines start with the status line so Response::get_status_message can find it.
        let mut headers_out: Vec<String> = vec![status_line];
        for (k, v) in resp.headers().iter() {
            headers_out.push(format!("{}: {}", k, v.to_str().unwrap_or("")));
        }

        let body = Self::read_body_with_limit(resp, max_file_size, filename)
            .await
            .map_err(|(message, is_max_file_size)| TransportError {
                message,
                retryable: false,
                is_connect: false,
                was_timeout: false,
                is_max_file_size,
            })?;

        Ok(RawResponse {
            status,
            headers: headers_out,
            body,
        })
    }

    /// Reads the body, enforcing max_file_size, writing to the `~` temp file when in file mode.
    /// The `bool` in the error is `true` when the failure is a max_file_size violation.
    async fn read_body_with_limit(
        mut resp: reqwest::Response,
        max_file_size: Option<u64>,
        filename: Option<&str>,
    ) -> Result<Body, (String, bool)> {
        use tokio::io::AsyncWriteExt as _;

        let mut written: u64 = 0;
        enum Sink {
            File(tokio::fs::File),
            Memory(Vec<u8>),
        }
        let mut sink = match filename {
            Some(f) => Sink::File(
                tokio::fs::File::create(format!("{}~", f))
                    .await
                    .map_err(|e| (e.to_string(), false))?,
            ),
            None => Sink::Memory(Vec::new()),
        };

        loop {
            let chunk = match resp.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(e) => return Err((e.to_string(), false)),
            };
            written += chunk.len() as u64;
            if let Some(max) = max_file_size
                && written > max
            {
                return Err((
                    format!(
                        "Maximum allowed download size reached. Downloaded {} of allowed {} bytes",
                        written, max
                    ),
                    true,
                ));
            }
            match &mut sink {
                Sink::File(f) => f
                    .write_all(&chunk)
                    .await
                    .map_err(|e| (e.to_string(), false))?,
                Sink::Memory(buf) => buf.extend_from_slice(&chunk),
            }
        }

        Ok(match sink {
            Sink::File(_) => Body::File,
            Sink::Memory(buf) => Body::Memory(buf),
        })
    }

    fn handle_redirect(
        &self,
        url: &str,
        attributes: &IndexMap<String, PhpMixed>,
        response: &CurlResponse,
    ) -> anyhow::Result<Result<String, TransportException>> {
        let mut target_url = String::new();
        if let Some(location_header) = response.inner.get_header("location")
            && !location_header.is_empty()
        {
            if !parse_url(&location_header, shirabe_php_shim::PHP_URL_SCHEME).is_null() {
                // Absolute URL; e.g. https://example.com/composer
                target_url = location_header.clone();
            } else if !parse_url(&location_header, shirabe_php_shim::PHP_URL_HOST).is_null() {
                // Scheme relative; e.g. //example.com/foo
                target_url = format!(
                    "{}:{}",
                    parse_url(url, shirabe_php_shim::PHP_URL_SCHEME)
                        .as_string()
                        .unwrap_or(""),
                    location_header
                );
            } else if location_header.starts_with('/') {
                // Absolute path; e.g. /foo
                let url_host = parse_url(url, shirabe_php_shim::PHP_URL_HOST);
                let url_host_str = url_host.as_string().unwrap_or("");
                target_url = Preg::replace(
                    &format!(
                        r"{{^(.+(?://|@){}(?::\d+)?)(?:[/\?].*)?$}}",
                        preg_quote(url_host_str, None)
                    ),
                    &format!("\\1{}", location_header),
                    url,
                );
            } else {
                // Relative path; e.g. foo
                target_url = Preg::replace(
                    r"{^(.+/)[^/?]*(?:\?.*)?$}",
                    &format!("\\1{}", location_header),
                    url,
                );
            }
        }

        if !target_url.is_empty() {
            self.io.write_error3(
                &format!(
                    "Following redirect ({}) {}",
                    attributes
                        .get("redirects")
                        .and_then(|v| v.as_int())
                        .unwrap_or(0)
                        + 1,
                    Url::sanitize(target_url.clone()),
                ),
                true,
                crate::io::DEBUG,
            );

            return Ok(Ok(target_url));
        }

        Ok(Err(TransportException::new(
            format!(
                "The \"{}\" file could not be downloaded, got redirect without Location ({})",
                url,
                response.inner.get_status_message().unwrap_or_default()
            ),
            0,
        )))
    }

    fn is_authenticated_retry_needed(
        &self,
        url: &str,
        origin: &str,
        filename: Option<&str>,
        attributes: &IndexMap<String, PhpMixed>,
        response: &CurlResponse,
    ) -> anyhow::Result<Result<PromptAuthResult, TransportException>> {
        let retry_auth_failure = attributes
            .get("retryAuthFailure")
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        let retries = attributes
            .get("retries")
            .and_then(|b| b.as_int())
            .unwrap_or(0);

        if in_array(
            PhpMixed::Int(response.inner.get_status_code()),
            &PhpMixed::List(vec![PhpMixed::Int(401), PhpMixed::Int(403)]),
            false,
        ) && retry_auth_failure
        {
            let status_message = response.inner.get_status_message();
            let body = response.inner.get_body().map(|s| s.to_string());
            let result = self.auth_helper.borrow_mut().prompt_auth_if_needed(
                url,
                origin,
                response.inner.get_status_code(),
                status_message.as_deref(),
                response.inner.get_headers().clone(),
                retries,
                body.as_deref(),
            )?;

            if result.retry {
                return Ok(Ok(result));
            }
        }

        let location_header = response.inner.get_header("location");
        let mut needs_auth_retry: Option<&'static str> = None;

        // check for bitbucket login page asking to authenticate
        if origin == "bitbucket.org"
            && !self.auth_helper.borrow().is_public_bit_bucket_download(url)
            && substr(url, -4, None) == ".zip"
            && (location_header.is_none()
                || substr(location_header.as_deref().unwrap_or(""), -4, None) != ".zip")
            && Preg::is_match(
                r"{^text/html\b}i",
                &response
                    .inner
                    .get_header("content-type")
                    .unwrap_or_default(),
            )
        {
            needs_auth_retry = Some("Bitbucket requires authentication and it was not provided");
        }

        // check for gitlab 404 when downloading archives
        let gitlab_domains = self.config.borrow_mut().get("gitlab-domains");
        let gitlab_domains_list: Vec<PhpMixed> = match gitlab_domains {
            PhpMixed::List(l) => l,
            _ => Vec::new(),
        };
        if response.inner.get_status_code() == 404
            && in_array(
                PhpMixed::String(origin.to_string()),
                &PhpMixed::List(gitlab_domains_list),
                true,
            )
            && strpos(url, "archive.zip").is_some()
        {
            needs_auth_retry = Some("GitLab requires authentication and it was not provided");
        }

        if let Some(msg) = needs_auth_retry {
            if retry_auth_failure {
                let result = self.auth_helper.borrow_mut().prompt_auth_if_needed(
                    url,
                    origin,
                    401,
                    None,
                    Vec::new(),
                    retries,
                    None,
                )?;
                if result.retry {
                    return Ok(Ok(result));
                }
            }

            return Ok(Err(self.fail_response(url, filename, response, msg)));
        }

        Ok(Ok(PromptAuthResult {
            retry: false,
            store_auth: StoreAuth::Bool(false),
        }))
    }

    /// The delay `restart_job_with_delay` used to sleep before restarting a retried job: half a
    /// second from the 3rd retry onward, 100ms for the 2nd, none for the 1st. `retries` is the
    /// post-increment retry count.
    fn retry_delay_ms(retries: i64) -> Option<u64> {
        if retries >= 3 {
            Some(500)
        } else if retries >= 2 {
            Some(100)
        } else {
            None
        }
    }

    fn fail_response(
        &self,
        url: &str,
        filename: Option<&str>,
        response: &CurlResponse,
        error_message: &str,
    ) -> TransportException {
        if let Some(filename) = filename {
            unlink_silent(&format!("{}~", filename));
        }

        let mut details = String::new();
        if in_array(
            PhpMixed::String(
                response
                    .inner
                    .get_header("content-type")
                    .unwrap_or_default()
                    .to_lowercase(),
            ),
            &PhpMixed::List(vec![
                PhpMixed::String("application/json".to_string()),
                PhpMixed::String("application/json; charset=utf-8".to_string()),
            ]),
            true,
        ) {
            let body = response.inner.get_body().unwrap_or("");
            details = format!(
                ":{}{}{}",
                shirabe_php_shim::PHP_EOL,
                substr(body, 0, Some(200)),
                if shirabe_php_shim::strlen(body) > 200 {
                    "..."
                } else {
                    ""
                }
            );
        }

        TransportException::new(
            format!(
                "The \"{}\" file could not be downloaded ({}){}",
                url, error_message, details
            ),
            response.inner.get_status_code(),
        )
    }

    fn method_is_get(options: &IndexMap<String, PhpMixed>) -> bool {
        let method = options
            .get("http")
            .and_then(|v| v.as_array())
            .and_then(|h| h.get("method"))
            .and_then(|v| v.as_string());
        match method {
            None => true,
            Some(m) => m == "GET",
        }
    }

    /// Extracts `options['http']['header']` as a `Vec<String>`.
    fn header_list(options: &IndexMap<String, PhpMixed>) -> Vec<String> {
        options
            .get("http")
            .and_then(|v| v.as_array())
            .and_then(|h| h.get("header"))
            .and_then(|v| match v {
                PhpMixed::List(l) => Some(
                    l.iter()
                        .filter_map(|x| x.as_string().map(|s| s.to_string()))
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }
}

/// A transport-layer failure approximating the curl errno branches PHP inspected.
struct TransportError {
    message: String,
    retryable: bool,
    is_connect: bool,
    was_timeout: bool,
    /// The failure is a max_file_size violation (PHP MaxFileSizeExceededException), not a curl error.
    is_max_file_size: bool,
}

/// Raw send result before conversion to `CurlResponse`.
struct RawResponse {
    status: i64,
    headers: Vec<String>,
    body: Body,
}

enum Body {
    /// Body read into memory.
    Memory(Vec<u8>),
    /// Body streamed to the `~` temp file (contents reference the final filename via the job).
    File,
}

impl RawResponse {
    fn into_curl_response(self, url: &str) -> CurlResponse {
        let body = match self.body {
            Body::Memory(b) => Some(String::from_utf8_lossy(&b).into_owned()),
            // File mode: PHP stores `$filename.'~'` as the contents string; the actual bytes live
            // on disk. We do not have the filename here, so leave the body empty — the caller's
            // rename/read handling does not consult the body for successful file downloads.
            Body::File => None,
        };
        CurlResponse::new(
            url.to_string(),
            Some(self.status),
            self.headers,
            body,
            IndexMap::new(),
        )
    }
}
