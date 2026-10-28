//! ref: composer/src/Composer/Util/HttpDownloader.php

use crate::composer;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::exception::IrrecoverableDownloadException;
use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::package::version::VersionParser;
use crate::util::http::{NativeHttpDownloader, Response};
use crate::util::{
    GetResult, LocalFilesystem, Platform, RemoteFilesystem, StreamContextFactory, Url,
    sync_executor,
};
use bstr::ByteSlice;
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, InvalidArgumentException, LogicException, PHP_EOL, PhpMixed,
    array_replace_recursive, empty, extension_loaded, file_get_contents, implode, is_numeric,
    php_regex, preg_is_match, preg_match, preg_replace, rawurldecode, stream_context_create,
    stripos, strpos, substr, ucfirst,
};
use shirabe_semver::constraint::SimpleConstraint;

#[derive(Debug)]
pub struct HttpDownloader {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    options: IndexMap<String, PhpMixed>,
    /// Replaces PHP's `$runningJobs`/`$maxJobs` counters: a permit is held for the duration of
    /// each in-flight request and released when it settles.
    semaphore: std::rc::Rc<tokio::sync::Semaphore>,
    /// Takes the http(s) requests, in the place of Composer's `?CurlDownloader`.
    native: Option<std::rc::Rc<NativeHttpDownloader>>,
    /// Takes `file://` URLs and local paths, which Composer hands to its `RemoteFilesystem`.
    local: LocalFilesystem,
    /// Takes whatever neither of the above does. Composer's `RemoteFilesystem`, in the worker.
    rfs: RemoteFilesystem,
    disabled: bool,
    allow_async: bool,
    /// Test-only internal hook. `None` in production. When `Some`, the request methods
    /// (`get`/`copy`/`add`/`add_copy`) short-circuit to canned responses from the
    /// expectation queue instead of performing real I/O. See ADR 0005 for the rationale
    /// (the same internal-hook pattern used for `ProcessExecutor`). Mirrors
    /// composer/tests/Composer/Test/Mock/HttpDownloaderMock.php.
    mock: Option<std::rc::Rc<std::cell::RefCell<HttpDownloaderMockState>>>,
}

/// What `HttpDownloader::add_job` hands back: a request that already ran, or the future of one
/// that has not been polled yet.
pub(crate) enum HttpDownloaderJob {
    Settled(anyhow::Result<Response>),
    Pending(std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Response>>>>),
}

/// For testing only. State backing the `HttpDownloaderMock`: an optional expectation queue,
/// strict flag, default handler for undefined requests, and a log of received URLs.
#[derive(Debug)]
pub struct HttpDownloaderMockState {
    expectations: Option<Vec<HttpDownloaderMockExpectation>>,
    strict: bool,
    default_handler: HttpDownloaderMockHandler,
    log: Vec<String>,
}

/// For testing only. A single expected HTTP request and the canned response to return.
/// `options` of `None` means "match any options"; otherwise the executed options must be
/// exactly equal (PHP `===`).
#[derive(Debug, Clone)]
pub struct HttpDownloaderMockExpectation {
    pub url: String,
    pub options: Option<IndexMap<String, PhpMixed>>,
    pub status: i64,
    pub body: String,
    pub headers: Vec<String>,
}

/// For testing only. The default response handler used for undefined requests when not strict.
#[derive(Debug, Clone)]
pub struct HttpDownloaderMockHandler {
    pub status: i64,
    pub body: String,
    pub headers: Vec<String>,
}

impl Default for HttpDownloaderMockHandler {
    fn default() -> Self {
        Self {
            status: 200,
            body: String::new(),
            headers: Vec::new(),
        }
    }
}

