//! ref: composer/src/Composer/Util/HttpDownloader.php sha1:87f8314e3bf11a6b4719ebd0e93284ec827a9e3c

use crate::composer;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::exception::IrrecoverableDownloadException;
use crate::io::{IOInterface, IOInterfaceImmutable, IOInterfaceWrite};
use crate::package::version::VersionParser;
use crate::util::http::{NativeHttpDownloader, Response};
use crate::util::{
    GetResult, LocalFilesystem, Platform, RemoteFilesystem, Silencer, StreamContextFactory, Url,
    sync_executor,
};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, InvalidArgumentException, LogicException, PHP_EOL, PhpJson, PhpMixed,
    array_replace_recursive, bconcat, empty, extension_loaded, implode, intval, is_numeric,
    path_to_bytes, pcre_check_utf8_subject, php_regex, php_truthy, preg_is_match, preg_match,
    preg_replace, rawurldecode, stripos, strpos, substr, ucfirst, utf8_text,
};
use shirabe_semver::constraint::SimpleConstraint;

#[derive(Debug)]
pub struct HttpDownloader {
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    options: IndexMap<BString, PhpMixed>,
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
    log: Vec<BString>,
}

/// For testing only. A single expected HTTP request and the canned response to return.
/// `options` of `None` means "match any options"; otherwise the executed options must be
/// exactly equal (PHP `===`).
#[derive(Debug, Clone)]
pub struct HttpDownloaderMockExpectation {
    pub url: BString,
    pub options: Option<IndexMap<BString, PhpMixed>>,
    pub status: i64,
    pub body: BString,
    pub headers: Vec<BString>,
}

/// For testing only. The default response handler used for undefined requests when not strict.
#[derive(Debug, Clone)]
pub struct HttpDownloaderMockHandler {
    pub status: i64,
    pub body: BString,
    pub headers: Vec<BString>,
}

impl Default for HttpDownloaderMockHandler {
    fn default() -> Self {
        Self {
            status: 200,
            body: BString::default(),
            headers: Vec::new(),
        }
    }
}

