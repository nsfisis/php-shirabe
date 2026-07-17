//! ref: composer/src/Composer/Util/HttpDownloader.php

use crate::composer;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::version::VersionParser;
use crate::util::GetResult;
use crate::util::Platform;
use crate::util::RemoteFilesystem;
use crate::util::Silencer;
use crate::util::StreamContextFactory;
use crate::util::Url;
use crate::util::http::CurlDownloader;
use crate::util::http::Response;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, array_replace_recursive, chr,
    extension_loaded, file_get_contents, function_exists, implode, is_numeric, rawurldecode,
    stream_context_create, stripos, strpos, substr, ucfirst,
};
use shirabe_semver::constraint::SimpleConstraint;

/// @phpstan-type Request array{url: non-empty-string, options: mixed[], copyTo: string|null}
/// @phpstan-type Job array{id: int, status: int, request: Request, sync: bool, origin: string, resolve?: callable, reject?: callable, curl_id?: int, response?: Response, exception?: \Throwable}
#[derive(Debug)]
pub struct HttpDownloader {
    /// @var IOInterface
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    /// @var Config
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    /// @var array<Job>
    jobs: IndexMap<i64, Job>,
    /// @var mixed[]
    options: IndexMap<String, PhpMixed>,
    /// @var int
    running_jobs: i64,
    /// @var int
    max_jobs: i64,
    /// @var ?CurlDownloader
    curl: Option<CurlDownloader>,
    /// @var ?RemoteFilesystem
    rfs: Option<std::rc::Rc<std::cell::RefCell<RemoteFilesystem>>>,
    /// @var int
    id_gen: i64,
    /// @var bool
    disabled: bool,
    /// @var bool
    allow_async: bool,
    /// Test-only internal hook. `None` in production. When `Some`, the request methods
    /// (`get`/`copy`/`add`/`add_copy`) short-circuit to canned responses from the
    /// expectation queue instead of performing real I/O. See ADR 0005 for the rationale
    /// (the same internal-hook pattern used for `ProcessExecutor`). Mirrors
    /// composer/tests/Composer/Test/Mock/HttpDownloaderMock.php.
    mock: Option<std::rc::Rc<std::cell::RefCell<HttpDownloaderMockState>>>,
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

#[derive(Debug)]
struct Job {
    id: i64,
    status: i64,
    request: Request,
    sync: bool,
    origin: String,
    response: Option<Response>,
    exception: Option<anyhow::Error>,
}

#[derive(Debug, Clone)]
struct Request {
    url: String,
    options: IndexMap<String, PhpMixed>,
    copy_to: Option<String>,
}

/// A single-threaded tokio Runtime used only to drive `CurlDownloader::download()` (which needs a
/// real reactor now that it uses the non-blocking `reqwest::Client`, unlike `sync_executor::block_on`
/// which assumes every awaited future resolves synchronously). `current_thread` is used because
/// `block_on` (unlike `spawn`) has no `Send` bound, and `download()`'s future closes over
/// `Rc<RefCell<...>>` handles that are not `Send`.
///
/// TODO(phase-e): remove this once `HttpDownloader::add`/`get` are driven by `Loop::wait`'s
/// `FuturesUnordered` under a single top-level Runtime (see the async re-architecture design).
fn curl_runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build the temporary CurlDownloader bridge runtime")
    });
    &RT
}

impl HttpDownloader {
    const STATUS_QUEUED: i64 = 1;
    const STATUS_STARTED: i64 = 2;
    const STATUS_COMPLETED: i64 = 3;
    const STATUS_FAILED: i64 = 4;
    const STATUS_ABORTED: i64 = 5;

    /// @param IOInterface $io         The IO instance
    /// @param Config      $config     The config
    /// @param mixed[]     $options    The options
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

        let curl = if Self::is_curl_enabled() {
            Some(CurlDownloader::new(
                io.clone(),
                config.clone(),
                options.clone(),
                disable_tls,
            ))
        } else {
            None
        };

        let rfs = Some(std::rc::Rc::new(std::cell::RefCell::new(
            RemoteFilesystem::new(
                io.clone(),
                config.clone(),
                options.clone(),
                disable_tls,
                None,
            ),
        )));

