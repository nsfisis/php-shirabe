//! ref: composer/src/Composer/Util/Http/CurlDownloader.php sha1:9f36e0e6628dcf49f7ed709364d4396f85d20d07
//!
//! The downloader `HttpDownloader` hands http(s) requests to, in the place of Composer's
//! `CurlDownloader`. It performs the requests in-process over reqwest instead of libcurl, so it
//! works whether or not the PHP runtime has the curl extension. Plugin code that names
//! `CurlDownloader` itself gets the real PHP class in the worker, not this one.
//!
//! Unlike the libcurl multi-handle event loop of the PHP original, `download()` is a single `async fn`: it sends the request, hands the result to
//! `decide()` (retry / redirect / fail / succeed), and loops until it resolves. There is no job
//! table or `tick()` driver anymore; the caller `.await`s `download()` directly and gets the
//! final `Response` (or `TransportException`) back.
//!
//! The PHP control flow (insecure-URL check, redirect following, transport/status retries,
//! authenticated-retry detection, max_file_size enforcement, atomic rename of the `~` temp file)
//! is preserved. The TLS, proxy and IP-resolve settings PHP sets on each curl handle are taken by
//! reqwest only per Client, so a request is sent through a Client built for its settings.
//!
//! TODO(async): PHP's `CurlDownloader::abortRequest()`, which `HttpDownloader`'s canceler calls,
//! has no counterpart here. A request is cancelled by dropping the `download()` future instead, which
//! closes the connection and the file handle but skips the `@unlink($filename.'~')` PHP does, so
//! a cancelled file download leaves its partial `~` file behind.

use crate::config::Config;
use crate::downloader::{MaxFileSizeExceededException, TransportException};
use crate::io::{DEBUG, IOInterface, IOInterfaceWrite};
use crate::util::http::{NativeHttpResponse, ProxyManager, Response};
use crate::util::{
    AuthHelper, HttpDownloader, HttpOptions, Platform, PromptAuthResult, SslOptions, StoreAuth,
    StreamContextFactory, StreamContextOptions, Url,
};
use bstr::{B, BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_shim::{
    CURLOPT_PROXY, CURLOPT_PROXYUSERPWD, PHP_EOL, PhpMixed, RuntimeException, ToPhpString,
    TypeError, base64_encode, bformat, boolval, display_to_bytes, get_debug_type, in_array_loose,
    in_array_strict, intval, json_decode_assoc, parse_url, path_from_bytes, path_to_bytes,
    php_regex, preg_is_match, preg_quote, preg_replace, rawurldecode, rename, stripos, strlen,
    strpos, strtolower, substr, trim, unlink_silent, utf8_text,
};

#[derive(Debug)]
pub struct NativeHttpDownloader {
    io: std::rc::Rc<dyn IOInterface>,
    config: std::rc::Rc<Config>,
    auth_helper: std::cell::RefCell<AuthHelper>,
    max_redirects: i64,
    max_retries: i64,
}

/// Function-static `$timeoutWarning` from `tick()`.
static TIMEOUT_WARNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The outcome of one send attempt, decided by `decide()`. Replaces the PHP promise
/// resolve/reject callbacks (and this crate's former restart_job/resolve_job/reject_job side
/// effects): `download()`'s loop matches on this instead.
enum Decision {
    Retry { url: BString, delay_ms: Option<u64> },
    Done(Response),
    Failed(anyhow::Error),
}

/// The settings PHP sets on each curl handle that reqwest only takes per Client: the `ssl` options
/// of `CurlDownloader::$options`, the proxy of `RequestProxy::getCurlOptions()` and
/// CURLOPT_IPRESOLVE.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ConnectionOptions {
    cafile: Option<std::path::PathBuf>,
    capath: Option<std::path::PathBuf>,
    verify_peer: bool,
    verify_peer_name: bool,
    local_cert: Option<std::path::PathBuf>,
    local_pk: Option<std::path::PathBuf>,
    /// CURLOPT_PROXY, `None` for the empty string (no proxy).
    proxy: Option<BString>,
    /// CURLOPT_PROXYUSERPWD.
    proxy_auth: Option<BString>,
    /// CURLOPT_IPRESOLVE: 4 or 6.
    ip_resolve: Option<i64>,
}

/// The reqwest Client to send a request with the given connection settings on the calling thread's
/// tokio runtime.
///
/// Clients are cached by their settings, so every NativeHttpDownloader a run builds shares them and
/// their connection pools. PHP instead hands each CurlDownloader its own curl multi handle.
///
/// A pooled connection is driven by a task on the runtime that opened it and hangs if it is later
/// handed to another one, so the Clients are cached per runtime: `main.rs` enters one runtime for
/// the whole process, while the disposable per-call runtimes `sync_executor::block_on` falls back
/// to outside `main` get their own.
fn runtime_client(connection: &ConnectionOptions) -> Result<reqwest::Client, BString> {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return build_client(connection);
    };
    static CACHED: std::sync::Mutex<
        Option<(
            tokio::runtime::Id,
            IndexMap<ConnectionOptions, reqwest::Client>,
        )>,
    > = std::sync::Mutex::new(None);
    let mut cached = CACHED.lock().unwrap();
    if cached.as_ref().is_none_or(|(id, _)| *id != handle.id()) {
        *cached = Some((handle.id(), IndexMap::new()));
    }
    let (_, clients) = cached.as_mut().unwrap();
    if let Some(client) = clients.get(connection) {
        return Ok(client.clone());
    }
    let client = build_client(connection)?;
    clients.insert(connection.clone(), client.clone());
    Ok(client)
}