impl HttpDownloader {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        options: IndexMap<String, PhpMixed>,
        disable_tls: bool,
    ) -> Self {
        let disabled = Platform::get_env("COMPOSER_DISABLE_NETWORK")
            .is_some_and(|s| !s.is_empty() && s != "0");

        // Setup TLS options
        // The cafile option can be set via config.json
        let mut self_options: IndexMap<String, PhpMixed> = IndexMap::new();
        if !disable_tls {
            self_options = StreamContextFactory::get_tls_defaults(&options, ()).unwrap_or_default();
        }

        // handle the other externally set options normally.
        self_options = array_replace_recursive(self_options, options.clone());

        // Composer builds its CurlDownloader only when the curl extension is loaded. The native
        // downloader needs no extension, so it is always there.
        let native = Some(std::rc::Rc::new(NativeHttpDownloader::new(
            io.clone(),
            config.clone(),
            options.clone(),
            disable_tls,
        )));

        let local = LocalFilesystem::new(io.clone(), config.clone());

        let rfs = RemoteFilesystem::new(io.clone(), config.clone(), options, disable_tls);

        let mut max_jobs: i64 = 12;
        let max_jobs_env = Platform::get_env("COMPOSER_MAX_PARALLEL_HTTP");
        let max_jobs_env_mixed = match &max_jobs_env {
            Some(s) => PhpMixed::string(s.clone()),
            None => PhpMixed::Bool(false),
        };
        if is_numeric(&max_jobs_env_mixed) {
            max_jobs = max_jobs_env
                .as_deref()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0)
                .clamp(1, 50);
        }

        Self {
            io,
            config,
            options: self_options,
            semaphore: std::rc::Rc::new(tokio::sync::Semaphore::new(max_jobs as usize)),
            native,
            local,
            rfs,
            disabled,
            allow_async: false,
            mock: None,
        }
    }

    /// Download a file synchronously
    pub fn get(&self, url: &str, options: IndexMap<String, PhpMixed>) -> anyhow::Result<Response> {
        sync_executor::block_on(self.execute(url, options, None, true))
    }

    /// Create an async download operation
    pub async fn add(
        &self,
        url: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Response> {
        self.execute(url, options, None, false).await
    }

    /// Copy a file synchronously
    pub fn copy(
        &self,
        url: &str,
        to: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Response> {
        sync_executor::block_on(self.execute(url, options, Some(to), true))
    }

    /// Create an async copy operation
    pub async fn add_copy(
        &self,
        url: &str,
        to: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Response> {
        self.execute(url, options, Some(to), false).await
    }

    /// Shared core of `get`/`add`/`copy`/`add_copy`: mock short-circuit, empty-URL guard, the
    /// sync/allow_async gate, and the resolver PHP's `addJob` picks for the request.
    async fn execute(
        &self,
        url: &str,
        options: IndexMap<String, PhpMixed>,
        copy_to: Option<&str>,
        sync: bool,
    ) -> anyhow::Result<Response> {
        if self.mock.is_some() {
            return self.mock_get(url, &options);
        }
        if url.is_empty() {
            return Err(InvalidArgumentException::new(
                "$url must not be an empty string".to_string(),
            )
            .into());
        }
        if !sync && !self.allow_async {
            return Err(LogicException::new("You must use the HttpDownloader instance which is part of a Composer\\Loop instance to be able to run async http requests"
                .to_string())
            .into());
        }

        let (options, origin) = self.prepare_request(url, options);
        if !self.can_use_native(url, &options) {
            return self.download_with_filesystem(&origin, url, options, copy_to);
        }

        // PHP keeps async jobs in a table `Loop::abortJobs()` can reach through the promises
        // being waited on; a synchronous job is in no such group.
        if sync {
            return Self::download_with_native(
                self.semaphore.clone(),
                self.native.clone().unwrap(),
                self.disabled,
                origin,
                url.to_string(),
                options,
                copy_to.map(ToOwned::to_owned),
                None,
            )
            .await;
        }
        let (abort_handle, abort_registration) = futures::future::AbortHandle::new_pair();
        let _recorded = crate::plugin::job_registry::record_internal_job(
            crate::plugin::job_registry::JobOwner::of_http_downloader(self),
            abort_handle,
        );
        let request = Self::download_with_native(
            self.semaphore.clone(),
            self.native.clone().unwrap(),
            self.disabled,
            origin,
            url.to_string(),
            options,
            copy_to.map(ToOwned::to_owned),
            None,
        );
        match futures::future::Abortable::new(request, abort_registration).await {
            Ok(result) => result,
            Err(futures::future::Aborted) => Err(Self::canceled(url).into()),
        }
    }

    /// The exception PHP's canceler raises for a job that had started.
    pub(crate) fn canceled(url: &str) -> IrrecoverableDownloadException {
        IrrecoverableDownloadException::new(format!(
            "Download of {} canceled",
            Url::sanitize(url.to_string())
        ))
    }

    /// The part of PHP's `addJob` that runs before the promise exists: the request options merged
    /// over the downloader's own, the origin, and the credentials a URL carries.
    fn prepare_request(
        &self,
        url: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> (IndexMap<String, PhpMixed>, String) {
        let options = array_replace_recursive(self.options.clone(), options);
        let origin = Url::get_origin(&self.config.borrow(), url);

        // capture username/password from URL if there is one
        if let Some(m) = preg_match(php_regex!(r"{^https?://([^:/]+):([^@/]+)@([^/]+)}i"), url) {
            self.io.borrow_mut().set_authentication(
                origin.clone(),
                rawurldecode(m.get(1).unwrap_or_default().to_string().as_str()),
                Some(rawurldecode(
                    m.get(2).unwrap_or_default().to_string().as_str(),
                )),
            );
        }

        (options, origin)
    }

    /// The job behind `add()` / `add_copy()` for a caller that registers it instead of awaiting it
    /// (see `docs/dev/plugin-async.md`). The gates PHP raises out of the call itself are the outer
    /// error. A request the native downloader does not take is run on the spot, as PHP runs the
    /// `RemoteFilesystem` resolver while it constructs the promise, and comes back settled.
    /// `started` tells whether the job budget has let the job through.
    pub(crate) fn add_job(
        &self,
        url: &str,
        copy_to: Option<&str>,
        options: IndexMap<String, PhpMixed>,
        started: std::rc::Rc<std::cell::Cell<bool>>,
    ) -> anyhow::Result<HttpDownloaderJob> {
        if self.mock.is_some() {
            // `HttpDownloaderMock::add()` answers with a promise resolved by its own `get()`.
            return Ok(HttpDownloaderJob::Settled(
                Ok(self.mock_get(url, &options)?),
            ));
        }
        if url.is_empty() {
            return Err(InvalidArgumentException::new(
                "$url must not be an empty string".to_string(),
            )
            .into());
        }
        if !self.allow_async {
            return Err(LogicException::new("You must use the HttpDownloader instance which is part of a Composer\\Loop instance to be able to run async http requests"
                .to_string())
            .into());
        }

        let (options, origin) = self.prepare_request(url, options);
        if !self.can_use_native(url, &options) {
            return Ok(HttpDownloaderJob::Settled(
                self.download_with_filesystem(&origin, url, options, copy_to),
            ));
        }

        // PHP starts the job before `add()` returns when the job budget has room. Nothing of the
        // request runs before the first poll here, but a canceller asking whether the job had
        // started gets PHP's answer.
        started.set(self.semaphore.available_permits() > 0);

        Ok(HttpDownloaderJob::Pending(Box::pin(
            Self::download_with_native(
                self.semaphore.clone(),
                self.native.clone().unwrap(),
                self.disabled,
                origin,
                url.to_string(),
                options,
                copy_to.map(ToOwned::to_owned),
                Some(started),
            ),
        )))
    }

    /// PHP's `startJob` for a job the curl resolver queued: wait for the job budget, then either
    /// answer for a disabled network or hand the request to the native downloader.
    #[allow(clippy::too_many_arguments)]
    async fn download_with_native(
        semaphore: std::rc::Rc<tokio::sync::Semaphore>,
        native: std::rc::Rc<NativeHttpDownloader>,
        disabled: bool,
        origin: String,
        url: String,
        options: IndexMap<String, PhpMixed>,
        copy_to: Option<String>,
        started: Option<std::rc::Rc<std::cell::Cell<bool>>>,
    ) -> anyhow::Result<Response> {
        let _permit = semaphore
            .acquire()
            .await
            .expect("semaphore is never closed");
        if let Some(started) = started {
            started.set(true);
        }

        if disabled {
            let has_if_modified_since = {
                let http_header = options
                    .get("http")
                    .and_then(|v| match v {
                        PhpMixed::Array(m) => m.get("header"),
                        _ => None,
                    })
                    .cloned();
                if let Some(PhpMixed::List(list)) = http_header.as_ref() {
                    let joined = implode(
                        "",
                        &list
                            .iter()
                            .map(|v| v.as_string().unwrap_or_default().to_string())
                            .collect::<Vec<_>>(),
                    );
                    stripos(&joined, "if-modified-since").is_some()
                } else if let Some(PhpMixed::Array(m)) = http_header.as_ref() {
                    let joined = implode(
                        "",
                        &m.values()
                            .map(|v| v.as_string().unwrap_or_default().to_string())
                            .collect::<Vec<_>>(),
                    );
                    stripos(&joined, "if-modified-since").is_some()
                } else {
                    false
                }
            };
            if has_if_modified_since {
                return Ok(Response::new(
                    url,
                    Some(304),
                    Vec::new(),
                    Some(String::new()),
                ));
            }

            let mut e = TransportException::new(
                format!("Network disabled, request canceled: {}", Url::sanitize(url)),
                499,
            );
            e.set_status_code(Some(499));
            return Err(e.into());
        }

        match native
            .download(&origin, &url, options, copy_to.as_deref())
            .await
        {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(e),
        }
    }

    /// The resolver PHP's `addJob` builds for a request curl does not take: the blocking
    /// `RemoteFilesystem` download, run immediately. A local file is read by `LocalFilesystem`
    /// instead, which answers with no headers, as the PHP class does for one. The
    /// network-disabled short-circuit never reaches it in PHP either, since `startJob` only
    /// handles jobs the curl resolver queued.
    fn download_with_filesystem(
        &self,
        origin: &str,
        url: &str,
        options: IndexMap<String, PhpMixed>,
        copy_to: Option<&str>,
    ) -> anyhow::Result<Response> {
        if LocalFilesystem::handles(url) {
            return if let Some(copy_to) = copy_to {
                self.local.copy(origin, url, copy_to, false, options)?;

                Ok(Response::new(
                    url.to_string(),
                    None,
                    Vec::new(),
                    Some(format!("{}~", copy_to)),
                ))
            } else {
                let body = match self.local.get_contents(origin, url, false, options)? {
                    GetResult::Content(s) => Some(s),
                    _ => None,
                };
                Ok(Response::new(url.to_string(), None, Vec::new(), body))
            };
        }

        if let Some(copy_to) = copy_to {
            self.rfs.copy(origin, url, copy_to, false, options)?;

            let headers = self.rfs.get_last_headers()?;
            let code = RemoteFilesystem::find_status_code(&headers);
            let body = Some(format!("{}~", copy_to));
            Ok(Response::new(url.to_string(), code, headers, body))
        } else {
            let result = self.rfs.get_contents(origin, url, false, options)?;
            let body = match result {
                GetResult::Content(s) => Some(s),
                _ => None,
            };
            let headers = self.rfs.get_last_headers()?;
            let code = RemoteFilesystem::find_status_code(&headers);
            Ok(Response::new(url.to_string(), code, headers, body))
        }
    }

    /// Retrieve the options set in the constructor
    pub fn get_options(&self) -> &IndexMap<String, PhpMixed> {
        &self.options
    }

    /// Merges new options
    pub fn set_options(&mut self, options: IndexMap<String, PhpMixed>) {
        self.options = array_replace_recursive(self.options.clone(), options);
    }

    pub fn enable_async(&mut self) {
        self.allow_async = true;
    }

    pub fn output_warnings(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        url: &str,
        data: &IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let clean_message = |msg: &str| -> anyhow::Result<String> {
            if !io.is_decorated() {
                return Ok(preg_replace("{\x1b\\[[;\\d]*m}u", "", msg));
            }

            Ok(msg.to_string())
        };

        // legacy warning/info keys
        for r#type in ["warning", "info"].iter() {
            let entry = data.get(*r#type);
            if entry.is_none() || empty(entry.unwrap()) {
                continue;
            }

            let versions_key = format!("{}-versions", r#type);
            if let Some(versions_value) = data.get(&versions_key)
                && !empty(versions_value)
            {
                let version_parser: VersionParser = VersionParser::new();
                let constraint = version_parser.parse_constraints(
                    &versions_value
                        .as_string()
                        .unwrap_or_default()
                        .to_str_lossy(),
                )?;
                let composer_constraint = SimpleConstraint::new(
                    "==".to_string(),
                    version_parser
                        .normalize(&composer::get_version(), None)?
                        .to_string(),
                    None,
                );
                if !constraint.matches(&composer_constraint.into()) {
                    continue;
                }
            }

            io.write_error(&format!(
                "<{tp}>{capitalized} from {url}: {msg}</{tp}>",
                tp = r#type,
                capitalized = ucfirst(r#type),
                url = Url::sanitize(url.to_string()),
                msg = clean_message(
                    &entry
                        .unwrap()
                        .as_string()
                        .unwrap_or_default()
                        .to_str_lossy()
                )?
            ));
        }

        // modern Composer 2.2+ format with support for multiple warning/info messages
        for key in ["warnings", "infos"].iter() {
            let entry = data.get(*key);
            if entry.is_none() || empty(entry.unwrap()) {
                continue;
            }

            let version_parser: VersionParser = VersionParser::new();
            if let Some(PhpMixed::List(list)) = entry {
                for spec in list {
                    let r#type = substr(key, 0, Some(-1));
                    if let PhpMixed::Array(spec_map) = spec {
                        let constraint = version_parser.parse_constraints(
                            &spec_map
                                .get("versions")
                                .and_then(|v| v.as_string())
                                .unwrap_or_default()
                                .to_str_lossy(),
                        )?;
                        let composer_constraint = SimpleConstraint::new(
                            "==".to_string(),
                            version_parser
                                .normalize(&composer::get_version(), None)?
                                .to_string(),
                            None,
                        );
                        if !constraint.matches(&composer_constraint.into()) {
                            continue;
                        }

                        io.write_error(&format!(
                            "<{tp}>{capitalized} from {url}: {msg}</{tp}>",
                            tp = r#type,
                            capitalized = ucfirst(&r#type),
                            url = Url::sanitize(url.to_string()),
                            msg = clean_message(
                                &spec_map
                                    .get("message")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default()
                                    .to_str_lossy()
                            )?
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_exception_hints(e: &anyhow::Error) -> Option<Vec<String>> {
        let e_as_transport: Option<&TransportException> = e.catch::<TransportException>();
        e_as_transport?;
        let e_as_transport = e_as_transport.unwrap();

        if strpos(e_as_transport.get_message(), "Resolving timed out").is_some()
            || strpos(e_as_transport.get_message(), "Could not resolve host").is_some()
        {
            let mut ctx_options: IndexMap<String, PhpMixed> = IndexMap::new();
            let mut ssl_map: IndexMap<String, PhpMixed> = IndexMap::new();
            ssl_map.insert("verify_peer".to_string(), PhpMixed::Bool(false));
            ctx_options.insert("ssl".to_string(), PhpMixed::Array(ssl_map));
            let mut http_map: IndexMap<String, PhpMixed> = IndexMap::new();
            http_map.insert("follow_location".to_string(), PhpMixed::Bool(false));
            http_map.insert("ignore_errors".to_string(), PhpMixed::Bool(true));
            ctx_options.insert("http".to_string(), PhpMixed::Array(http_map));
            // TODO(http): file_get_contents only takes a path; the stream context arg is dropped
            // until the PHP stream-context layer is modeled.
            let _ = stream_context_create(&ctx_options, None);
            let test_connectivity = file_get_contents("https://8.8.8.8");
            if test_connectivity.is_ok() {
                return Some(vec![
                    "<error>The following exception probably indicates you have misconfigured DNS resolver(s)</error>".to_string(),
                ]);
            }

            return Some(vec![
                "<error>The following exception probably indicates you are offline or have misconfigured DNS resolver(s)</error>".to_string(),
            ]);
        }

        None
    }

    /// PHP's `canUseCurl`, asked of the native downloader: Composer's reasons to keep a request
    /// away from curl hold for it too.
    fn can_use_native(&self, url: &str, options: &IndexMap<String, PhpMixed>) -> bool {
        if self.native.is_none() {
            return false;
        }

        if !preg_is_match(php_regex!(r"{^https?://}i"), url) {
            return false;
        }

        let allow_self_signed = options.get("ssl").and_then(|v| match v {
            PhpMixed::Array(m) => m.get("allow_self_signed").cloned(),
            _ => None,
        });
        if let Some(v) = allow_self_signed
            && !empty(&v)
        {
            return false;
        }

        true
    }

    pub fn is_curl_enabled() -> bool {
        extension_loaded("curl")
    }

    /// For testing only. Builds an HttpDownloader whose request methods are fully
    /// short-circuited by the mock (see [`HttpDownloader::__expects`]), without
    /// constructing the native downloader. Mirrors HttpDownloaderMock, which
    /// extends HttpDownloader but never performs curl I/O.
    pub fn __new_mock(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> Self {
        Self {
            local: LocalFilesystem::new(io.clone(), config.clone()),
            rfs: RemoteFilesystem::new(io.clone(), config.clone(), IndexMap::new(), false),
            io,
            config,
            options: IndexMap::new(),
            semaphore: std::rc::Rc::new(tokio::sync::Semaphore::new(12)),
            native: None,
            disabled: false,
            allow_async: false,
            mock: None,
        }
    }

    /// For testing only. Mirrors HttpDownloaderMock::expects: installs the expectation queue,
    /// strict flag and default handler used by the mock request path.
    pub fn __expects(
        &mut self,
        expectations: Vec<HttpDownloaderMockExpectation>,
        strict: bool,
        default_handler: HttpDownloaderMockHandler,
    ) {
        self.mock = Some(std::rc::Rc::new(std::cell::RefCell::new(
            HttpDownloaderMockState {
                expectations: Some(expectations),
                strict,
                default_handler,
                log: Vec::new(),
            },
        )));
    }

    /// For testing only. Mirrors HttpDownloaderMock::assertComplete: panics if any expected
    /// HTTP requests remain unconsumed.
    pub fn __assert_complete(&self) {
        let Some(mock) = &self.mock else {
            return;
        };
        let mock = mock.borrow();
        // this was not configured to expect anything, so no need to react here
        let Some(expectations) = &mock.expectations else {
            return;
        };
        if !expectations.is_empty() {
            let urls: Vec<String> = expectations.iter().map(|e| e.url.clone()).collect();
            panic!(
                "There are still {} expected HTTP requests which have not been consumed:{}{}{}{}Received calls:{}{}",
                expectations.len(),
                PHP_EOL,
                urls.join(PHP_EOL),
                PHP_EOL,
                PHP_EOL,
                PHP_EOL,
                mock.log.join(PHP_EOL),
            );
        }
    }

    /// For testing only. Mirrors HttpDownloaderMock::get: consumes the next matching expectation
    /// (or falls back to the default handler when not strict) and returns the canned response.
    fn mock_get(
        &self,
        file_url: &str,
        options: &IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Response> {
        if file_url.is_empty() {
            return Err(LogicException::new("url cannot be an empty string".to_string()).into());
        }

        let mock = self
            .mock
            .as_ref()
            .expect("mock_get called without a mock")
            .clone();
        let mut mock = mock.borrow_mut();
        mock.log.push(file_url.to_string());

        let matches_first = mock
            .expectations
            .as_ref()
            .and_then(|e| e.first())
            .is_some_and(|first| {
                first.url == file_url
                    && (first.options.is_none() || first.options.as_ref() == Some(options))
            });

        if matches_first {
            let expect = mock.expectations.as_mut().unwrap().remove(0);
            return Self::mock_respond(file_url, expect.status, expect.headers, expect.body);
        }

        if !mock.strict {
            let handler = mock.default_handler.clone();
            return Self::mock_respond(file_url, handler.status, handler.headers, handler.body);
        }

        let next_expected = mock
            .expectations
            .as_ref()
            .and_then(|e| e.first())
            .map(|first| {
                let opts = if first.options.is_some() {
                    format!(
                        "\" with options \"{}",
                        serde_json::to_string(&first.options).unwrap_or_default()
                    )
                } else {
                    String::new()
                };
                format!("Expected \"{}{}\" at this point.", first.url, opts)
            })
            .unwrap_or_else(|| "Expected no more calls at this point.".to_string());
        let prior_calls = if mock.log.len() > 1 {
            mock.log[..mock.log.len() - 1].join(PHP_EOL)
        } else {
            String::new()
        };
        panic!(
            "Received unexpected request for \"{}\" with options \"{}\"{}{}{}Received calls:{}{}",
            file_url,
            serde_json::to_string(options).unwrap_or_default(),
            PHP_EOL,
            next_expected,
            PHP_EOL,
            PHP_EOL,
            prior_calls,
        );
    }

    /// For testing only. Mirrors HttpDownloaderMock::respond.
    fn mock_respond(
        url: &str,
        status: i64,
        headers: Vec<String>,
        body: String,
    ) -> anyhow::Result<Response> {
        if status < 400 {
            return Ok(Response::new(
                url.to_string(),
                Some(status),
                headers,
                Some(body),
            ));
        }

        let mut e = TransportException::new(
            format!("The \"{}\" file could not be downloaded", url),
            status,
        );
        e.set_headers(headers);
        e.set_response(Some(body));

        Err(e.into())
    }
}
