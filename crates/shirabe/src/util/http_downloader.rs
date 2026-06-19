//! ref: composer/src/Composer/Util/HttpDownloader.php

use anyhow::Result;
use indexmap::IndexMap;

use crate::util::Silencer;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, array_replace_recursive, chr,
    extension_loaded, file_get_contents, function_exists, implode, is_numeric, max, min,
    rawurldecode, stream_context_create, stripos, strpos, substr, ucfirst,
};
use shirabe_semver::constraint::AnyConstraint;
use shirabe_semver::constraint::SimpleConstraint;

use crate::composer;
use crate::composer::ComposerHandle;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::version::VersionParser;
use crate::util::GetResult;
use crate::util::Platform;
use crate::util::RemoteFilesystem;
use crate::util::StreamContextFactory;
use crate::util::Url;
use crate::util::http::CurlDownloader;
use crate::util::http::Response;

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
    rfs: Option<RemoteFilesystem>,
    /// @var int
    id_gen: i64,
    /// @var bool
    disabled: bool,
    /// @var bool
    allow_async: bool,
}

struct Job {
    id: i64,
    status: i64,
    request: Request,
    sync: bool,
    origin: String,
    curl_id: Option<i64>,
    response: Option<Response>,
    exception: Option<anyhow::Error>,
    /// Completion slot written by the curl resolve/reject closures (driven by `curl.tick()`)
    /// and read by `count_active_jobs`. Uses `Arc<Mutex>` because `CurlDownloader::download`
    /// requires `Send + Sync` callbacks.
    settled: std::sync::Arc<std::sync::Mutex<Option<anyhow::Result<Response>>>>,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("request", &self.request)
            .field("sync", &self.sync)
            .field("origin", &self.origin)
            .field("curl_id", &self.curl_id)
            .field("response", &self.response)
            .field("exception", &self.exception)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct Request {
    url: String,
    options: IndexMap<String, PhpMixed>,
    copy_to: Option<String>,
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

        let rfs = Some(RemoteFilesystem::new(
            io.clone(),
            config.clone(),
            options.clone(),
            disable_tls,
            None,
        ));

        let mut max_jobs: i64 = 12;
        let max_jobs_env = Platform::get_env("COMPOSER_MAX_PARALLEL_HTTP");
        let max_jobs_env_mixed = match &max_jobs_env {
            Some(s) => PhpMixed::String(s.clone()),
            None => PhpMixed::Bool(false),
        };
        if is_numeric(&max_jobs_env_mixed) {
            max_jobs = max(
                1,
                min(
                    50,
                    max_jobs_env.as_deref().unwrap_or("0").parse().unwrap_or(0),
                ),
            );
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
        }
    }

    /// Download a file synchronously
    pub fn get(&mut self, url: &str, options: IndexMap<String, PhpMixed>) -> Result<Response> {
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
    ) -> Result<Response> {
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
    ) -> Result<Response> {
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
    ) -> Result<Response> {
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
    fn add_job(&mut self, mut request: Request, sync: bool) -> Result<JobHandle> {
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
            curl_id: None,
            response: None,
            exception: None,
            settled: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
            let rfs = self.rfs.as_mut().unwrap();
            (|| -> anyhow::Result<Response> {
                if let Some(copy_to) = copy_to.as_deref() {
                    rfs.copy(&origin, &url, copy_to, false, options.clone())?;

                    let headers = rfs.get_last_headers().to_vec();
                    let code = RemoteFilesystem::find_status_code(&headers);
                    let body = Some(format!("{}~", copy_to));
                    Ok(Response::new(request.url.clone(), code, headers, body))
                } else {
                    let body = match rfs.get_contents(&origin, &url, false, options.clone())? {
                        GetResult::Content(s) => Some(s),
                        _ => None,
                    };
                    let headers = rfs.get_last_headers().to_vec();
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
                if let Some(PhpMixed::List(list)) = http_header.as_deref() {
                    let joined = implode(
                        "",
                        &list
                            .iter()
                            .map(|v| v.as_string().unwrap_or("").to_string())
                            .collect::<Vec<_>>(),
                    );
                    stripos(&joined, "if-modified-since").is_some()
                } else if let Some(PhpMixed::Array(m)) = http_header.as_deref() {
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

        // curl branch: register the request with the curl multi handle. Completion is delivered
        // asynchronously by curl.tick() into the job's `settled` slot (read by count_active_jobs).
        // PHP catches any exception from download() and rejects the job.
        let settled = self.jobs.get(&id).unwrap().settled.clone();
        let settled_for_reject = settled.clone();
        let resolve: Box<dyn Fn(PhpMixed) + Send + Sync> = Box::new(move |_response: PhpMixed| {
            // TODO(phase-c-promise): curl.tick() delivers the response as PhpMixed here; convert it
            // into a Response and store Ok(..) in `settled`. Bottoms at the todo!() curl I/O.
            let _ = &settled;
        });
        let reject: Box<dyn Fn(PhpMixed) + Send + Sync> = Box::new(move |_error: PhpMixed| {
            // TODO(phase-c-promise): convert the PhpMixed error into anyhow::Error and store
            // Err(..) in `settled`. Bottoms at the todo!() curl I/O.
            let _ = &settled_for_reject;
        });

        let download_result = {
            let curl = self.curl.as_mut().unwrap();
            curl.download(resolve, reject, &origin, &url, options, copy_to.as_deref())
        };
        match download_result {
            Ok(Ok(curl_id)) => {
                if let Some(job) = self.jobs.get_mut(&id) {
                    job.curl_id = Some(curl_id);
                }
            }
            Ok(Err(e)) => {
                self.settle_job(id, Err(e.into()));
            }
            Err(e) => {
                self.settle_job(id, Err(e));
            }
        }
    }

    fn mark_job_done(&mut self) {
        self.running_jobs -= 1;
    }

    /// Wait for current async download jobs to complete
    ///
    /// @param int|null $index For internal use only, the job id
    pub fn wait(&mut self) -> Result<()> {
        self.wait_id(None)
    }

    fn wait_id(&mut self, index: Option<i64>) -> Result<()> {
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
    pub fn count_active_jobs(&mut self, index: Option<i64>) -> Result<i64> {
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

        if let Some(curl) = self.curl.as_mut() {
            curl.tick()?;
        }

        // Apply completions delivered by curl.tick() into each started job's `settled` slot.
        // This reproduces the effect of PHP's resolve/reject callbacks firing during tick().
        let started_ids: Vec<i64> = self
            .jobs
            .values()
            .filter(|j| j.status == Self::STATUS_STARTED)
            .map(|j| j.id)
            .collect();
        for id in started_ids {
            let settled = self.jobs.get(&id).unwrap().settled.lock().unwrap().take();
            if let Some(result) = settled {
                self.settle_job(id, result);
            }
        }

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
    fn get_response(&mut self, index: i64) -> Result<Response> {
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
    ) -> Result<()> {
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
                    if let PhpMixed::Array(spec_map) = spec.as_ref() {
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
            let mut ssl_map: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            ssl_map.insert("verify_peer".to_string(), Box::new(PhpMixed::Bool(false)));
            ctx_options.insert("ssl".to_string(), PhpMixed::Array(ssl_map));
            let mut http_map: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            http_map.insert(
                "follow_location".to_string(),
                Box::new(PhpMixed::Bool(false)),
            );
            http_map.insert("ignore_errors".to_string(), Box::new(PhpMixed::Bool(true)));
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
}

#[derive(Debug, Clone, Copy)]
struct JobHandle {
    id: i64,
}