fn build_client(connection: &ConnectionOptions) -> Result<reqwest::Client, BString> {
    let mut builder = reqwest::Client::builder()
        .pool_max_idle_per_host(8)
        // Redirects are followed manually (to control auth-header re-attachment), matching
        // CURLOPT_FOLLOWLOCATION = false.
        .redirect(reqwest::redirect::Policy::none())
        // CURLOPT_ENCODING = "": advertise and transparently decode every encoding curl
        // supports. The body is decoded before read_body_with_limit sees it, so max_file_size
        // applies to the decoded size, as in PHP.
        .brotli(true)
        .deflate(true)
        .gzip(true)
        .zstd(true)
        // PHP always sets CURLOPT_PROXY, even to an empty string, so curl ignores the proxy
        // environment variables: ProxyManager has already read them.
        .no_proxy();

    if connection.verify_peer {
        // CURLOPT_CAINFO and CURLOPT_CAPATH replace curl's default CA store.
        if connection.cafile.is_some() || connection.capath.is_some() {
            let mut roots = Vec::new();
            if let Some(cafile) = &connection.cafile {
                roots.extend(read_pem_certificates(cafile)?);
            }
            if let Some(capath) = &connection.capath {
                roots.extend(read_capath(capath)?);
            }
            builder = builder.tls_certs_only(roots);
        }
        if !connection.verify_peer_name {
            // TODO(http): rustls skips only the host name check against explicitly given roots,
            // so without a cafile or capath the Client fails to build.
            builder = builder.tls_danger_accept_invalid_hostnames(true);
        }
    } else {
        // TODO(http): curl still checks the host name when only CURLOPT_SSL_VERIFYPEER is off,
        // while reqwest skips every check of the server certificate.
        builder = builder.tls_danger_accept_invalid_certs(true);
    }

    // curl reads the private key from the CURLOPT_SSLCERT file unless CURLOPT_SSLKEY names another.
    // TODO(http): `ssl.passphrase` (CURLOPT_SSLKEYPASSWD) is not applied: an encrypted private
    // key cannot be read.
    if let Some(local_cert) = &connection.local_cert {
        let mut pem = read_file(local_cert)?;
        if let Some(local_pk) = &connection.local_pk {
            pem.push(b'\n');
            pem.extend(read_file(local_pk)?);
        }
        builder = builder.identity(reqwest::Identity::from_pem(&pem).map_err(|e| error_chain(&e))?);
    }

    // TODO(http): reqwest verifies an `https://` proxy with the same settings as the server, while
    // curl keeps verifying the proxy (CURLOPT_PROXY_SSL_VERIFYPEER) when `ssl.verify_peer` is off.
    if let Some(proxy_url) = &connection.proxy {
        let proxy_url = utf8_text(proxy_url).ok_or_else(|| {
            display_to_bytes(format_args!("proxy URL is not valid UTF-8: {proxy_url:?}"))
        })?;
        let mut proxy = reqwest::Proxy::all(proxy_url).map_err(|e| error_chain(&e))?;
        if let Some(auth) = &connection.proxy_auth {
            // curl splits CURLOPT_PROXYUSERPWD at the first colon and URL-decodes both halves.
            let (user, password) = auth.split_once_str(":").unwrap_or((auth.as_slice(), b""));
            let credentials = bformat!("{}:{}", rawurldecode(user), rawurldecode(password));
            let header = reqwest::header::HeaderValue::from_bytes(&bformat!(
                "Basic {}",
                base64_encode(credentials)
            ))
            .map_err(|e| display_to_bytes(&e))?;
            proxy = proxy.custom_http_auth(header);
        }
        builder = builder.proxy(proxy);
    }

    // Binding the unspecified address of one family makes the connector try only the addresses of
    // that family, which is what CURL_IPRESOLVE_V4 / CURL_IPRESOLVE_V6 do.
    match connection.ip_resolve {
        Some(4) => {
            builder = builder.local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        }
        Some(6) => {
            builder = builder.local_address(std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED));
        }
        _ => {}
    }

    builder.build().map_err(|e| error_chain(&e))
}

fn read_file(path: &std::path::Path) -> Result<Vec<u8>, BString> {
    std::fs::read(path)
        .map_err(|e| display_to_bytes(format_args!("could not read {}: {e}", path.display())))
}

fn read_pem_certificates(path: &std::path::Path) -> Result<Vec<reqwest::Certificate>, BString> {
    reqwest::Certificate::from_pem_bundle(&read_file(path)?).map_err(|e| {
        bformat!(
            "could not load certificates from {}: {}",
            path_to_bytes(path),
            error_chain(&e)
        )
    })
}

/// OpenSSL finds a CA in a capath by its subject-hash file name (`<8 hex digits>.<n>`), so the
/// files named that way hold every certificate it can find there.
fn read_capath(capath: &std::path::Path) -> Result<Vec<reqwest::Certificate>, BString> {
    let entries = std::fs::read_dir(capath)
        .map_err(|e| display_to_bytes(format_args!("could not read {}: {e}", capath.display())))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            display_to_bytes(format_args!("could not read {}: {e}", capath.display()))
        })?;
        let name = entry.file_name();
        let is_hash_name = name
            .as_encoded_bytes()
            .split_once_str(".")
            .is_some_and(|(hash, n)| {
                hash.len() == 8
                    && hash.iter().all(u8::is_ascii_hexdigit)
                    && !n.is_empty()
                    && n.iter().all(u8::is_ascii_digit)
            });
        if is_hash_name {
            paths.push(entry.path());
        }
    }
    paths.sort();
    let mut certs = Vec::new();
    for path in paths {
        certs.extend(read_pem_certificates(&path)?);
    }
    Ok(certs)
}