impl HttpDownloader {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        options: IndexMap<BString, PhpMixed>,
        disable_tls: bool,
    ) -> Self {
        let disabled = Platform::get_env("COMPOSER_DISABLE_NETWORK")
            .is_some_and(|s| !s.is_empty() && s != "0");

        // Setup TLS options
        // The cafile option can be set via config.json
        let mut self_options: IndexMap<BString, PhpMixed> = IndexMap::new();
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
            max_jobs = intval(&max_jobs_env_mixed).clamp(1, 50);
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
    pub fn get(
        &self,
        url: impl AsRef<[u8]>,
        options: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<Response> {
        sync_executor::block_on(self.execute(url.as_ref().as_bstr(), options, None, true))
    }

    /// Create an async download operation
    pub async fn add(
        &self,
        url: impl AsRef<[u8]>,
        options: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<Response> {
        self.execute(url.as_ref().as_bstr(), options, None, false)
            .await
    }

    /// Copy a file synchronously
    pub fn copy(
        &self,
        url: impl AsRef<[u8]>,
        to: impl AsRef<std::path::Path>,
        options: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<Response> {
        sync_executor::block_on(self.execute(
            url.as_ref().as_bstr(),
            options,
            Some(to.as_ref()),
            true,
        ))
    }

    /// Create an async copy operation
    pub async fn add_copy(
        &self,
        url: impl AsRef<[u8]>,
        to: impl AsRef<std::path::Path>,
        options: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<Response> {
        self.execute(url.as_ref().as_bstr(), options, Some(to.as_ref()), false)
            .await
    }

    /// Shared core of `get`/`add`/`copy`/`add_copy`: mock short-circuit, empty-URL guard, the
    /// sync/allow_async gate, and the resolver PHP's `addJob` picks for the request.
    async fn execute(
        &self,
        url: &BStr,
        options: IndexMap<BString, PhpMixed>,
        copy_to: Option<&std::path::Path>,
        sync: bool,
    ) -> anyhow::Result<Response> {
        if self.mock.is_some() {
            return self.mock_get(url, &options);
        }
        if url.is_empty() {
            return Err(InvalidArgumentException::new(BString::from(
                "$url must not be an empty string",
            ))
            .into());
        }
        if !sync && !self.allow_async {
            return Err(LogicException::new(BString::from("You must use the HttpDownloader instance which is part of a Composer\\Loop instance to be able to run async http requests"))
            .into());
        }

        let (options, origin) = self.prepare_request(url, options)?;
        if !self.can_use_native(url, &options) {
            return self.download_with_filesystem(origin.as_bstr(), url, options, copy_to);
        }

        // PHP keeps async jobs in a table `Loop::abortJobs()` can reach through the promises
        // being waited on; a synchronous job is in no such group.
        if sync {
            return Self::download_with_native(
                self.semaphore.clone(),
                self.native.clone().unwrap(),
                self.disabled,
                origin,
                url.to_owned(),
                options,
                copy_to.map(ToOwned::to_owned),
                None,
            )
            .await;
        }
        let (abort_handle, abort_registration) = futures::future::AbortHandle::new_pair();
        let _recorded = crate::plugin::job_registry::record_internal_job(
            crate::plugin::job_registry::JobOwner::of_http_downloader(self),
            Some(abort_handle),
        );
        let request = Self::download_with_native(
            self.semaphore.clone(),
            self.native.clone().unwrap(),
            self.disabled,
            origin,
            url.to_owned(),
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
    pub(crate) fn canceled(url: &BStr) -> IrrecoverableDownloadException {
        IrrecoverableDownloadException::new(bconcat!(
            "Download of ",
            Url::sanitize(url),
            " canceled"
        ))
    }

    /// The part of PHP's `addJob` that runs before the promise exists: the request options merged
    /// over the downloader's own, the origin, and the credentials a URL carries.
    fn prepare_request(
        &self,
        url: &BStr,
        options: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<(IndexMap<BString, PhpMixed>, BString)> {
        let options = array_replace_recursive(self.options.clone(), options);
        let origin = Url::get_origin(&self.config.borrow(), url)?;

        // capture username/password from URL if there is one
        if let Some(m) = preg_match(php_regex!(r"{^https?://([^:/]+):([^@/]+)@([^/]+)}i"), url) {
            self.io.borrow_mut().set_authentication(
                origin.clone(),
                rawurldecode(m.get(1).unwrap_or_default()),
                Some(rawurldecode(m.get(2).unwrap_or_default())),
            );
        }

        Ok((options, origin))
    }

    /// The job behind `add()` / `add_copy()` for a caller that registers it instead of awaiting it
    /// (see `docs/dev/plugin-async.md`). The gates PHP raises out of the call itself are the outer
    /// error. A request the native downloader does not take is run on the spot, as PHP runs the
    /// `RemoteFilesystem` resolver while it constructs the promise, and comes back settled.
    /// `started` tells whether the job budget has let the job through.
    pub(crate) fn add_job(
        &self,
        url: &BStr,
        copy_to: Option<&std::path::Path>,
        options: IndexMap<BString, PhpMixed>,
        started: std::rc::Rc<std::cell::Cell<bool>>,
    ) -> anyhow::Result<HttpDownloaderJob> {
        if self.mock.is_some() {
            // `HttpDownloaderMock::add()` answers with a promise resolved by its own `get()`.
            return Ok(HttpDownloaderJob::Settled(
                Ok(self.mock_get(url, &options)?),
            ));
        }
        if url.is_empty() {
            return Err(InvalidArgumentException::new(BString::from(
                "$url must not be an empty string",
            ))
            .into());
        }
        if !self.allow_async {
            return Err(LogicException::new(BString::from("You must use the HttpDownloader instance which is part of a Composer\\Loop instance to be able to run async http requests"))
            .into());
        }

        let (options, origin) = self.prepare_request(url, options)?;
        if !self.can_use_native(url, &options) {
            return Ok(HttpDownloaderJob::Settled(self.download_with_filesystem(
                origin.as_bstr(),
                url,
                options,
                copy_to,
            )));
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
                url.to_owned(),
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
        origin: BString,
        url: BString,
        options: IndexMap<BString, PhpMixed>,
        copy_to: Option<std::path::PathBuf>,
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
                    .get(B("http"))
                    .and_then(|v| match v {
                        PhpMixed::Array(m) => m.get(B("header")),
                        _ => None,
                    })
                    .cloned();
                if let Some(PhpMixed::List(list)) = http_header.as_ref() {
                    let joined = implode(
                        "",
                        &list
                            .iter()
                            .map(|v| v.as_string().unwrap_or_default())
                            .collect::<Vec<_>>(),
                    );
                    stripos(&joined, "if-modified-since").is_some()
                } else if let Some(PhpMixed::Array(m)) = http_header.as_ref() {
                    let joined = implode(
                        "",
                        &m.values()
                            .map(|v| v.as_string().unwrap_or_default())
                            .collect::<Vec<_>>(),
                    );
                    stripos(&joined, "if-modified-since").is_some()
                } else {
                    false
                }
            };
            if has_if_modified_since {
                return Ok(Response::new(url, Some(304), Vec::new(), Some(Vec::new())));
            }

            let mut e = TransportException::new(
                bconcat!("Network disabled, request canceled: ", Url::sanitize(&url)),
                499,
            );
            e.set_status_code(Some(499));
            return Err(e.into());
        }

        match native
            .download(origin.as_bstr(), url.as_bstr(), options, copy_to.as_deref())
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
        origin: &BStr,
        url: &BStr,
        options: IndexMap<BString, PhpMixed>,
        copy_to: Option<&std::path::Path>,
    ) -> anyhow::Result<Response> {
        if LocalFilesystem::handles(url) {
            return if let Some(copy_to) = copy_to {
                self.local.copy(origin, url, copy_to, false, options)?;

                Ok(Response::new(
                    url.to_owned(),
                    None,
                    Vec::new(),
                    Some(bconcat!(path_to_bytes(copy_to), "~").into()),
                ))
            } else {
                let body = match self.local.get_contents(origin, url, false, options)? {
                    GetResult::Content(s) => Some(s),
                    _ => None,
                };
                Ok(Response::new(url.to_owned(), None, Vec::new(), body))
            };
        }

        if let Some(copy_to) = copy_to {
            self.rfs.copy(origin, url, copy_to, false, options)?;

            let headers = self.rfs.get_last_headers()?;
            let code = RemoteFilesystem::find_status_code(&headers);
            let body = Some(bconcat!(path_to_bytes(copy_to), "~").into());
            Ok(Response::new(url.to_owned(), code, headers, body))
        } else {
            let result = self.rfs.get_contents(origin, url, false, options)?;
            let body = match result {
                GetResult::Content(s) => Some(s),
                _ => None,
            };
            let headers = self.rfs.get_last_headers()?;
            let code = RemoteFilesystem::find_status_code(&headers);
            Ok(Response::new(url.to_owned(), code, headers, body))
        }
    }

    /// Retrieve the options set in the constructor
    pub fn get_options(&self) -> &IndexMap<BString, PhpMixed> {
        &self.options
    }

    /// Merges new options
    pub fn set_options(&mut self, options: IndexMap<BString, PhpMixed>) {
        self.options = array_replace_recursive(self.options.clone(), options);
    }

    pub fn enable_async(&mut self) {
        self.allow_async = true;
    }

    pub fn output_warnings(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        url: impl AsRef<[u8]>,
        data: &IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<()> {
        let url = url.as_ref();
        let clean_message = |msg: &BStr| -> anyhow::Result<BString> {
            if !io.is_decorated() {
                pcre_check_utf8_subject("preg_replace", "{\x1b\\[[;\\d]*m}u", msg)?;
                return Ok(preg_replace("{\x1b\\[[;\\d]*m}u", "", msg));
            }

            Ok(msg.to_owned())
        };

        // legacy warning/info keys
        for r#type in ["warning", "info"].iter() {
            let entry = data.get(B(r#type));
            if entry.is_none() || empty(entry.unwrap()) {
                continue;
            }

            let versions_key = bconcat!(r#type, "-versions");
            if let Some(versions_value) = data.get(B(&versions_key))
                && !empty(versions_value)
            {
                let version_parser: VersionParser = VersionParser::new();
                let constraint = version_parser
                    .parse_constraints(versions_value.as_string().unwrap_or_default())?;
                let composer_constraint = SimpleConstraint::new(
                    "==",
                    version_parser.normalize(composer::get_version(), None)?,
                    None,
                );
                if !constraint.matches(&composer_constraint.into()) {
                    continue;
                }
            }

            io.write_error(bconcat!(
                "<",
                r#type,
                ">",
                ucfirst(r#type),
                " from ",
                Url::sanitize(url),
                ": ",
                clean_message(entry.unwrap().as_string().unwrap_or_default())?,
                "</",
                r#type,
                ">"
            ));
        }

        // modern Composer 2.2+ format with support for multiple warning/info messages
        for key in ["warnings", "infos"].iter() {
            let entry = data.get(B(key));
            if entry.is_none() || empty(entry.unwrap()) {
                continue;
            }

            let version_parser: VersionParser = VersionParser::new();
            if let Some(PhpMixed::List(list)) = entry {
                for spec in list {
                    let r#type = substr(key, 0, Some(-1));
                    if let PhpMixed::Array(spec_map) = spec {
                        let constraint = version_parser.parse_constraints(
                            spec_map
                                .get(B("versions"))
                                .and_then(|v| v.as_string())
                                .unwrap_or_default(),
                        )?;
                        let composer_constraint = SimpleConstraint::new(
                            "==",
                            version_parser.normalize(composer::get_version(), None)?,
                            None,
                        );
                        if !constraint.matches(&composer_constraint.into()) {
                            continue;
                        }

                        io.write_error(bconcat!(
                            "<",
                            r#type,
                            ">",
                            ucfirst(&r#type),
                            " from ",
                            Url::sanitize(url),
                            ": ",
                            clean_message(
                                spec_map
                                    .get(B("message"))
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_default()
                            )?,
                            "</",
                            r#type,
                            ">"
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_exception_hints(e: &anyhow::Error) -> Option<Vec<BString>> {
        let e_as_transport: Option<&TransportException> = e.catch::<TransportException>();
        e_as_transport?;
        let e_as_transport = e_as_transport.unwrap();

        if strpos(e_as_transport.get_message(), "Resolving timed out").is_some()
            || strpos(e_as_transport.get_message(), "Could not resolve host").is_some()
        {
            Silencer::suppress(None);
            let mut ctx_options: IndexMap<BString, PhpMixed> = IndexMap::new();
            let mut ssl_map: IndexMap<BString, PhpMixed> = IndexMap::new();
            ssl_map.insert("verify_peer".into(), PhpMixed::Bool(false));
            ctx_options.insert("ssl".into(), PhpMixed::Array(ssl_map));
            let mut http_map: IndexMap<BString, PhpMixed> = IndexMap::new();
            http_map.insert("follow_location".into(), PhpMixed::Bool(false));
            http_map.insert("ignore_errors".into(), PhpMixed::Bool(true));
            ctx_options.insert("http".into(), PhpMixed::Array(http_map));
            let test_connectivity =
                get_as_php_http_wrapper(BStr::new("https://8.8.8.8"), &ctx_options);
            Silencer::restore();
            if test_connectivity.is_ok() {
                return Some(vec![BString::from(
                    "<error>The following exception probably indicates you have misconfigured DNS resolver(s)</error>",
                )]);
            }

            return Some(vec![BString::from(
                "<error>The following exception probably indicates you are offline or have misconfigured DNS resolver(s)</error>",
            )]);
        }

        None
    }

    /// PHP's `canUseCurl`, asked of the native downloader: Composer's reasons to keep a request
    /// away from curl hold for it too.
    fn can_use_native(&self, url: &BStr, options: &IndexMap<BString, PhpMixed>) -> bool {
        if self.native.is_none() {
            return false;
        }

        if !preg_is_match(php_regex!(r"{^https?://}i"), url) {
            return false;
        }

        let allow_self_signed = options.get(B("ssl")).and_then(|v| match v {
            PhpMixed::Array(m) => m.get(B("allow_self_signed")).cloned(),
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
            let urls: Vec<&BStr> = expectations.iter().map(|e| e.url.as_bstr()).collect();
            panic!(
                "There are still {} expected HTTP requests which have not been consumed:{}{}{}{}Received calls:{}{}",
                expectations.len(),
                PHP_EOL.display_lossy(),
                implode("\n", &urls).display_lossy(),
                PHP_EOL.display_lossy(),
                PHP_EOL.display_lossy(),
                PHP_EOL.display_lossy(),
                implode(PHP_EOL, &mock.log).display_lossy(),
            );
        }
    }

    /// For testing only. Mirrors HttpDownloaderMock::get: consumes the next matching expectation
    /// (or falls back to the default handler when not strict) and returns the canned response.
    fn mock_get(
        &self,
        file_url: &BStr,
        options: &IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<Response> {
        if file_url.is_empty() {
            return Err(LogicException::new(BString::from("url cannot be an empty string")).into());
        }

        let mock = self
            .mock
            .as_ref()
            .expect("mock_get called without a mock")
            .clone();
        let mut mock = mock.borrow_mut();
        mock.log.push(file_url.to_owned());

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
                    bconcat!(
                        "\" with options \"",
                        BString::from(
                            serde_json::to_vec(&PhpJson::new(
                                &first.options.clone().map(PhpMixed::Array),
                            ))
                            .unwrap_or_default()
                        )
                    )
                } else {
                    BString::default()
                };
                bconcat!("Expected \"", first.url, opts, "\" at this point.")
            })
            .unwrap_or_else(|| BString::from("Expected no more calls at this point."));
        let prior_calls = if mock.log.len() > 1 {
            implode(PHP_EOL, &mock.log[..mock.log.len() - 1])
        } else {
            BString::default()
        };
        panic!(
            "Received unexpected request for \"{}\" with options \"{}\"{}{}{}Received calls:{}{}",
            file_url.display_lossy(),
            serde_json::to_string(&PhpJson::new(&PhpMixed::Array(options.clone())))
                .unwrap_or_default(),
            PHP_EOL.display_lossy(),
            next_expected.display_lossy(),
            PHP_EOL.display_lossy(),
            PHP_EOL.display_lossy(),
            prior_calls.display_lossy(),
        );
    }

    /// For testing only. Mirrors HttpDownloaderMock::respond.
    fn mock_respond(
        url: &BStr,
        status: i64,
        headers: Vec<BString>,
        body: BString,
    ) -> anyhow::Result<Response> {
        if status < 400 {
            return Ok(Response::new(
                url.to_owned(),
                Some(status),
                headers,
                Some(body.into()),
            ));
        }

        let mut e = TransportException::new(
            bconcat!("The \"", url, "\" file could not be downloaded"),
            status,
        );
        e.set_headers(headers);
        e.set_response(Some(body.into()));

        Err(e.into())
    }
}

/// Stands in for PHP's `file_get_contents($url, false, stream_context_create($options))` on an
/// `http://` or `https://` URL: performs a GET the way PHP's http stream wrapper does and returns
/// the body. It honors the context options `ssl.verify_peer`, `ssl.verify_peer_name`,
/// `http.follow_location` and `http.ignore_errors`, and panics on any other option.
/// TODO(http): the remaining options of PHP's `http` and `ssl` contexts (`method`, `header`,
/// `content`, `timeout`, `max_redirects`, `proxy`, `cafile`, ...) are not modeled.
fn get_as_php_http_wrapper(
    url: &BStr,
    options: &IndexMap<BString, PhpMixed>,
) -> anyhow::Result<Vec<u8>> {
    assert!(
        preg_is_match(php_regex!(r"{^https?://}i"), url),
        "{url:?} is not an http:// or https:// URL",
    );
    for key in options.keys() {
        assert!(
            key == "ssl" || key == "http",
            "the stream context option {key:?} is not supported",
        );
    }
    let wrapper_options = |wrapper: &[u8], known: &[&[u8]]| -> IndexMap<BString, PhpMixed> {
        let map = options
            .get(wrapper)
            .and_then(PhpMixed::as_array)
            .cloned()
            .unwrap_or_default();
        for key in map.keys() {
            assert!(
                known.contains(&key.as_slice()),
                "the {:?} stream context option {key:?} is not supported",
                wrapper.as_bstr(),
            );
        }
        map
    };
    let ssl = wrapper_options(b"ssl", &[b"verify_peer", b"verify_peer_name"]);
    let http = wrapper_options(b"http", &[b"follow_location", b"ignore_errors"]);
    let is_true_or = |map: &IndexMap<BString, PhpMixed>, key: &[u8], default: bool| {
        map.get(key).map_or(default, php_truthy)
    };
    let verify_peer = is_true_or(&ssl, b"verify_peer", true);
    let verify_peer_name = is_true_or(&ssl, b"verify_peer_name", true);
    let follow_location = is_true_or(&http, b"follow_location", true);
    let ignore_errors = is_true_or(&http, b"ignore_errors", false);
    // PHP's default `max_redirects` and `default_socket_timeout`.
    let max_redirects = 20;
    let timeout = std::time::Duration::from_secs(60);

    let url = utf8_text(url)
        .ok_or_else(|| anyhow::anyhow!("the URL is not valid UTF-8"))?
        .to_owned();
    let fetch = async move {
        let client = reqwest::Client::builder()
            // PHP's http wrapper speaks HTTP/1.x, sends no Accept-Encoding, does not decode the
            // body and does not read the *_proxy environment variables.
            .http1_only()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .no_proxy()
            .redirect(if follow_location {
                reqwest::redirect::Policy::limited(max_redirects)
            } else {
                reqwest::redirect::Policy::none()
            })
            .tls_danger_accept_invalid_certs(!verify_peer)
            .tls_danger_accept_invalid_hostnames(!verify_peer_name)
            .connect_timeout(timeout)
            .read_timeout(timeout)
            .build()?;
        let mut response = client.get(url).send().await?;
        if !ignore_errors {
            response = response.error_for_status()?;
        }
        Ok(response.bytes().await?.to_vec())
    };
    // The request runs on a runtime of its own so that the function works whether or not the
    // caller is inside a tokio runtime.
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(fetch)
    })
    .join()
    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_php_shim::display_to_bytes;
    use std::io::{Read as _, Write as _};

    // Serves one HTTP/1.1 request on a loopback port with `response` and returns its URL.
    fn serve_once(response: &'static [u8]) -> BString {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
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
            stream.write_all(response).unwrap();
        });
        bconcat!("http://", display_to_bytes(addr), "/")
    }

    fn http_options(follow_location: bool, ignore_errors: bool) -> IndexMap<BString, PhpMixed> {
        let mut http = IndexMap::new();
        http.insert("follow_location".into(), PhpMixed::Bool(follow_location));
        http.insert("ignore_errors".into(), PhpMixed::Bool(ignore_errors));
        let mut options = IndexMap::new();
        options.insert("http".into(), PhpMixed::Array(http));
        options
    }

    const NOT_FOUND: &[u8] =
        b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope";
    const REDIRECT: &[u8] = b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/\r\nContent-Length: 5\r\nConnection: close\r\n\r\nmoved";

    #[test]
    fn http_error_status_fails_without_ignore_errors() {
        let url = serve_once(NOT_FOUND);
        assert!(get_as_php_http_wrapper(url.as_bstr(), &http_options(true, false)).is_err());
    }

    #[test]
    fn http_error_status_returns_body_with_ignore_errors() {
        let url = serve_once(NOT_FOUND);
        assert_eq!(
            get_as_php_http_wrapper(url.as_bstr(), &http_options(true, true)).unwrap(),
            b"nope"
        );
    }

    #[test]
    fn http_redirect_is_not_followed_without_follow_location() {
        let url = serve_once(REDIRECT);
        assert_eq!(
            get_as_php_http_wrapper(url.as_bstr(), &http_options(false, false)).unwrap(),
            b"moved"
        );
    }
}
