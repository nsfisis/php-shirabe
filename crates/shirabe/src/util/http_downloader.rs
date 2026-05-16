//! ref: composer/src/Composer/Util/HttpDownloader.php

use anyhow::Result;
use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise::promise::Promise;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::{
    array_replace_recursive, chr, extension_loaded, file_get_contents, function_exists, implode,
    is_numeric, max, min, rawurldecode, stream_context_create, stripos, strpos, substr, ucfirst,
    InvalidArgumentException, LogicException, PhpMixed, Silencer,
};
use shirabe_semver::constraint::constraint::Constraint;

use crate::composer::Composer;
use crate::config::Config;
use crate::downloader::transport_exception::TransportException;
use crate::exception::irrecoverable_download_exception::IrrecoverableDownloadException;
use crate::io::io_interface::IOInterface;
use crate::package::version::version_parser::VersionParser;
use crate::util::http::curl_downloader::CurlDownloader;
use crate::util::http::response::Response;
use crate::util::platform::Platform;
use crate::util::remote_filesystem::RemoteFilesystem;
use crate::util::stream_context_factory::StreamContextFactory;
use crate::util::url::Url;

/// @phpstan-type Request array{url: non-empty-string, options: mixed[], copyTo: string|null}
/// @phpstan-type Job array{id: int, status: int, request: Request, sync: bool, origin: string, resolve?: callable, reject?: callable, curl_id?: int, response?: Response, exception?: \Throwable}
#[derive(Debug)]
pub struct HttpDownloader {
    /// @var IOInterface
    io: Box<dyn IOInterface>,
    /// @var Config
    config: Config,
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

#[derive(Debug)]
struct Job {
    id: i64,
    status: i64,
    request: Request,
    sync: bool,
    origin: String,
    resolve: Option<Box<dyn Fn(PhpMixed) + Send + Sync>>,
    reject: Option<Box<dyn Fn(PhpMixed) + Send + Sync>>,
    curl_id: Option<i64>,
    response: Option<Response>,
    exception: Option<anyhow::Error>,
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
        io: Box<dyn IOInterface>,
        config: Config,
        options: IndexMap<String, PhpMixed>,
        disable_tls: bool,
    ) -> Self {
        let disabled = Platform::get_env("COMPOSER_DISABLE_NETWORK")
            .as_bool()
            .unwrap_or(false);

        // Setup TLS options
        // The cafile option can be set via config.json
        let mut self_options: IndexMap<String, PhpMixed> = IndexMap::new();
        if disable_tls == false {
            self_options = StreamContextFactory::get_tls_defaults(&options, Some(&*io));
        }

        // handle the other externally set options normally.
        self_options = array_replace_recursive(
            PhpMixed::Array(
                self_options
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
            PhpMixed::Array(
                options
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        )
        .as_array()
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), (**v).clone()))
                .collect()
        })
        .unwrap_or_default();

        let curl = if Self::is_curl_enabled() {
            Some(CurlDownloader::new(
                &*io,
                &config,
                options.clone(),
                disable_tls,
            ))
        } else {
            None
        };

        let rfs = Some(RemoteFilesystem::new(
            &*io,
            &config,
            options.clone(),
            disable_tls,
        ));

        let mut max_jobs: i64 = 12;
        let max_jobs_env = Platform::get_env("COMPOSER_MAX_PARALLEL_HTTP");
        if is_numeric(&max_jobs_env) {
            max_jobs = max(
                1,
                min(50, max_jobs_env.as_string().unwrap_or("0").parse().unwrap_or(0)),
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
    pub fn get(
        &mut self,
        url: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> Result<Response> {
        if "" == url {
            return Err(InvalidArgumentException {
                message: "$url must not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }
        let (job, promise) = self.add_job(
            Request {
                url: url.to_string(),
                options,
                copy_to: None,
            },
            true,
        )?;
        promise.then_with(
            None,
            Some(Box::new(|_e: PhpMixed| {
                // suppress error as it is rethrown to the caller by getResponse() a few lines below
                PhpMixed::Null
            })),
        );
        self.wait_id(Some(job.id))?;

        let response = self.get_response(job.id)?;

        Ok(response)
    }

    /// Create an async download operation
    pub fn add(
        &mut self,
        url: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> Result<Box<dyn PromiseInterface>> {
        if "" == url {
            return Err(InvalidArgumentException {
                message: "$url must not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }
        let (_, promise) = self.add_job(
            Request {
                url: url.to_string(),
                options,
                copy_to: None,
            },
            false,
        )?;

        Ok(promise)
    }

    /// Copy a file synchronously
    pub fn copy(
        &mut self,
        url: &str,
        to: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> Result<Response> {
        if "" == url {
            return Err(InvalidArgumentException {
                message: "$url must not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }
        let (job, _) = self.add_job(
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
    pub fn add_copy(
        &mut self,
        url: &str,
        to: &str,
        options: IndexMap<String, PhpMixed>,
    ) -> Result<Box<dyn PromiseInterface>> {
        if "" == url {
            return Err(InvalidArgumentException {
                message: "$url must not be an empty string".to_string(),
                code: 0,
            }
            .into());
        }
        let (_, promise) = self.add_job(
            Request {
                url: url.to_string(),
                options,
                copy_to: Some(to.to_string()),
            },
            false,
        )?;

        Ok(promise)
    }

    /// Retrieve the options set in the constructor
    pub fn get_options(&self) -> &IndexMap<String, PhpMixed> {
        &self.options
    }

    /// Merges new options
    pub fn set_options(&mut self, options: IndexMap<String, PhpMixed>) {
        self.options = array_replace_recursive(
            PhpMixed::Array(
                self.options
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
            PhpMixed::Array(
                options
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        )
        .as_array()
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), (**v).clone()))
                .collect()
        })
        .unwrap_or_default();
    }

    /// @phpstan-param Request $request
    /// @return array{Job, PromiseInterface}
    fn add_job(
        &mut self,
        mut request: Request,
        sync: bool,
    ) -> Result<(JobHandle, Box<dyn PromiseInterface>)> {
        request.options = array_replace_recursive(
            PhpMixed::Array(
                self.options
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
            PhpMixed::Array(
                request
                    .options
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        )
        .as_array()
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), (**v).clone()))
                .collect()
        })
        .unwrap_or_default();

        let id = self.id_gen;
        self.id_gen += 1;
        let origin = Url::get_origin(&self.config, &request.url);

        let job = Job {
            id,
            status: Self::STATUS_QUEUED,
            request: request.clone(),
            sync,
            origin: origin.clone(),
            resolve: None,
            reject: None,
            curl_id: None,
            response: None,
            exception: None,
        };

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
        if let Some(m) = Preg::is_match_strict_groups(
            r"{^https?://([^:/]+):([^@/]+)@([^/]+)}i",
            &request.url,
        ) {
            self.io.set_authentication(
                origin.clone(),
                rawurldecode(m.get(1).cloned().unwrap_or_default().as_str()),
                Some(rawurldecode(m.get(2).cloned().unwrap_or_default().as_str())),
            );
        }

        // TODO(phase-b): build resolver/canceler closures bound to &mut self.jobs; needs Rc<RefCell> wiring
        let _ = (&self.rfs, &self.curl);

        let resolver: Box<dyn Fn(_, _)> = Box::new(|_resolve, _reject| {
            // TODO(phase-b)
        });
        let canceler: Box<dyn Fn()> = Box::new(|| {
            // PHP canceler logic — TODO(phase-b)
            let _ = IrrecoverableDownloadException {
                inner: TransportException::new(
                    "Download canceled".to_string(),
                    0,
                ),
            };
            let _ = Url::sanitize("");
        });
        let _ = (resolver, canceler);

        let promise = Promise::new(
            Box::new(|_resolve, _reject| {}),
            Box::new(|| {}),
        );
        // TODO(phase-b): wire promise.then() side-effects: mark job done & store response/exception
        let promise: Box<dyn PromiseInterface> = Box::new(promise);

        self.jobs.insert(id, job);

        if self.running_jobs < self.max_jobs {
            self.start_job(id);
        }

        Ok((JobHandle { id }, promise))
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
            (job.request.clone(), job.origin.clone(), job.request.copy_to.clone())
        };
        let url = request.url.clone();
        let options = request.options.clone();
        let _ = origin;

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
                let mut req_map: IndexMap<String, PhpMixed> = IndexMap::new();
                req_map.insert("url".to_string(), PhpMixed::String(url.clone()));
                let _ = Response::new(req_map, 304, IndexMap::new(), String::new());
                // job.resolve(response) — TODO(phase-b)
            } else {
                let mut e = TransportException::new(
                    format!("Network disabled, request canceled: {}", Url::sanitize(&url)),
                    499,
                );
                e.set_status_code(499);
                // job.reject(e) — TODO(phase-b)
                let _ = e;
            }

            return;
        }

        let _ = copy_to;
        // TODO(phase-b): try { curl->download(...) } catch (...) { reject(e) }
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
            let job_count = self.count_active_jobs(index);
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
    pub fn count_active_jobs(&mut self, index: Option<i64>) -> i64 {
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
            curl.tick();
        }

        if let Some(index) = index {
            return if self.jobs.get(&index).map(|j| j.status).unwrap_or(0) < Self::STATUS_COMPLETED
            {
                1
            } else {
                0
            };
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

        active
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
        io: &dyn IOInterface,
        url: &str,
        data: &IndexMap<String, PhpMixed>,
    ) -> Result<()> {
        let clean_message = |msg: &str| -> String {
            if !io.is_decorated() {
                return Preg::replace(
                    &format!("{{{}{}}}u", chr(27), "\\[[;\\d]*m"),
                    "",
                    msg,
                );
            }

            msg.to_string()
        };

        // legacy warning/info keys
        for r#type in ["warning", "info"].iter() {
            let entry = data.get(*r#type);
            if entry.is_none() || shirabe_php_shim::empty(entry.unwrap()) {
                continue;
            }

            let versions_key = format!("{}-versions", r#type);
            if let Some(versions_value) = data.get(&versions_key) {
                if !shirabe_php_shim::empty(versions_value) {
                    // TODO(phase-b): VersionParser::new
                    let version_parser: VersionParser = todo!("VersionParser::new()");
                    let constraint = version_parser
                        .parse_constraints(versions_value.as_string().unwrap_or(""))?;
                    let composer_constraint = Constraint::new(
                        "==",
                        &version_parser.normalize(&Composer::get_version(), None)?,
                    );
                    if !constraint.matches(&composer_constraint) {
                        continue;
                    }
                }
            }

            io.write_error(&format!(
                "<{tp}>{capitalized} from {url}: {msg}</{tp}>",
                tp = r#type,
                capitalized = ucfirst(r#type),
                url = Url::sanitize(url),
                msg = clean_message(entry.unwrap().as_string().unwrap_or(""))
            ));
        }

        // modern Composer 2.2+ format with support for multiple warning/info messages
        for key in ["warnings", "infos"].iter() {
            let entry = data.get(*key);
            if entry.is_none() || shirabe_php_shim::empty(entry.unwrap()) {
                continue;
            }

            // TODO(phase-b): VersionParser::new
            let version_parser: VersionParser = todo!("VersionParser::new()");
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
                        let composer_constraint = Constraint::new(
                            "==",
                            &version_parser.normalize(&Composer::get_version(), None)?,
                        );
                        if !constraint.matches(&composer_constraint) {
                            continue;
                        }

                        io.write_error(&format!(
                            "<{tp}>{capitalized} from {url}: {msg}</{tp}>",
                            tp = r#type,
                            capitalized = ucfirst(&r#type),
                            url = Url::sanitize(url),
                            msg = clean_message(
                                spec_map
                                    .get("message")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                            )
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
        // TODO(phase-b): `$e instanceof TransportException`
        let e_as_transport: Option<&TransportException> = e.downcast_ref::<TransportException>();
        if e_as_transport.is_none() {
            return None;
        }
        let e_as_transport = e_as_transport.unwrap();

        if false != strpos(e_as_transport.get_message(), "Resolving timed out").is_some()
            || false != strpos(e_as_transport.get_message(), "Could not resolve host").is_some()
        {
            Silencer::suppress();
            let mut ctx_options: IndexMap<String, PhpMixed> = IndexMap::new();
            let mut ssl_map: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            ssl_map.insert("verify_peer".to_string(), Box::new(PhpMixed::Bool(false)));
            ctx_options.insert("ssl".to_string(), PhpMixed::Array(ssl_map));
            let mut http_map: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            http_map.insert("follow_location".to_string(), Box::new(PhpMixed::Bool(false)));
            http_map.insert("ignore_errors".to_string(), Box::new(PhpMixed::Bool(true)));
            ctx_options.insert("http".to_string(), PhpMixed::Array(http_map));
            let test_connectivity = file_get_contents(
                "https://8.8.8.8",
                false,
                Some(stream_context_create(ctx_options)),
            );
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

        let allow_self_signed = job
            .request
            .options
            .get("ssl")
            .and_then(|v| match v {
                PhpMixed::Array(m) => m.get("allow_self_signed").cloned(),
                _ => None,
            });
        if let Some(v) = allow_self_signed {
            if !shirabe_php_shim::empty(&v) {
                return false;
            }
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