/// The message of an error followed by those of its sources, which hold the detail of a reqwest
/// builder error.
fn error_chain(error: &dyn std::error::Error) -> BString {
    let mut message = display_to_bytes(error);
    let mut source = error.source();
    while let Some(e) = source {
        message.extend_from_slice(b": ");
        message.extend_from_slice(&display_to_bytes(e));
        source = e.source();
    }
    message
}

impl NativeHttpDownloader {
    pub fn new(
        io: std::rc::Rc<dyn IOInterface>,
        config: std::rc::Rc<Config>,
        _options: StreamContextOptions,
        _disable_tls: bool,
    ) -> Self {
        // PHP set up the multi/share handle here (CURLMOPT_PIPELINING, MAX_HOST_CONNECTIONS,
        // CURLSHOPT_SHARE). reqwest folds all of that into the Client builder:
        //   - pool_max_idle_per_host(8) ~ CURLMOPT_MAX_HOST_CONNECTIONS = 8
        //   - cookie_store(true)        ~ CURL_LOCK_DATA_COOKIE
        //   - redirect(none)            ~ CURLOPT_FOLLOWLOCATION = false (we follow manually)
        // The libcurl version-specific multiplexing / accept-encoding workarounds are not needed.
        // TODO(http): cookie sharing (CURL_LOCK_DATA_COOKIE) would need reqwest's `cookies` feature
        // (.cookie_store(true)); omitted as it is not required for package downloads.

        let auth_helper = std::cell::RefCell::new(AuthHelper::new(io.clone(), config.clone()));

        Self {
            io,
            config,
            auth_helper,
            max_redirects: 20,
            max_retries: 3,
        }
    }