        let mut max_jobs: i64 = 12;
        let max_jobs_env = Platform::get_env("COMPOSER_MAX_PARALLEL_HTTP");
        let max_jobs_env_mixed = match &max_jobs_env {
            Some(s) => PhpMixed::String(s.clone()),
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
            jobs: IndexMap::new(),
            options: self_options,
            running_jobs: 0,
            max_jobs,
            curl,
            rfs,
            id_gen: 0,
            disabled,
            allow_async: false,
            mock: None,
        }
    }

    /// Download a file synchronously
    pub fn get(
        &mut self,
        url: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Response> {
        if self.mock.is_some() {
            return self.mock_get(url, &options);
        }
        if url.is_empty() {
            return Err(InvalidArgumentException {
                message: "$url must not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }
        let job = self.add_job(
            Request {
                url: url.to_string(),
                options,
                copy_to: None,
            },
            true,
        )?;
        self.wait_id(Some(job.id))?;

        let response = self.get_response(job.id)?;

        Ok(response)
    }

    /// Create an async download operation
    pub async fn add(
        &mut self,
        url: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Response> {
        if self.mock.is_some() {
            return self.mock_get(url, &options);
        }
        if url.is_empty() {
            return Err(InvalidArgumentException {
                message: "$url must not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }
        let job = self.add_job(
            Request {
                url: url.to_string(),
                options,
                copy_to: None,
            },
            false,
        )?;
        self.wait_id(Some(job.id))?;

        self.get_response(job.id)
    }

    /// Copy a file synchronously
    pub fn copy(
        &mut self,
        url: &str,
        to: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Response> {
        if self.mock.is_some() {
            return self.mock_get(url, &options);
        }
        if url.is_empty() {
            return Err(InvalidArgumentException {
                message: "$url must not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }
        let job = self.add_job(
            Request {
                url: url.to_string(),
                options,
                copy_to: Some(to.to_string()),
            },
            true,
        )?;
        self.wait_id(Some(job.id))?;

        self.get_response(job.id)
    }

    /// Create an async copy operation
    pub async fn add_copy(
        &mut self,
        url: &str,
        to: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Response> {
        if self.mock.is_some() {
            return self.mock_get(url, &options);
        }
        if url.is_empty() {
            return Err(InvalidArgumentException {
                message: "$url must not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }
        let job = self.add_job(
            Request {
                url: url.to_string(),
                options,
                copy_to: Some(to.to_string()),
            },
            false,
        )?;
        self.wait_id(Some(job.id))?;

        self.get_response(job.id)
    }

    /// Retrieve the options set in the constructor
    pub fn get_options(&self) -> &IndexMap<String, PhpMixed> {
        &self.options
    }

    /// Merges new options
    pub fn set_options(&mut self, options: IndexMap<String, PhpMixed>) {
        self.options = array_replace_recursive(self.options.clone(), options);
    }

    /// @phpstan-param Request $request
    ///
    /// Queues a job and starts it if there is capacity. Mirrors PHP `addJob`: for non-curl (rfs)
    /// jobs the work runs synchronously here (PHP runs it in the Promise resolver during
    /// construction); for curl jobs the work is driven later by `start_job` / `count_active_jobs`.
    fn add_job(&mut self, mut request: Request, sync: bool) -> anyhow::Result<JobHandle> {
        request.options = array_replace_recursive(self.options.clone(), request.options);

        let id = self.id_gen;
        self.id_gen += 1;
        let origin = Url::get_origin(&self.config.borrow(), &request.url);

        if !sync && !self.allow_async {
            return Err(LogicException {
                message:
                    "You must use the HttpDownloader instance which is part of a Composer\\Loop instance to be able to run async http requests"
                        .to_string(),
                code: 0,
            }
            .into());
        }

        // capture username/password from URL if there is one
        let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
        if Preg::is_match3(
            r"{^https?://([^:/]+):([^@/]+)@([^/]+)}i",
            &request.url,
            Some(&mut m),
        ) {
            self.io.borrow_mut().set_authentication(
                origin.clone(),
                rawurldecode(
                    m.get(&CaptureKey::ByIndex(1))
                        .cloned()
                        .unwrap_or_default()
                        .as_str(),
                ),
                Some(rawurldecode(
                    m.get(&CaptureKey::ByIndex(2))
                        .cloned()
                        .unwrap_or_default()
                        .as_str(),
                )),
            );
        }

        let job = Job {
            id,
            status: Self::STATUS_QUEUED,
            request: request.clone(),
            sync,
            origin,
            response: None,
            exception: None,
        };
        let can_use_curl = self.can_use_curl(&job);
        self.jobs.insert(id, job);

        // PHP runs the resolver synchronously while constructing the Promise. For non-curl jobs
        // the resolver performs the blocking RemoteFilesystem download and resolves immediately.
        if !can_use_curl {
            self.run_rfs_job(id);
        }

        if self.running_jobs < self.max_jobs {
            self.start_job(id);
        }

        Ok(JobHandle { id })
    }

    /// Mirrors the non-curl branch of PHP `addJob`'s Promise resolver plus the `.then` side
    /// effects: performs the blocking RemoteFilesystem download and settles the job.
    fn run_rfs_job(&mut self, id: i64) {
        let (request, origin, url, options, copy_to) = {
            let job = self.jobs.get(&id).unwrap();
            (
                job.request.clone(),
                job.origin.clone(),
                job.request.url.clone(),
                job.request.options.clone(),
                job.request.copy_to.clone(),
            )
        };

        if let Some(job) = self.jobs.get_mut(&id) {
            job.status = Self::STATUS_STARTED;
        }

        let result: anyhow::Result<Response> = {
            let rfs = self.rfs.as_ref().unwrap().clone();
            (|| -> anyhow::Result<Response> {
                if let Some(copy_to) = copy_to.as_deref() {
                    let (_, headers) =
                        rfs.borrow_mut()
                            .copy(&origin, &url, copy_to, false, options.clone())?;

                    let code = RemoteFilesystem::find_status_code(&headers);
                    let body = Some(format!("{}~", copy_to));
                    Ok(Response::new(request.url.clone(), code, headers, body))
                } else {
                    let (result, headers) =
                        rfs.borrow_mut()
                            .get_contents(&origin, &url, false, options.clone())?;
                    let body = match result {
                        GetResult::Content(s) => Some(s),
                        _ => None,
                    };
                    let code = RemoteFilesystem::find_status_code(&headers);
                    Ok(Response::new(request.url.clone(), code, headers, body))
                }
            })()
        };

        self.settle_job(id, result);
    }

    /// Applies the effect of PHP's promise `.then` handlers: records the response/exception,
    /// transitions the job status and decrements the running-job counter.
    fn settle_job(&mut self, id: i64, result: anyhow::Result<Response>) {
        match result {
            Ok(response) => {
                if let Some(job) = self.jobs.get_mut(&id) {
                    job.status = Self::STATUS_COMPLETED;
                    job.response = Some(response);
                }
            }
            Err(e) => {
                if let Some(job) = self.jobs.get_mut(&id) {
                    job.status = Self::STATUS_FAILED;
                    job.exception = Some(e);
                }
            }
        }
        self.mark_job_done();
    }

    fn start_job(&mut self, id: i64) {
        let job_status = self.jobs.get(&id).map(|j| j.status);
        if job_status != Some(Self::STATUS_QUEUED) {
            return;
        }

        // start job
        if let Some(job) = self.jobs.get_mut(&id) {
            job.status = Self::STATUS_STARTED;
        }
        self.running_jobs += 1;

        let (request, origin, copy_to) = {
            let job = self.jobs.get(&id).unwrap();
            (
                job.request.clone(),
                job.origin.clone(),
                job.request.copy_to.clone(),
            )
        };
        let url = request.url.clone();
        let options = request.options.clone();

        if self.disabled {
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
                            .map(|v| v.as_string().unwrap_or("").to_string())
                            .collect::<Vec<_>>(),
                    );
                    stripos(&joined, "if-modified-since").is_some()
                } else if let Some(PhpMixed::Array(m)) = http_header.as_ref() {
                    let joined = implode(
                        "",
                        &m.values()
                            .map(|v| v.as_string().unwrap_or("").to_string())
                            .collect::<Vec<_>>(),
                    );
                    stripos(&joined, "if-modified-since").is_some()
                } else {
                    false
                }
            };
            if has_if_modified_since {
                let response = Ok(Response::new(
                    url.clone(),
                    Some(304),
                    Vec::new(),
                    Some(String::new()),
                ));
                self.settle_job(id, response);
            } else {
                let mut e = TransportException::new(
                    format!(
                        "Network disabled, request canceled: {}",
                        Url::sanitize(url.clone())
                    ),
                    499,
                );
                e.set_status_code(Some(499));
                self.settle_job(id, Err(e.into()));
            }

            return;
        }

        // curl branch: `CurlDownloader::download` now runs the whole redirect/retry/auth state
        // machine to completion itself and returns the settled result directly, so this drives
        // it through the temporary bridge runtime instead of PHP's promise resolve/reject firing
        // during tick(). PHP catches any exception from download() and rejects the job.
        let result: anyhow::Result<Response> = {
            let curl = self.curl.as_ref().unwrap();
            match curl_runtime().block_on(curl.download(&origin, &url, options, copy_to.as_deref()))
            {
                Ok(Ok(response)) => Ok(response),
                Ok(Err(transport_exception)) => Err(transport_exception.into()),
                Err(e) => Err(e),
            }
        };
        self.settle_job(id, result);
    }

    fn mark_job_done(&mut self) {
        self.running_jobs -= 1;
    }

    /// Wait for current async download jobs to complete
    ///
    /// @param int|null $index For internal use only, the job id
    pub fn wait(&mut self) -> anyhow::Result<()> {
        self.wait_id(None)
    }

    fn wait_id(&mut self, index: Option<i64>) -> anyhow::Result<()> {
        loop {
            let job_count = self.count_active_jobs(index)?;
            if job_count == 0 {
                break;
            }
        }
        Ok(())
    }

    /// @internal
    pub fn enable_async(&mut self) {
        self.allow_async = true;
    }

    /// @internal
    pub fn count_active_jobs(&mut self, index: Option<i64>) -> anyhow::Result<i64> {
        if self.running_jobs < self.max_jobs {
            let queued_ids: Vec<i64> = self
                .jobs
                .values()
                .filter(|j| j.status == Self::STATUS_QUEUED)
                .map(|j| j.id)
                .collect();
            for id in queued_ids {
                if self.running_jobs >= self.max_jobs {
                    break;
                }
                self.start_job(id);
            }
        }

        // Unlike the old tick()-driven curl path, `start_job` now settles curl jobs synchronously
        // (via the temporary bridge runtime), so no job is ever left lingering in STATUS_STARTED
        // by the time we get here — nothing left to poll or collect.

        if let Some(index) = index {
            return Ok(
                if self.jobs.get(&index).map(|j| j.status).unwrap_or(0) < Self::STATUS_COMPLETED {
                    1
                } else {
                    0
                },
            );
        }

        let mut active: i64 = 0;
        let ids: Vec<i64> = self.jobs.keys().copied().collect();
        for id in ids {
            let (status, sync) = {
                let j = self.jobs.get(&id).unwrap();
                (j.status, j.sync)
            };
            if status < Self::STATUS_COMPLETED {
                active += 1;
            } else if !sync {
                self.jobs.shift_remove(&id);
            }
        }

        Ok(active)
    }

    /// @param  int $index Job id
    fn get_response(&mut self, index: i64) -> anyhow::Result<Response> {
        if !self.jobs.contains_key(&index) {
            return Err(LogicException {
                message: "Invalid request id".to_string(),
                code: 0,
            }
            .into());
        }

        if self.jobs.get(&index).unwrap().status == Self::STATUS_FAILED {
            // PHP: assert(isset($this->jobs[$index]['exception']))
            let mut job = self.jobs.shift_remove(&index).unwrap();
            return Err(job.exception.take().unwrap());
        }

        if self.jobs.get(&index).unwrap().response.is_none() {
            return Err(LogicException {
                message: "Response not available yet, call wait() first".to_string(),
                code: 0,
            }
            .into());
        }

        let mut job = self.jobs.shift_remove(&index).unwrap();
        let resp = job.response.take().unwrap();

        Ok(resp)
    }

    /// @internal
    ///
    /// @param  array{warning?: string, info?: string, warning-versions?: string, info-versions?: string, warnings?: array<array{versions: string, message: string}>, infos?: array<array{versions: string, message: string}>} $data
    pub fn output_warnings(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        url: &str,
        data: &IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let clean_message = |msg: &str| -> anyhow::Result<String> {
            if !io.is_decorated() {
                return Ok(Preg::replace(
                    &format!("{{{}{}}}u", chr(27), "\\[[;\\d]*m"),
                    "",
                    msg,
                ));
            }

            Ok(msg.to_string())
        };

        // legacy warning/info keys
        for r#type in ["warning", "info"].iter() {
            let entry = data.get(*r#type);
            if entry.is_none() || shirabe_php_shim::empty(entry.unwrap()) {
                continue;
            }

            let versions_key = format!("{}-versions", r#type);
            if let Some(versions_value) = data.get(&versions_key)
                && !shirabe_php_shim::empty(versions_value)
            {
                let version_parser: VersionParser = VersionParser::new();
                let constraint =
                    version_parser.parse_constraints(versions_value.as_string().unwrap_or(""))?;
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
                msg = clean_message(entry.unwrap().as_string().unwrap_or(""))?
            ));
        }

        // modern Composer 2.2+ format with support for multiple warning/info messages
        for key in ["warnings", "infos"].iter() {
            let entry = data.get(*key);
            if entry.is_none() || shirabe_php_shim::empty(entry.unwrap()) {
                continue;
            }

            let version_parser: VersionParser = VersionParser::new();
            if let Some(PhpMixed::List(list)) = entry {
                for spec in list {
                    let r#type = substr(key, 0, Some(-1));
                    if let PhpMixed::Array(spec_map) = spec {
                        let constraint = version_parser.parse_constraints(
                            spec_map
                                .get("versions")
                                .and_then(|v| v.as_string())
                                .unwrap_or(""),
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
                                spec_map
                                    .get("message")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                            )?
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// @internal
    ///
    /// @return ?string[]
    pub fn get_exception_hints(e: &anyhow::Error) -> Option<Vec<String>> {
        let e_as_transport: Option<&TransportException> = e.downcast_ref::<TransportException>();
        e_as_transport?;
        let e_as_transport = e_as_transport.unwrap();

        if strpos(e_as_transport.get_message(), "Resolving timed out").is_some()
            || strpos(e_as_transport.get_message(), "Could not resolve host").is_some()
        {
            Silencer::suppress(None);
            let mut ctx_options: IndexMap<String, PhpMixed> = IndexMap::new();
            let mut ssl_map: IndexMap<String, PhpMixed> = IndexMap::new();
            ssl_map.insert("verify_peer".to_string(), PhpMixed::Bool(false));
            ctx_options.insert("ssl".to_string(), PhpMixed::Array(ssl_map));
            let mut http_map: IndexMap<String, PhpMixed> = IndexMap::new();
            http_map.insert("follow_location".to_string(), PhpMixed::Bool(false));
            http_map.insert("ignore_errors".to_string(), PhpMixed::Bool(true));
            ctx_options.insert("http".to_string(), PhpMixed::Array(http_map));
            // TODO(phase-c): file_get_contents only takes a path; the stream context arg is dropped
            // until the PHP stream-context layer is modeled.
            let _ = stream_context_create(&ctx_options, None);
            let test_connectivity = file_get_contents("https://8.8.8.8");
            Silencer::restore();
            if test_connectivity.is_some() {
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

    /// @param  Job  $job
    fn can_use_curl(&self, job: &Job) -> bool {
        if self.curl.is_none() {
            return false;
        }

        if !Preg::is_match(r"{^https?://}i", &job.request.url) {
            return false;
        }

        let allow_self_signed = job.request.options.get("ssl").and_then(|v| match v {
            PhpMixed::Array(m) => m.get("allow_self_signed").cloned(),
            _ => None,
        });
        if let Some(v) = allow_self_signed
            && !shirabe_php_shim::empty(&v)
        {
            return false;
        }

        true
    }

    /// @internal
    pub fn is_curl_enabled() -> bool {
        extension_loaded("curl")
            && function_exists("curl_multi_exec")
            && function_exists("curl_multi_init")
    }

    /// For testing only. Builds an HttpDownloader whose request methods are fully
    /// short-circuited by the mock (see [`HttpDownloader::__expects`]), without
    /// constructing the real curl/rfs backends. Mirrors HttpDownloaderMock, which
    /// extends HttpDownloader but never performs curl I/O.
    pub fn __new_mock(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
    ) -> Self {
        Self {
            io,
            config,
            jobs: IndexMap::new(),
            options: IndexMap::new(),
            running_jobs: 0,
            max_jobs: 12,
            curl: None,
            rfs: None,
            id_gen: 0,
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
                shirabe_php_shim::PHP_EOL,
                urls.join(shirabe_php_shim::PHP_EOL),
                shirabe_php_shim::PHP_EOL,
                shirabe_php_shim::PHP_EOL,
                shirabe_php_shim::PHP_EOL,
                mock.log.join(shirabe_php_shim::PHP_EOL),
            );
        }
    }

    /// For testing only. Mirrors HttpDownloaderMock::get: consumes the next matching expectation
    /// (or falls back to the default handler when not strict) and returns the canned response.
    fn mock_get(
        &mut self,
        file_url: &str,
        options: &IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Response> {
        if file_url.is_empty() {
            return Err(LogicException {
                message: "url cannot be an empty string".to_string(),
                code: 0,
            }
            .into());
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
            mock.log[..mock.log.len() - 1].join(shirabe_php_shim::PHP_EOL)
        } else {
            String::new()
        };
        panic!(
            "Received unexpected request for \"{}\" with options \"{}\"{}{}{}Received calls:{}{}",
            file_url,
            serde_json::to_string(options).unwrap_or_default(),
            shirabe_php_shim::PHP_EOL,
            next_expected,
            shirabe_php_shim::PHP_EOL,
            shirabe_php_shim::PHP_EOL,
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

#[derive(Debug, Clone, Copy)]
struct JobHandle {
    id: i64,
}