    /// Runs the request through the redirect/retry/status state machine until it resolves,
    /// mirroring what the PHP promise resolver + `tick()` loop used to do together.
    pub async fn download(
        &self,
        origin: &BStr,
        url: &BStr,
        mut options: StreamContextOptions,
        copy_to: Option<&std::path::Path>,
    ) -> anyhow::Result<Result<Response, anyhow::Error>> {
        let mut attributes: IndexMap<BString, PhpMixed> = {
            let mut m = IndexMap::new();
            m.insert("retryAuthFailure".into(), PhpMixed::Bool(true));
            m.insert("redirects".into(), PhpMixed::Int(0));
            m.insert("retries".into(), PhpMixed::Int(0));
            m.insert("storeAuth".into(), PhpMixed::Bool(false));
            m.insert("ipResolve".into(), PhpMixed::Null);
            m
        };
        if options.isset("retry-auth-failure")
            && let Some(retry_auth_failure) = options.remove("retry-auth-failure")
        {
            attributes.insert("retryAuthFailure".into(), retry_auth_failure);
        }

        if attributes
            .get(B("ipResolve"))
            .map(|v| v.is_null())
            .unwrap_or(true)
            && Platform::get_env("COMPOSER_IPRESOLVE").is_some_and(|v| v == "4")
        {
            attributes.insert("ipResolve".into(), PhpMixed::Int(4));
        } else if attributes
            .get(B("ipResolve"))
            .map(|v| v.is_null())
            .unwrap_or(true)
            && Platform::get_env("COMPOSER_IPRESOLVE").is_some_and(|v| v == "6")
        {
            attributes.insert("ipResolve".into(), PhpMixed::Int(6));
        }

        // check URL can be accessed (i.e. is not insecure), but allow insecure Packagist calls to
        // $hashed providers as file integrity is verified with sha256
        if !preg_is_match(php_regex!(r"{^http://(repo\.)?packagist\.org/p/}"), url)
            || (strpos(url, "$").is_none() && strpos(url, "%24").is_none())
        {
            self.config
                .prohibit_url_by_config(url, Some(self.io.clone()), &options.to_array())?;
        }

        // PHP added the auth options and ran StreamContextFactory::initOptions here, and would fail
        // up-front if the body temp file could not be opened. reqwest opens no file until
        // send_once(), so the auth/stream-context merge happens once per attempt inside the loop.

        // PHP runs `array_diff()` on a set header before anything converts it.
        if let Some(header) = options.http().and_then(HttpOptions::header)
            && !matches!(header, PhpMixed::List(_) | PhpMixed::Array(_))
        {
            return Err(TypeError::new(bformat!(
                "array_diff(): Argument #1 ($array) must be of type array, {} given",
                get_debug_type(header)
            ))
            .into());
        }
        let if_modified = match options.http() {
            Some(http)
                if http.header().is_some()
                    && stripos(http.implode_header(",")?, "if-modified-since:").is_some() =>
            {
                " if modified"
            }
            _ => "",
        };
        let using_proxy = ProxyManager::get_instance()
            .as_ref()
            .map(|pm| pm.get_proxy_for_request(url))
            .transpose()?
            .and_then(|p| p.get_status(Some(BStr::new(" using proxy (%s)"))).ok())
            .unwrap_or_default();
        // `attributes.redirects == 0 && attributes.retries == 0` in PHP is always true here since
        // this runs exactly once, before the retry loop below has a chance to advance either.
        self.io.write_error_with_verbosity(
            bformat!(
                "Downloading {}{using_proxy}{if_modified}",
                Url::sanitize(url)
            ),
            DEBUG,
        );

        let mut current_url = url.to_owned();
        let mut current_origin = origin.to_owned();
        loop {
            // PHP merges auth options + stream-context options at curl_setopt time. We need the
            // resulting header/method/content/timeout/ssl/max_file_size, so do it here per send.
            let send_options = self.auth_helper.borrow_mut().add_authentication_options(
                options.clone(),
                current_origin.as_bstr(),
                current_url.as_bstr(),
            )?;
            let send_options =
                StreamContextFactory::init_options(&current_url, send_options, true)??;

            let request = Self::curl_request_options(&send_options)?;
            let connection =
                Self::connection_options(current_url.as_bstr(), &send_options, &attributes)?;
            let send_result = self
                .send_once(current_url.as_bstr(), &request, &connection, copy_to)
                .await;

            match self
                .decide(
                    send_result,
                    current_url.as_bstr(),
                    current_origin.as_bstr(),
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
                    current_origin = Url::get_origin(&self.config, &current_url)?;
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
        url: &BStr,
        origin: &BStr,
        filename: Option<&std::path::Path>,
        options: &StreamContextOptions,
        attributes: &mut IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<Decision> {
        let response = match send_result {
            Ok(resp) => resp,
            Err(transport_err) => {
                // CURLE_OPERATION_TIMEDOUT one-time warning (errno 28). reqwest cannot report
                // the curl errno, so this fires on the is_timeout() branch instead.
                if transport_err.was_timeout
                    && !TIMEOUT_WARNING.load(std::sync::atomic::Ordering::Relaxed)
                {
                    TIMEOUT_WARNING.store(true, std::sync::atomic::Ordering::Relaxed);
                    self.io.write_error("<warning>A connection timeout was encountered. If you intend to run Composer without connecting to the internet, run the command again prefixed with COMPOSER_DISABLE_NETWORK=1 to make Composer run in offline mode.</warning>");
                }

                // PHP retried on a set of curl errnos (7/16/92/6/28 and 56/35 with "Connection
                // reset by peer"); reqwest does not expose those errnos, so approximate with
                // is_connect/is_timeout/is_request on GET requests.
                let retries = attributes
                    .get(B("retries"))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0);
                if transport_err.retryable
                    && Self::method_is_get(options)
                    && retries < self.max_retries
                {
                    attributes.insert("retries".into(), PhpMixed::Int(retries + 1));
                    // CURLE_COULDNT_CONNECT analogue: force IPv4 if no IP stack chosen.
                    if transport_err.is_connect && !attributes.contains_key(B("ipResolve")) {
                        attributes.insert("ipResolve".into(), PhpMixed::Int(4));
                    }
                    self.io.write_error_with_verbosity(
                        bformat!(
                            "Retrying ({}) {} due to connection error",
                            retries + 1,
                            Url::sanitize(url)
                        ),
                        DEBUG,
                    );
                    if let Some(filename) = filename {
                        unlink_silent(Self::temp_file(filename));
                    }
                    return Ok(Decision::Retry {
                        url: url.to_owned(),
                        delay_ms: Self::retry_delay_ms(retries + 1),
                    });
                }

                if let Some(filename) = filename {
                    unlink_silent(Self::temp_file(filename));
                }
                // The message carries the raw "Maximum allowed download size reached..." text
                // rather than the generic curl-error wrapper used below.
                if transport_err.is_max_file_size {
                    return Ok(Decision::Failed(
                        MaxFileSizeExceededException::new(transport_err.message).into(),
                    ));
                }
                return Ok(Decision::Failed(
                    TransportException::new(
                        bformat!(
                            "curl error while downloading {}: {}",
                            Url::sanitize(url),
                            transport_err.message
                        ),
                        0,
                    )
                    .into(),
                ));
            }
        };

        let status_code = response.status;
        let native_response = response.into_native_http_response(url);

        self.io
            .write_error_with_verbosity(bformat!("[{status_code}] {}", Url::sanitize(url)), DEBUG);

        // Output JSON warnings (PHP HttpDownloader::outputWarnings) for >= 300 JSON bodies.
        if native_response.inner.get_status_code() >= 300
            && native_response
                .inner
                .get_header(BStr::new("content-type"))
                .is_some_and(|content_type| content_type == "application/json")
            && let Some(body) = native_response.inner.get_body()
        {
            let decoded = json_decode_assoc(body)?;
            if let PhpMixed::Array(a) = decoded {
                HttpDownloader::output_warnings(self.io.clone(), origin, &a)?;
            }
        }

        // Authenticated-retry detection (401/403, Bitbucket login page, GitLab archive 404).
        let auth_result = self.is_authenticated_retry_needed(
            url,
            origin,
            filename,
            attributes,
            &native_response,
        )?;
        match auth_result {
            Ok(prompt) if prompt.retry => {
                attributes.insert(
                    "storeAuth".into(),
                    match prompt.store_auth {
                        StoreAuth::Bool(b) => PhpMixed::Bool(b),
                        StoreAuth::Prompt => PhpMixed::string(BString::from("prompt")),
                    },
                );
                let retries = attributes
                    .get(B("retries"))
                    .and_then(|v| v.as_int())
                    .unwrap_or(0);
                attributes.insert("retries".into(), PhpMixed::Int(retries + 1));
                if let Some(filename) = filename {
                    unlink_silent(Self::temp_file(filename));
                }
                return Ok(Decision::Retry {
                    url: url.to_owned(),
                    delay_ms: None,
                });
            }
            Ok(_) => {}
            Err(e) => return Ok(Decision::Failed((*e).into())),
        }

        // Handle 3xx redirects, 304 Not Modified excluded.
        let redirects = attributes
            .get(B("redirects"))
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        if (300..=399).contains(&status_code)
            && status_code != 304
            && redirects < self.max_redirects
        {
            match self.handle_redirect(url, attributes, &native_response)? {
                Ok(location) if !location.is_empty() => {
                    attributes.insert("redirects".into(), PhpMixed::Int(redirects + 1));
                    if let Some(filename) = filename {
                        unlink_silent(Self::temp_file(filename));
                    }
                    return Ok(Decision::Retry {
                        url: location,
                        delay_ms: None,
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    if let Some(filename) = filename {
                        unlink_silent(Self::temp_file(filename));
                    }
                    return Ok(Decision::Failed((*e).into()));
                }
            }
        }

        // Fail 4xx and 5xx responses (some are retried on GET).
        if (400..=599).contains(&status_code) {
            let retries = attributes
                .get(B("retries"))
                .and_then(|v| v.as_int())
                .unwrap_or(0);
            if Self::method_is_get(options)
                && matches!(status_code, 423 | 425 | 500 | 502 | 503 | 504 | 507 | 510)
                && retries < self.max_retries
            {
                self.io.write_error_with_verbosity(
                    bformat!(
                        "Retrying ({}) {} due to status code {status_code}",
                        retries + 1,
                        Url::sanitize(url)
                    ),
                    DEBUG,
                );
                attributes.insert("retries".into(), PhpMixed::Int(retries + 1));
                if let Some(filename) = filename {
                    unlink_silent(Self::temp_file(filename));
                }
                return Ok(Decision::Retry {
                    url: url.to_owned(),
                    delay_ms: Self::retry_delay_ms(retries + 1),
                });
            }

            let status_msg = native_response
                .inner
                .get_status_message()
                .unwrap_or_default();
            // Enrich with the response headers/status/body, mirroring PHP's catch block that
            // calls setHeaders/setStatusCode/setResponse before reject().
            let mut e = self.fail_response(url, filename, &native_response, &status_msg);
            e.set_headers(native_response.inner.get_headers().clone());
            e.set_status_code(Some(native_response.inner.get_status_code()));
            e.set_response(native_response.inner.get_body().map(<[u8]>::to_vec));
            return Ok(Decision::Failed((*e).into()));
        }

        // storeAuth on success.
        let store_auth = attributes.get(B("storeAuth")).cloned();
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
            let _ = rename(Self::temp_file(filename), filename);
        }

        Ok(Decision::Done(native_response.inner))
    }

    /// Performs one non-blocking HTTP request via reqwest, enforcing max_file_size and streaming
    /// the body to the `~` temp file when in file mode. Replaces PHP's curl_setopt block + curl I/O.
    async fn send_once(
        &self,
        url: &BStr,
        request: &CurlRequestOptions,
        connection: &ConnectionOptions,
        filename: Option<&std::path::Path>,
    ) -> Result<RawResponse, TransportError> {
        let CurlRequestOptions {
            method,
            body,
            timeout_secs,
            headers,
            max_file_size,
        } = request;
        let (body, max_file_size) = (body.clone(), *max_file_size);

        // curl reports a CA file, client certificate or proxy it cannot use as an error of the
        // transfer.
        let client = runtime_client(connection).map_err(|message| TransportError {
            message,
            retryable: false,
            is_connect: false,
            was_timeout: false,
            is_max_file_size: false,
        })?;

        let reqwest_method = reqwest::Method::from_bytes(method).unwrap_or(reqwest::Method::GET);

        // TODO(http): reqwest parses the URL as text, so a URL that is not valid UTF-8 fails here
        // as a transport error, where curl would send its bytes.
        let Some(url) = utf8_text(url) else {
            return Err(TransportError {
                message: display_to_bytes(format_args!("URL is not valid UTF-8: {:?}", url)),
                retryable: false,
                is_connect: false,
                was_timeout: false,
                is_max_file_size: false,
            });
        };
        let mut builder = client.request(reqwest_method, url);
        for header in headers {
            if let Some((name, value)) = header.split_once_str(":") {
                // Skip the Connection header reqwest manages itself.
                let name = trim(name);
                if name.eq_ignore_ascii_case(b"connection") {
                    continue;
                }
                builder = builder.header(name.as_slice(), trim(value).as_slice());
            }
        }
        if let Some(timeout_secs) = timeout_secs {
            builder = builder.timeout(std::time::Duration::from_secs(*timeout_secs));
        }
        if let Some(body) = body {
            builder = builder.body(body);
        }

        let resp = builder.send().await.map_err(|e| TransportError {
            message: display_to_bytes(&e),
            retryable: e.is_timeout() || e.is_connect() || e.is_request(),
            is_connect: e.is_connect(),
            was_timeout: e.is_timeout(),
            is_max_file_size: false,
        })?;

        let status = resp.status().as_u16() as i64;
        let status_line = bformat!(
            "HTTP/{} {}",
            match resp.version() {
                reqwest::Version::HTTP_09 => "0.9",
                reqwest::Version::HTTP_10 => "1.0",
                reqwest::Version::HTTP_11 => "1.1",
                reqwest::Version::HTTP_2 => "2",
                reqwest::Version::HTTP_3 => "3",
                _ => "1.1",
            },
            display_to_bytes(resp.status())
        );
        // Header lines start with the status line so Response::get_status_message can find it.
        // TODO(http): reqwest drops Content-Encoding and Content-Length from a response it decodes,
        // while curl reports the headers as received.
        let mut headers_out: Vec<BString> = vec![status_line];
        for (k, v) in resp.headers().iter() {
            headers_out.push(bformat!("{}: {}", k.as_str(), v.as_bytes()));
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
        filename: Option<&std::path::Path>,
    ) -> Result<Body, (BString, bool)> {
        use tokio::io::AsyncWriteExt as _;

        let mut written: u64 = 0;
        enum Sink {
            File(tokio::fs::File),
            Memory(Vec<u8>),
        }
        let mut sink = match filename {
            Some(f) => Sink::File(
                tokio::fs::File::create(Self::temp_file(f))
                    .await
                    .map_err(|e| (display_to_bytes(&e), false))?,
            ),
            None => Sink::Memory(Vec::new()),
        };

        loop {
            let chunk = match resp.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(e) => return Err((display_to_bytes(&e), false)),
            };
            written += chunk.len() as u64;
            if let Some(max) = max_file_size
                && written > max
            {
                return Err((
                    bformat!(
                        "Maximum allowed download size reached. Downloaded {written} of allowed {max} bytes"
                    ),
                    true,
                ));
            }
            match &mut sink {
                Sink::File(f) => f
                    .write_all(&chunk)
                    .await
                    .map_err(|e| (display_to_bytes(&e), false))?,
                Sink::Memory(buf) => buf.extend_from_slice(&chunk),
            }
        }

        Ok(match sink {
            Sink::File(mut f) => {
                // tokio's File returns from write_all once the blocking write is queued, so the
                // tail of the body can still be in flight after the loop above. flush waits for it
                // and reports its error; without it the caller renames and copies a short file.
                f.flush().await.map_err(|e| (display_to_bytes(&e), false))?;
                Body::File
            }
            Sink::Memory(buf) => Body::Memory(buf),
        })
    }

    fn handle_redirect(
        &self,
        url: &BStr,
        attributes: &IndexMap<BString, PhpMixed>,
        response: &NativeHttpResponse,
    ) -> anyhow::Result<Result<BString, Box<TransportException>>> {
        let mut target_url = BString::default();
        if let Some(location_header) = response.inner.get_header(BStr::new("location"))
            && !location_header.is_empty()
        {
            let location_parsed = parse_url(&location_header);
            if boolval(
                &location_parsed
                    .as_ref()
                    .and_then(|parsed| parsed.scheme.as_ref()),
            ) {
                // Absolute URL; e.g. https://example.com/composer
                target_url = location_header;
            } else if boolval(
                &location_parsed
                    .as_ref()
                    .and_then(|parsed| parsed.host.as_ref()),
            ) {
                // Scheme relative; e.g. //example.com/foo
                target_url = bformat!(
                    "{}:{location_header}",
                    parse_url(url)
                        .and_then(|parsed| parsed.scheme)
                        .unwrap_or_default()
                );
            } else if location_header.starts_with(b"/") {
                // Absolute path; e.g. /foo
                let url_host = parse_url(url).and_then(|parsed| parsed.host);
                let url_host_str = url_host.unwrap_or_default();
                target_url = preg_replace(
                    bformat!(
                        "{{^(.+(?://|@){}(?::\\d+)?)(?:[/\\?].*)?$}}",
                        preg_quote(url_host_str, None)
                    ),
                    bformat!("\\1{location_header}"),
                    url,
                );
            } else {
                // Relative path; e.g. foo
                target_url = preg_replace(
                    php_regex!(r"{^(.+/)[^/?]*(?:\?.*)?$}"),
                    bformat!("\\1{location_header}"),
                    url,
                );
            }
        }

        if !target_url.is_empty() {
            self.io.write_error_with_verbosity(
                bformat!(
                    "Following redirect ({}) {}",
                    attributes
                        .get(B("redirects"))
                        .and_then(|v| v.as_int())
                        .unwrap_or(0)
                        + 1,
                    Url::sanitize(target_url.clone())
                ),
                DEBUG,
            );

            return Ok(Ok(target_url));
        }

        Ok(Err(Box::new(TransportException::new(
            bformat!(
                "The \"{url}\" file could not be downloaded, got redirect without Location ({})",
                response.inner.get_status_message().unwrap_or_default()
            ),
            0,
        ))))
    }

    fn is_authenticated_retry_needed(
        &self,
        url: &BStr,
        origin: &BStr,
        filename: Option<&std::path::Path>,
        attributes: &IndexMap<BString, PhpMixed>,
        response: &NativeHttpResponse,
    ) -> anyhow::Result<Result<PromptAuthResult, Box<TransportException>>> {
        let retry_auth_failure = attributes
            .get(B("retryAuthFailure"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false);
        let retries = attributes
            .get(B("retries"))
            .and_then(|b| b.as_int())
            .unwrap_or(0);

        if in_array_loose(
            response.inner.get_status_code(),
            &[PhpMixed::Int(401), PhpMixed::Int(403)],
        ) && retry_auth_failure
        {
            let status_message = response.inner.get_status_message();
            let result = self.auth_helper.borrow_mut().prompt_auth_if_needed(
                url,
                origin,
                response.inner.get_status_code(),
                status_message.as_ref().map(|message| message.as_bstr()),
                response.inner.get_headers().clone(),
                retries,
                response.inner.get_body().map(BStr::new),
            )?;

            if result.retry {
                return Ok(Ok(result));
            }
        }

        let location_header = response.inner.get_header(BStr::new("location"));
        let mut needs_auth_retry: Option<&'static BStr> = None;

        // check for bitbucket login page asking to authenticate
        if origin == "bitbucket.org"
            && !self.auth_helper.borrow().is_public_bit_bucket_download(url)
            && substr(url, -4, None) == ".zip"
            && (location_header.is_none()
                || substr(location_header.as_ref().unwrap(), -4, None) != ".zip")
            && preg_is_match(
                php_regex!(r"{^text/html\b}i"),
                response
                    .inner
                    .get_header(BStr::new("content-type"))
                    .unwrap_or_default(),
            )
        {
            needs_auth_retry = Some(BStr::new(
                "Bitbucket requires authentication and it was not provided",
            ));
        }

        // check for gitlab 404 when downloading archives
        if response.inner.get_status_code() == 404
            && in_array_strict(
                PhpMixed::string(origin),
                &self.config.get_list("gitlab-domains")?,
            )
            && strpos(url, "archive.zip").is_some()
        {
            needs_auth_retry = Some(BStr::new(
                "GitLab requires authentication and it was not provided",
            ));
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

            return Ok(Err(self.fail_response(
                url,
                filename,
                response,
                msg.as_bytes(),
            )));
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
        url: &BStr,
        filename: Option<&std::path::Path>,
        response: &NativeHttpResponse,
        error_message: &[u8],
    ) -> Box<TransportException> {
        if let Some(filename) = filename {
            unlink_silent(Self::temp_file(filename));
        }

        let mut details = BString::default();
        if matches!(
            strtolower(
                response
                    .inner
                    .get_header(BStr::new("content-type"))
                    .unwrap_or_default()
            )
            .as_slice(),
            b"application/json" | b"application/json; charset=utf-8"
        ) {
            let body = response.inner.get_body().unwrap_or_default();
            details = bformat!(
                ":{PHP_EOL}{}{}",
                substr(body, 0, Some(200)),
                if strlen(body) > 200 { "..." } else { "" }
            );
        }

        Box::new(TransportException::new(
            bformat!(
                "The \"{url}\" file could not be downloaded ({}){details}",
                error_message.as_bstr()
            ),
            response.inner.get_status_code(),
        ))
    }

    /// The `curl_setopt()` conversions of the options `CurlDownloader` maps to curl options.
    fn curl_request_options(options: &StreamContextOptions) -> anyhow::Result<CurlRequestOptions> {
        let http = options.http();
        let method = http
            .and_then(HttpOptions::method)
            .map_or_else(|| BString::from("GET"), ToPhpString::to_php_string);
        let body = match http.and_then(HttpOptions::content) {
            None => None,
            // TODO(http): curl sends an array CURLOPT_POSTFIELDS as a multipart/form-data body.
            Some(PhpMixed::List(_) | PhpMixed::Array(_)) => {
                return Err(RuntimeException::new(
                    "An array http.content (a multipart/form-data body) is not supported",
                )
                .into());
            }
            Some(content) => Some(content.to_php_string().into()),
        };
        // CURLOPT_TIMEOUT is max(default_socket_timeout, 300) unless http.timeout overrides it,
        // and curl reads 0 as no timeout.
        let timeout_secs = match http.and_then(HttpOptions::timeout) {
            None => Some(300),
            Some(timeout) => u64::try_from(intval(timeout)).ok().filter(|secs| *secs > 0),
        };
        let headers = match http {
            Some(http) if http.header().is_some() => http.header_lines()?,
            _ => Vec::new(),
        };
        let max_file_size = options.max_file_size()?.map(|n| n.max(0) as u64);

        Ok(CurlRequestOptions {
            method,
            body,
            timeout_secs,
            headers,
            max_file_size,
        })
    }

    /// The `ssl` part of PHP's `curl_setopt` loop over `self::$options`, and
    /// `curl_setopt_array($curlHandle, $proxy->getCurlOptions($options['ssl'] ?? []))`.
    fn connection_options(
        url: &BStr,
        options: &StreamContextOptions,
        attributes: &IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<ConnectionOptions> {
        let ssl = options.ssl();
        let isset = |name: &[u8]| ssl.and_then(|ssl| ssl.isset(name));
        let path = |name: &[u8]| {
            isset(name).map(|value| path_from_bytes(&value.to_php_string()).to_owned())
        };
        // curl_setopt() converts the value of a long option with zval_get_long(), and curl reads
        // 0 as off. PHP passes 2 for a `verify_peer_name` of true.
        let enabled = |name: &[u8]| isset(name).is_none_or(|value| intval(value) != 0);

        let proxy = ProxyManager::get_instance()
            .as_ref()
            .expect("ProxyManager::get_instance() always creates the instance")
            .get_proxy_for_request(url)?;
        let proxy_options =
            proxy.get_curl_options(&ssl.map(SslOptions::to_array).unwrap_or_default())?;

        Ok(ConnectionOptions {
            cafile: path(b"cafile"),
            capath: path(b"capath"),
            verify_peer: enabled(b"verify_peer"),
            verify_peer_name: enabled(b"verify_peer_name"),
            local_cert: path(b"local_cert"),
            local_pk: path(b"local_pk"),
            proxy: proxy_options
                .get(&CURLOPT_PROXY)
                .map(ToPhpString::to_php_string)
                .filter(|proxy| !proxy.is_empty()),
            proxy_auth: proxy_options
                .get(&CURLOPT_PROXYUSERPWD)
                .map(ToPhpString::to_php_string),
            ip_resolve: attributes.get(B("ipResolve")).and_then(PhpMixed::as_int),
        })
    }

    fn method_is_get(options: &StreamContextOptions) -> bool {
        match options.http().and_then(HttpOptions::method) {
            None => true,
            Some(method) => matches!(method, PhpMixed::String(method) if method == "GET"),
        }
    }

    /// The `~` file a download is written to before it is renamed to `filename`.
    fn temp_file(filename: &std::path::Path) -> std::path::PathBuf {
        path_from_bytes(&bformat!("{}~", path_to_bytes(filename))).to_owned()
    }
}

/// What one request sends, converted from its options.
#[derive(Debug)]
struct CurlRequestOptions {
    method: BString,
    body: Option<Vec<u8>>,
    /// `None` for no timeout.
    timeout_secs: Option<u64>,
    headers: Vec<BString>,
    max_file_size: Option<u64>,
}

/// A transport-layer failure approximating the curl errno branches PHP inspected.
struct TransportError {
    message: BString,
    retryable: bool,
    is_connect: bool,
    was_timeout: bool,
    /// The failure is a max_file_size violation (PHP MaxFileSizeExceededException), not a curl error.
    is_max_file_size: bool,
}

/// Raw send result before conversion to `NativeHttpResponse`.
struct RawResponse {
    status: i64,
    headers: Vec<BString>,
    body: Body,
}

enum Body {
    /// Body read into memory.
    Memory(Vec<u8>),
    /// Body streamed to the `~` temp file (contents reference the final filename via the job).
    File,
}

impl RawResponse {
    fn into_native_http_response(self, url: &BStr) -> NativeHttpResponse {
        let body = match self.body {
            Body::Memory(b) => Some(b),
            // File mode: PHP stores `$filename.'~'` as the contents string; the actual bytes live
            // on disk. We do not have the filename here, so leave the body empty — the caller's
            // rename/read handling does not consult the body for successful file downloads.
            Body::File => None,
        };
        NativeHttpResponse::new(
            url.to_owned(),
            Some(self.status),
            self.headers,
            body,
            IndexMap::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read as _, Write as _};

    // Serves one HTTP/1.1 request on a loopback port with a fixed response, and hands the request
    // it received to the returned receiver.
    fn serve_once() -> (std::net::SocketAddr, std::sync::mpsc::Receiver<Vec<u8>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.ends_with(b"\r\n\r\n") {
                let n = stream.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buf[..n]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
            sender.send(request).unwrap();
        });
        (addr, receiver)
    }

    fn connection() -> ConnectionOptions {
        ConnectionOptions {
            cafile: None,
            capath: None,
            verify_peer: true,
            verify_peer_name: true,
            local_cert: None,
            local_pk: None,
            proxy: None,
            proxy_auth: None,
            ip_resolve: None,
        }
    }

    fn get(connection: &ConnectionOptions, url: &BStr) -> Result<Vec<u8>, BString> {
        let client = build_client(connection)?;
        let url = utf8_text(url).unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let response = client.get(url).send().await?;
                Ok(response.bytes().await?.to_vec())
            })
            .map_err(|e: reqwest::Error| error_chain(&e))
    }

    #[test]
    fn sends_requests_through_the_proxy_with_its_credentials() {
        let (addr, request) = serve_once();
        let connection = ConnectionOptions {
            proxy: Some(bformat!("http://{}", display_to_bytes(addr))),
            proxy_auth: Some(BString::from("us%3Aer:p%40ss:word")),
            ..connection()
        };

        assert_eq!(
            get(&connection, BStr::new("http://example.invalid/foo")).unwrap(),
            b"ok"
        );

        let request = BString::from(request.recv().unwrap());
        assert!(
            request.starts_with(b"GET http://example.invalid/foo HTTP/1.1\r\n"),
            "{request:?}"
        );
        let header = bformat!(
            "proxy-authorization: Basic {}\r\n",
            base64_encode("us:er:p@ss:word")
        );
        assert!(request.contains_str(&header), "{request:?}");
    }

    #[test]
    fn ip_resolve_restricts_the_address_family() {
        let (addr, _request) = serve_once();
        let url = bformat!("http://{}/", display_to_bytes(addr));

        let ipv6 = ConnectionOptions {
            ip_resolve: Some(6),
            ..connection()
        };
        assert!(get(&ipv6, url.as_bstr()).is_err());

        let ipv4 = ConnectionOptions {
            ip_resolve: Some(4),
            ..connection()
        };
        assert_eq!(get(&ipv4, url.as_bstr()).unwrap(), b"ok");
    }

    #[test]
    fn an_unreadable_cafile_fails_to_build_the_client() {
        let connection = ConnectionOptions {
            cafile: Some("/nonexistent/cafile.pem".into()),
            ..connection()
        };
        let message = build_client(&connection).unwrap_err();
        assert!(
            message.contains_str("/nonexistent/cafile.pem"),
            "{message:?}"
        );
    }

    #[test]
    fn reads_the_ssl_options_as_curl_setopt_does() {
        let mut ssl = SslOptions::new();
        ssl.set_cafile("/ca/file.pem");
        ssl.insert("capath", PhpMixed::Null);
        ssl.insert("verify_peer", PhpMixed::string("0"));
        ssl.insert("verify_peer_name", PhpMixed::Bool(true));
        ssl.insert("local_cert", PhpMixed::string("/client.pem"));
        let mut options = StreamContextOptions::new();
        options.set_ssl(ssl);
        let mut attributes = IndexMap::new();
        attributes.insert(BString::from("ipResolve"), PhpMixed::Int(4));

        let connection = NativeHttpDownloader::connection_options(
            BStr::new("https://example.org/"),
            &options,
            &attributes,
        )
        .unwrap();

        assert_eq!(connection.cafile, Some("/ca/file.pem".into()));
        assert_eq!(connection.capath, None);
        assert!(!connection.verify_peer);
        assert!(connection.verify_peer_name);
        assert_eq!(connection.local_cert, Some("/client.pem".into()));
        assert_eq!(connection.local_pk, None);
        assert_eq!(connection.ip_resolve, Some(4));
    }
}
