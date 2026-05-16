//! ref: composer/src/Composer/Util/Http/CurlDownloader.php

use std::sync::atomic::{AtomicBool, Ordering};

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    array_diff, array_diff_key, array_merge, count, curl_errno, curl_error, curl_getinfo,
    curl_handle_id, curl_init, curl_multi_add_handle, curl_multi_exec, curl_multi_info_read,
    curl_multi_init, curl_multi_select, curl_multi_setopt, curl_setopt, curl_setopt_array,
    curl_share_init, curl_share_setopt, curl_strerror, curl_version, defined, explode, fclose,
    fopen, function_exists, implode, in_array, ini_get, is_resource, json_decode, max, parse_url,
    preg_quote, rename, restore_error_handler, rewind, rtrim, set_error_handler_closure, sprintf,
    str_contains, strpos, stream_get_contents, stream_get_contents_with_max, stripos, substr,
    unlink_silent, usleep, var_export, CurlMultiHandle, CurlShareHandle, LogicException, PhpMixed,
    RuntimeException, CURL_HTTP_VERSION_2_0, CURL_HTTP_VERSION_3, CURL_IPRESOLVE_V4,
    CURL_IPRESOLVE_V6, CURL_LOCK_DATA_COOKIE, CURL_LOCK_DATA_DNS, CURL_LOCK_DATA_SSL_SESSION,
    CURL_VERSION_HTTP2, CURL_VERSION_HTTP3, CURL_VERSION_LIBZ, CURLE_OK, CURLM_BAD_EASY_HANDLE,
    CURLM_BAD_HANDLE, CURLM_CALL_MULTI_PERFORM, CURLM_INTERNAL_ERROR, CURLM_OK, CURLM_OUT_OF_MEMORY,
    CURLMOPT_MAX_HOST_CONNECTIONS, CURLMOPT_PIPELINING, CURLOPT_CONNECTTIMEOUT, CURLOPT_ENCODING,
    CURLOPT_FILE, CURLOPT_FOLLOWLOCATION, CURLOPT_HTTP_VERSION, CURLOPT_IPRESOLVE,
    CURLOPT_PROTOCOLS, CURLOPT_SHARE, CURLOPT_TIMEOUT, CURLOPT_URL, CURLOPT_WRITEHEADER,
    CURLPROTO_HTTP, CURLPROTO_HTTPS, CURLSHOPT_SHARE, PHP_VERSION_ID,
};

use crate::config::Config;
use crate::downloader::max_file_size_exceeded_exception::MaxFileSizeExceededException;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::util::auth_helper::{AuthHelper, PromptAuthResult, StoreAuth};
use crate::util::http::curl_response::CurlResponse;
use crate::util::http::proxy_manager::ProxyManager;
use crate::util::http_downloader::HttpDownloader;
use crate::util::platform::Platform;
use crate::util::stream_context_factory::StreamContextFactory;
use crate::util::url::Url;
// use shirabe_external_packages::react::promise::promise::Promise; // typehint only in PHP

/// @phpstan-type Attributes array{retryAuthFailure: bool, redirects: int<0, max>, retries: int<0, max>, storeAuth: 'prompt'|bool, ipResolve: 4|6|null}
/// @phpstan-type Job array{url: non-empty-string, origin: string, attributes: Attributes, options: mixed[], progress: mixed[], curlHandle: \CurlHandle, filename: string|null, headerHandle: resource, bodyHandle: resource, resolve: callable, reject: callable, primaryIp: string}
#[derive(Debug)]
pub struct CurlDownloader {
    /// @var \CurlMultiHandle
    multi_handle: CurlMultiHandle,
    /// @var \CurlShareHandle
    share_handle: Option<CurlShareHandle>,
    /// @var Job[]
    jobs: IndexMap<i64, IndexMap<String, PhpMixed>>,
    /// @var IOInterface
    io: Box<dyn IOInterface>,
    /// @var Config
    config: Config,
    /// @var AuthHelper
    auth_helper: AuthHelper,
    /// @var float
    select_timeout: f64,
    /// @var int
    max_redirects: i64,
    /// @var int
    max_retries: i64,
    /// @var array<int, string[]>
    pub(crate) multi_errors: IndexMap<i64, Vec<String>>,
}

/// Known libcurl's broken versions when proxy is in use with HTTP/2
/// multiplexing.
///
/// @var list<non-empty-string>
const BAD_MULTIPLEXING_CURL_VERSIONS: &[&str] = &["7.87.0", "7.88.0", "7.88.1"];

/// @var mixed[]
fn options_static() -> IndexMap<String, IndexMap<String, i64>> {
    let mut http: IndexMap<String, i64> = IndexMap::new();
    http.insert("method".to_string(), shirabe_php_shim::CURLOPT_CUSTOMREQUEST);
    http.insert("content".to_string(), shirabe_php_shim::CURLOPT_POSTFIELDS);
    http.insert("header".to_string(), shirabe_php_shim::CURLOPT_HTTPHEADER);
    http.insert("timeout".to_string(), CURLOPT_TIMEOUT);

    let mut ssl: IndexMap<String, i64> = IndexMap::new();
    ssl.insert("cafile".to_string(), shirabe_php_shim::CURLOPT_CAINFO);
    ssl.insert("capath".to_string(), shirabe_php_shim::CURLOPT_CAPATH);
    ssl.insert("verify_peer".to_string(), shirabe_php_shim::CURLOPT_SSL_VERIFYPEER);
    ssl.insert("verify_peer_name".to_string(), shirabe_php_shim::CURLOPT_SSL_VERIFYHOST);
    ssl.insert("local_cert".to_string(), shirabe_php_shim::CURLOPT_SSLCERT);
    ssl.insert("local_pk".to_string(), shirabe_php_shim::CURLOPT_SSLKEY);
    ssl.insert("passphrase".to_string(), shirabe_php_shim::CURLOPT_SSLKEYPASSWD);

    let mut out: IndexMap<String, IndexMap<String, i64>> = IndexMap::new();
    out.insert("http".to_string(), http);
    out.insert("ssl".to_string(), ssl);
    out
}

/// @var array<string, true>
fn time_info_static() -> IndexMap<String, bool> {
    let mut m: IndexMap<String, bool> = IndexMap::new();
    m.insert("total_time".to_string(), true);
    m.insert("namelookup_time".to_string(), true);
    m.insert("connect_time".to_string(), true);
    m.insert("pretransfer_time".to_string(), true);
    m.insert("starttransfer_time".to_string(), true);
    m.insert("redirect_time".to_string(), true);
    m
}

/// Function-static `$timeoutWarning` from `tick()`.
static TIMEOUT_WARNING: AtomicBool = AtomicBool::new(false);

impl CurlDownloader {
    /// @param mixed[] $options
    pub fn new(
        io: Box<dyn IOInterface>,
        config: Config,
        _options: IndexMap<String, PhpMixed>,
        _disable_tls: bool,
    ) -> Self {
        let multi_handle = curl_multi_init();
        let mut share_handle: Option<CurlShareHandle> = None;

        if function_exists("curl_multi_setopt") {
            let version = curl_version();
            let proxy_with_bad_version = ProxyManager::get_instance()
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .has_proxy()
                && version.is_some()
                && in_array(
                    version
                        .as_ref()
                        .and_then(|v| v.get("version"))
                        .map(|b| (**b).clone())
                        .unwrap_or(PhpMixed::Null),
                    &PhpMixed::List(
                        BAD_MULTIPLEXING_CURL_VERSIONS
                            .iter()
                            .map(|s| Box::new(PhpMixed::String((*s).to_string())))
                            .collect(),
                    ),
                    true,
                );
            if proxy_with_bad_version {
                // Disable HTTP/2 multiplexing for some broken versions of libcurl.
                //
                // In certain versions of libcurl when proxy is in use with HTTP/2
                // multiplexing, connections will continue stacking up. This was
                // fixed in libcurl 8.0.0 in curl/curl@821f6e2a89de8aec1c7da3c0f381b92b2b801efc
                curl_multi_setopt(
                    &multi_handle,
                    CURLMOPT_PIPELINING,
                    PhpMixed::Int(0 /* CURLPIPE_NOTHING */),
                );
            } else {
                curl_multi_setopt(
                    &multi_handle,
                    CURLMOPT_PIPELINING,
                    PhpMixed::Int(if PHP_VERSION_ID >= 70400 {
                        2 /* CURLPIPE_MULTIPLEX */
                    } else {
                        3 /* CURLPIPE_HTTP1 | CURLPIPE_MULTIPLEX */
                    }),
                );
            }
            if defined("CURLMOPT_MAX_HOST_CONNECTIONS") && !defined("HHVM_VERSION") {
                curl_multi_setopt(&multi_handle, CURLMOPT_MAX_HOST_CONNECTIONS, PhpMixed::Int(8));
            }
        }

        if function_exists("curl_share_init") {
            let sh = curl_share_init();
            curl_share_setopt(&sh, CURLSHOPT_SHARE, PhpMixed::Int(CURL_LOCK_DATA_COOKIE));
            curl_share_setopt(&sh, CURLSHOPT_SHARE, PhpMixed::Int(CURL_LOCK_DATA_DNS));
            curl_share_setopt(&sh, CURLSHOPT_SHARE, PhpMixed::Int(CURL_LOCK_DATA_SSL_SESSION));
            share_handle = Some(sh);
        }

        // TODO(phase-b): clone io/config for AuthHelper construction without consuming.
        let auth_helper = AuthHelper::new(unsafe { std::mem::zeroed() }, unsafe {
            std::mem::zeroed()
        });

        let mut multi_errors: IndexMap<i64, Vec<String>> = IndexMap::new();
        multi_errors.insert(
            CURLM_BAD_HANDLE,
            vec![
                "CURLM_BAD_HANDLE".to_string(),
                "The passed-in handle is not a valid CURLM handle.".to_string(),
            ],
        );
        multi_errors.insert(
            CURLM_BAD_EASY_HANDLE,
            vec![
                "CURLM_BAD_EASY_HANDLE".to_string(),
                "An easy handle was not good/valid. It could mean that it isn't an easy handle at all, or possibly that the handle already is in used by this or another multi handle.".to_string(),
            ],
        );
        multi_errors.insert(
            CURLM_OUT_OF_MEMORY,
            vec![
                "CURLM_OUT_OF_MEMORY".to_string(),
                "You are doomed.".to_string(),
            ],
        );
        multi_errors.insert(
            CURLM_INTERNAL_ERROR,
            vec![
                "CURLM_INTERNAL_ERROR".to_string(),
                "This can only be returned if libcurl bugs. Please report it to us!".to_string(),
            ],
        );

        Self {
            multi_handle,
            share_handle,
            jobs: IndexMap::new(),
            io,
            config,
            auth_helper,
            select_timeout: 5.0,
            max_redirects: 20,
            max_retries: 3,
            multi_errors,
        }
    }

    /// @param mixed[]  $options
    /// @param non-empty-string $url
    ///
    /// @return int internal job id
    pub fn download(
        &mut self,
        resolve: Box<dyn Fn(PhpMixed) + Send + Sync>,
        reject: Box<dyn Fn(PhpMixed) + Send + Sync>,
        origin: &str,
        url: &str,
        mut options: IndexMap<String, PhpMixed>,
        copy_to: Option<&str>,
    ) -> anyhow::Result<Result<i64, TransportException>> {
        let mut attributes: IndexMap<String, PhpMixed> = IndexMap::new();
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

        self.init_download(resolve, reject, origin, url, options, copy_to, attributes)
    }

    /// @param mixed[]  $options
    ///
    /// @param array{retryAuthFailure?: bool, redirects?: int<0, max>, retries?: int<0, max>, storeAuth?: 'prompt'|bool, ipResolve?: 4|6|null} $attributes
    /// @param non-empty-string $url
    ///
    /// @return int internal job id
    fn init_download(
        &mut self,
        resolve: Box<dyn Fn(PhpMixed) + Send + Sync>,
        reject: Box<dyn Fn(PhpMixed) + Send + Sync>,
        origin: &str,
        url: &str,
        mut options: IndexMap<String, PhpMixed>,
        copy_to: Option<&str>,
        attributes: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<Result<i64, TransportException>> {
        let defaults: IndexMap<String, PhpMixed> = {
            let mut m = IndexMap::new();
            m.insert("retryAuthFailure".to_string(), PhpMixed::Bool(true));
            m.insert("redirects".to_string(), PhpMixed::Int(0));
            m.insert("retries".to_string(), PhpMixed::Int(0));
            m.insert("storeAuth".to_string(), PhpMixed::Bool(false));
            m.insert("ipResolve".to_string(), PhpMixed::Null);
            m
        };
        let merged = array_merge(
            PhpMixed::Array(
                defaults
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
            PhpMixed::Array(
                attributes
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        );
        let mut attributes: IndexMap<String, PhpMixed> = match merged {
            PhpMixed::Array(a) => a.into_iter().map(|(k, v)| (k, *v)).collect(),
            _ => IndexMap::new(),
        };

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

        let original_options = options.clone();

        // check URL can be accessed (i.e. is not insecure), but allow insecure Packagist calls to $hashed providers as file integrity is verified with sha256
        if !Preg::is_match(r"{^http://(repo\.)?packagist\.org/p/}", url)
            || (strpos(url, "$").is_none() && strpos(url, "%24").is_none())
        {
            self.config
                .prohibit_url_by_config(url, Some(&*self.io), &options)?;
        }

        let curl_handle = curl_init();
        let header_handle = fopen("php://temp/maxmemory:32768", "w+b");
        if matches!(header_handle, PhpMixed::Bool(false)) {
            anyhow::bail!(
                RuntimeException {
                    message: "Failed to open a temp stream to store curl headers".to_string(),
                    code: 0,
                }
                .message
            );
        }

        let body_target: String = if let Some(copy_to) = copy_to {
            format!("{}~", copy_to)
        } else {
            "php://temp/maxmemory:524288".to_string()
        };

        let error_message: std::rc::Rc<std::cell::RefCell<String>> =
            std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        {
            let error_message = error_message.clone();
            set_error_handler_closure(Box::new(move |_code: i64, msg: &str| -> bool {
                let mut em = error_message.borrow_mut();
                if !em.is_empty() {
                    em.push_str("\n");
                }
                em.push_str(&Preg::replace(r"{^fopen\(.*?\): }", "", msg));
                true
            }));
        }
        let body_handle = fopen(&body_target, "w+b");
        restore_error_handler();
        if matches!(body_handle, PhpMixed::Bool(false)) {
            return Ok(Err(TransportException::new(
                format!(
                    "The \"{}\" file could not be written to {}: {}",
                    url,
                    copy_to.unwrap_or("a temporary file"),
                    error_message.borrow()
                ),
                0,
            )));
        }

        curl_setopt(&curl_handle, CURLOPT_URL, PhpMixed::String(url.to_string()));
        curl_setopt(&curl_handle, CURLOPT_FOLLOWLOCATION, PhpMixed::Bool(false));
        curl_setopt(&curl_handle, CURLOPT_CONNECTTIMEOUT, PhpMixed::Int(10));
        curl_setopt(
            &curl_handle,
            CURLOPT_TIMEOUT,
            PhpMixed::Int(max(
                ini_get("default_socket_timeout")
                    .as_deref()
                    .unwrap_or("0")
                    .parse::<i64>()
                    .unwrap_or(0),
                300,
            )),
        );
        curl_setopt(&curl_handle, CURLOPT_WRITEHEADER, header_handle.clone());
        curl_setopt(&curl_handle, CURLOPT_FILE, body_handle.clone());
        curl_setopt(&curl_handle, CURLOPT_ENCODING, PhpMixed::String(String::new())); // let cURL set the Accept-Encoding header to what it supports
        curl_setopt(
            &curl_handle,
            CURLOPT_PROTOCOLS,
            PhpMixed::Int(CURLPROTO_HTTP | CURLPROTO_HTTPS),
        );

        if attributes.get("ipResolve").and_then(|v| v.as_int()) == Some(4) {
            curl_setopt(&curl_handle, CURLOPT_IPRESOLVE, PhpMixed::Int(CURL_IPRESOLVE_V4));
        } else if attributes.get("ipResolve").and_then(|v| v.as_int()) == Some(6) {
            curl_setopt(&curl_handle, CURLOPT_IPRESOLVE, PhpMixed::Int(CURL_IPRESOLVE_V6));
        }

        if function_exists("curl_share_init") {
            // share_handle is set when curl_share_init exists
            if let Some(sh) = &self.share_handle {
                curl_setopt(&curl_handle, CURLOPT_SHARE, PhpMixed::Null);
                let _ = sh;
            }
        }

        if !options
            .get("http")
            .and_then(|v| v.as_array())
            .map(|a| a.contains_key("header"))
            .unwrap_or(false)
        {
            let http = options
                .entry("http".to_string())
                .or_insert(PhpMixed::Array(IndexMap::new()));
            if let PhpMixed::Array(a) = http {
                a.insert(
                    "header".to_string(),
                    Box::new(PhpMixed::List(Vec::new())),
                );
            }
        }

        // $options['http']['header'] = array_diff($options['http']['header'], ['Connection: close']);
        // $options['http']['header'][] = 'Connection: keep-alive';
        if let Some(PhpMixed::Array(http)) = options.get_mut("http") {
            if let Some(boxed) = http.get_mut("header") {
                if let PhpMixed::List(list) = boxed.as_mut() {
                    let headers: Vec<String> = list
                        .iter()
                        .filter_map(|b| match b.as_ref() {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect();
                    let diffed = array_diff(&headers, &["Connection: close".to_string()]);
                    let mut new_list: Vec<Box<PhpMixed>> = diffed
                        .into_iter()
                        .map(|s| Box::new(PhpMixed::String(s)))
                        .collect();
                    new_list.push(Box::new(PhpMixed::String("Connection: keep-alive".to_string())));
                    *list = new_list;
                }
            }
        }

        let version = curl_version();
        let features = version
            .as_ref()
            .and_then(|v| v.get("features"))
            .and_then(|b| b.as_int())
            .unwrap_or(0);

        let proxy = ProxyManager::get_instance()
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .get_proxy_for_request(url)
            .map_err(|e| anyhow::anyhow!(e.message))?;

        if strpos(url, "https://") == Some(0) {
            let will_use_proxy = proxy
                .get_status(None)
                .map(|s| !s.is_empty())
                .unwrap_or(false)
                && !proxy.is_excluded_by_no_proxy();

            if !will_use_proxy
                && defined("CURL_VERSION_HTTP3")
                && defined("CURL_HTTP_VERSION_3")
                && (CURL_VERSION_HTTP3 & features) != 0
            {
                curl_setopt(
                    &curl_handle,
                    CURLOPT_HTTP_VERSION,
                    PhpMixed::Int(CURL_HTTP_VERSION_3),
                );
            } else if defined("CURL_VERSION_HTTP2")
                && defined("CURL_HTTP_VERSION_2_0")
                && (CURL_VERSION_HTTP2 & features) != 0
            {
                curl_setopt(
                    &curl_handle,
                    CURLOPT_HTTP_VERSION,
                    PhpMixed::Int(CURL_HTTP_VERSION_2_0),
                );
            }
        }

        // curl 8.7.0 - 8.7.1 has a bug whereas automatic accept-encoding header results in an error when reading the response
        // https://github.com/composer/composer/issues/11913
        if version
            .as_ref()
            .map(|v| v.contains_key("version"))
            .unwrap_or(false)
            && in_array(
                version
                    .as_ref()
                    .and_then(|v| v.get("version"))
                    .map(|b| (**b).clone())
                    .unwrap_or(PhpMixed::Null),
                &PhpMixed::List(vec![
                    Box::new(PhpMixed::String("8.7.0".to_string())),
                    Box::new(PhpMixed::String("8.7.1".to_string())),
                ]),
                true,
            )
            && defined("CURL_VERSION_LIBZ")
            && (CURL_VERSION_LIBZ & features) != 0
        {
            curl_setopt(
                &curl_handle,
                CURLOPT_ENCODING,
                PhpMixed::String("gzip".to_string()),
            );
        }

        let options = self
            .auth_helper
            .add_authentication_options(options, origin, url);
        let options = StreamContextFactory::init_options(url, options, true)
            .map_err(|e| anyhow::anyhow!(e.message))?;

        for (r#type, curl_options) in options_static() {
            for (name, curl_option) in &curl_options {
                if options
                    .get(&r#type)
                    .and_then(|v| v.as_array())
                    .map(|a| a.contains_key(name))
                    .unwrap_or(false)
                {
                    if r#type == "ssl" && name == "verify_peer_name" {
                        let val = options
                            .get(&r#type)
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.get(name))
                            .map(|b| (**b).clone())
                            .unwrap_or(PhpMixed::Null);
                        let to_set = if matches!(val, PhpMixed::Bool(true)) {
                            PhpMixed::Int(2)
                        } else {
                            val
                        };
                        curl_setopt(&curl_handle, *curl_option, to_set);
                    } else {
                        let val = options
                            .get(&r#type)
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.get(name))
                            .map(|b| (**b).clone())
                            .unwrap_or(PhpMixed::Null);
                        curl_setopt(&curl_handle, *curl_option, val);
                    }
                }
            }
        }

        let ssl_options: IndexMap<String, PhpMixed> = options
            .get("ssl")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect())
            .unwrap_or_else(IndexMap::new);
        let proxy_curl_options = proxy
            .get_curl_options(&ssl_options)
            .map_err(|e| anyhow::anyhow!(e.message))?;
        curl_setopt_array(&curl_handle, &proxy_curl_options.into_iter().collect());

        let progress = array_diff_key(
            &match curl_getinfo(&curl_handle) {
                PhpMixed::Array(a) => a,
                _ => IndexMap::new(),
            },
            &time_info_static()
                .into_iter()
                .map(|(k, v)| (k, Box::new(PhpMixed::Bool(v))))
                .collect(),
        );

        let mut job: IndexMap<String, PhpMixed> = IndexMap::new();
        job.insert("url".to_string(), PhpMixed::String(url.to_string()));
        job.insert("origin".to_string(), PhpMixed::String(origin.to_string()));
        job.insert(
            "attributes".to_string(),
            PhpMixed::Array(
                attributes
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        );
        job.insert(
            "options".to_string(),
            PhpMixed::Array(
                original_options
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        );
        job.insert(
            "progress".to_string(),
            PhpMixed::Array(progress.clone()),
        );
        // curlHandle, headerHandle, bodyHandle, resolve, reject are PHP resources/callables;
        // stored as opaque PhpMixed::Null placeholders (real values live in Rust-side fields).
        // TODO(phase-b): wire handle/closure storage properly.
        job.insert("curlHandle".to_string(), PhpMixed::Null);
        job.insert(
            "filename".to_string(),
            copy_to
                .map(|s| PhpMixed::String(s.to_string()))
                .unwrap_or(PhpMixed::Null),
        );
        job.insert("headerHandle".to_string(), header_handle.clone());
        job.insert("bodyHandle".to_string(), body_handle.clone());
        job.insert("resolve".to_string(), PhpMixed::Null);
        job.insert("reject".to_string(), PhpMixed::Null);
        job.insert("primaryIp".to_string(), PhpMixed::String(String::new()));

        let _ = (resolve, reject); // TODO(phase-b): store callables in Job

        self.jobs.insert(curl_handle_id(&curl_handle), job);

        let using_proxy = proxy
            .get_status(Some(" using proxy (%s)"))
            .unwrap_or_default();
        let header_strings: Vec<String> = options
            .get("http")
            .and_then(|v| v.as_array())
            .and_then(|a| a.get("header"))
            .and_then(|b| match b.as_ref() {
                PhpMixed::List(l) => Some(
                    l.iter()
                        .filter_map(|x| match x.as_ref() {
                            PhpMixed::String(s) => Some(s.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let if_modified =
            if stripos(&implode(",", &header_strings), "if-modified-since:").is_some() {
                " if modified"
            } else {
                ""
            };
        if attributes.get("redirects").and_then(|v| v.as_int()) == Some(0)
            && attributes.get("retries").and_then(|v| v.as_int()) == Some(0)
        {
            self.io.write_error(
                PhpMixed::String(format!(
                    "Downloading {}{}{}",
                    Url::sanitize(url.to_string()),
                    using_proxy,
                    if_modified
                )),
                true,
                <dyn IOInterface>::DEBUG,
            );
        }

        self.check_curl_result(curl_multi_add_handle(&self.multi_handle, &curl_handle))?;
        // TODO progress

        Ok(Ok(curl_handle_id(&curl_handle)))
    }

    pub fn abort_request(&mut self, id: i64) {
        if self.jobs.contains_key(&id)
            && self
                .jobs
                .get(&id)
                .map(|j| j.contains_key("curlHandle"))
                .unwrap_or(false)
        {
            let job = self.jobs.get(&id).cloned().unwrap_or_default();
            // job['curlHandle'] is the actual \CurlHandle in PHP; in this port we keep
            // handles in Rust-owned storage. TODO(phase-b): wire actual handle removal.
            // curl_multi_remove_handle($this->multiHandle, $job['curlHandle']);
            if PHP_VERSION_ID < 80000 {
                // curl_close($job['curlHandle']);
            }
            if is_resource(job.get("headerHandle").unwrap_or(&PhpMixed::Null)) {
                fclose(job.get("headerHandle").cloned().unwrap_or(PhpMixed::Null));
            }
            if is_resource(job.get("bodyHandle").unwrap_or(&PhpMixed::Null)) {
                fclose(job.get("bodyHandle").cloned().unwrap_or(PhpMixed::Null));
            }
            if let Some(PhpMixed::String(filename)) = job.get("filename") {
                unlink_silent(&format!("{}~", filename));
            }
            self.jobs.shift_remove(&id);
        }
    }

    pub fn tick(&mut self) -> anyhow::Result<()> {
        if count(&PhpMixed::Array(
            self.jobs
                .iter()
                .map(|(k, v)| {
                    (
                        k.to_string(),
                        Box::new(PhpMixed::Array(
                            v.iter()
                                .map(|(k2, v2)| (k2.clone(), Box::new(v2.clone())))
                                .collect(),
                        )),
                    )
                })
                .collect(),
        )) == 0
        {
            return Ok(());
        }

        let mut active = true;
        self.check_curl_result(curl_multi_exec(&self.multi_handle, &mut active))?;
        if -1 == curl_multi_select(&self.multi_handle, self.select_timeout) {
            // sleep in case select returns -1 as it can happen on old php versions or some platforms where curl does not manage to do the select
            usleep(150);
        }

        loop {
            let progress_read = curl_multi_info_read(&self.multi_handle);
            let mut progress: IndexMap<String, Box<PhpMixed>> = match &progress_read {
                PhpMixed::Array(a) => a.clone(),
                _ => break,
            };
            // $curlHandle = $progress['handle']; $result = $progress['result']; $i = (int) $curlHandle;
            let _curl_handle_placeholder: PhpMixed = progress
                .get("handle")
                .map(|b| (**b).clone())
                .unwrap_or(PhpMixed::Null);
            let result_code: i64 = progress
                .get("result")
                .and_then(|b| b.as_int())
                .unwrap_or(0);
            // TODO(phase-b): correlate handle in `progress['handle']` to its job id.
            let i: i64 = 0;
            if !self.jobs.contains_key(&i) {
                continue;
            }

            // $progress = curl_getinfo($curlHandle);
            // if (false === $progress) throw new RuntimeException(...)
            let info = curl_getinfo(/* TODO real handle */ &curl_init());
            match info {
                PhpMixed::Array(a) => progress = a,
                PhpMixed::Bool(false) => {
                    anyhow::bail!(
                        RuntimeException {
                            message: format!(
                                "Failed getting info from curl handle {} ({})",
                                i,
                                self.jobs
                                    .get(&i)
                                    .and_then(|j| j.get("url"))
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                            ),
                            code: 0,
                        }
                        .message
                    );
                }
                _ => {}
            }
            let job = self.jobs.get(&i).cloned().unwrap_or_default();
            self.jobs.shift_remove(&i);
            let mut error = curl_error(/* TODO real handle */ &curl_init());
            let mut errno = curl_errno(/* TODO real handle */ &curl_init());
            // curl_multi_remove_handle($this->multiHandle, $curlHandle);
            if PHP_VERSION_ID < 80000 {
                // curl_close($curlHandle);
            }

            let mut headers: Option<Vec<String>> = None;
            let mut status_code: Option<i64> = None;
            let mut response: Option<CurlResponse> = None;
            // PHP try block; recoverable errors (TransportException, LogicException) flow through
            // the inner Result, fatal errors propagate via anyhow::Result.
            let try_block: anyhow::Result<Result<(), TransportException>> = (|| {
                // TODO progress
                if CURLE_OK != errno || !error.is_empty() || result_code != CURLE_OK {
                    if errno == 0 {
                        errno = result_code;
                    }
                    if error.is_empty() && function_exists("curl_strerror") {
                        error = curl_strerror(errno).unwrap_or_default();
                    }
                    progress.insert("error_code".to_string(), Box::new(PhpMixed::Int(errno)));

                    if errno == 28 /* CURLE_OPERATION_TIMEDOUT */
                        && PHP_VERSION_ID >= 70300
                        && progress
                            .get("namelookup_time")
                            .and_then(|b| b.as_float())
                            == Some(0.0)
                        && !TIMEOUT_WARNING.load(Ordering::Relaxed)
                    {
                        TIMEOUT_WARNING.store(true, Ordering::Relaxed);
                        self.io.write_error(
                            PhpMixed::String(
                                "<warning>A connection timeout was encountered. If you intend to run Composer without connecting to the internet, run the command again prefixed with COMPOSER_DISABLE_NETWORK=1 to make Composer run in offline mode.</warning>"
                                    .to_string(),
                            ),
                            true,
                            <dyn IOInterface>::NORMAL,
                        );
                    }

                    let method_is_get = !job
                        .get("options")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("http"))
                        .and_then(|b| b.as_array())
                        .map(|a| a.contains_key("method"))
                        .unwrap_or(false)
                        || job
                            .get("options")
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.get("http"))
                            .and_then(|b| b.as_array())
                            .and_then(|a| a.get("method"))
                            .and_then(|b| b.as_string().map(|s| s.to_string()))
                            == Some("GET".to_string());
                    if method_is_get
                        && (in_array(
                            PhpMixed::Int(errno),
                            &PhpMixed::List(vec![
                                Box::new(PhpMixed::Int(7 /* CURLE_COULDNT_CONNECT */)),
                                Box::new(PhpMixed::Int(16 /* CURLE_HTTP2 */)),
                                Box::new(PhpMixed::Int(92 /* CURLE_HTTP2_STREAM */)),
                                Box::new(PhpMixed::Int(6 /* CURLE_COULDNT_RESOLVE_HOST */)),
                                Box::new(PhpMixed::Int(28 /* CURLE_OPERATION_TIMEDOUT */)),
                            ]),
                            true,
                        ) || (in_array(
                            PhpMixed::Int(errno),
                            &PhpMixed::List(vec![
                                Box::new(PhpMixed::Int(56 /* CURLE_RECV_ERROR */)),
                                Box::new(PhpMixed::Int(35 /* CURLE_SSL_CONNECT_ERROR */)),
                            ]),
                            true,
                        ) && str_contains(&error, "Connection reset by peer")))
                        && job
                            .get("attributes")
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.get("retries"))
                            .and_then(|b| b.as_int())
                            .unwrap_or(0)
                            < self.max_retries
                    {
                        let mut attributes: IndexMap<String, PhpMixed> = IndexMap::new();
                        attributes.insert(
                            "retries".to_string(),
                            PhpMixed::Int(
                                job.get("attributes")
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.get("retries"))
                                    .and_then(|b| b.as_int())
                                    .unwrap_or(0)
                                    + 1,
                            ),
                        );
                        if errno == 7
                            && !job
                                .get("attributes")
                                .and_then(|v| v.as_array())
                                .map(|a| a.contains_key("ipResolve"))
                                .unwrap_or(false)
                        {
                            // CURLE_COULDNT_CONNECT, retry forcing IPv4 if no IP stack was selected
                            attributes.insert("ipResolve".to_string(), PhpMixed::Int(4));
                        }
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "Retrying ({}) {} due to curl error {}",
                                job.get("attributes")
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.get("retries"))
                                    .and_then(|b| b.as_int())
                                    .unwrap_or(0)
                                    + 1,
                                Url::sanitize(
                                    job.get("url")
                                        .and_then(|v| v.as_string())
                                        .unwrap_or("")
                                        .to_string()
                                ),
                                errno
                            )),
                            true,
                            <dyn IOInterface>::DEBUG,
                        );
                        self.restart_job_with_delay(
                            &job,
                            job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                            attributes,
                        )?;
                        return Ok(Ok(()));
                    }

                    // TODO: Remove this as soon as https://github.com/curl/curl/issues/10591 is resolved
                    if errno == 55 /* CURLE_SEND_ERROR */ {
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "Retrying ({}) {} due to curl error {}",
                                job.get("attributes")
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.get("retries"))
                                    .and_then(|b| b.as_int())
                                    .unwrap_or(0)
                                    + 1,
                                Url::sanitize(
                                    job.get("url")
                                        .and_then(|v| v.as_string())
                                        .unwrap_or("")
                                        .to_string()
                                ),
                                errno
                            )),
                            true,
                            <dyn IOInterface>::DEBUG,
                        );
                        let mut attrs: IndexMap<String, PhpMixed> = IndexMap::new();
                        attrs.insert(
                            "retries".to_string(),
                            PhpMixed::Int(
                                job.get("attributes")
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.get("retries"))
                                    .and_then(|b| b.as_int())
                                    .unwrap_or(0)
                                    + 1,
                            ),
                        );
                        self.restart_job_with_delay(
                            &job,
                            job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                            attrs,
                        )?;
                        return Ok(Ok(()));
                    }

                    return Ok(Err(TransportException::new(
                        format!(
                            "curl error {} while downloading {}: {}",
                            errno,
                            Url::sanitize(
                                progress
                                    .get("url")
                                    .and_then(|b| b.as_string())
                                    .unwrap_or("")
                                    .to_string()
                            ),
                            error
                        ),
                        0,
                    )));
                }
                status_code = progress.get("http_code").and_then(|b| b.as_int());
                rewind(
                    job.get("headerHandle")
                        .cloned()
                        .unwrap_or(PhpMixed::Null),
                );
                headers = Some(explode(
                    "\r\n",
                    &rtrim(
                        &stream_get_contents(
                            job.get("headerHandle")
                                .cloned()
                                .unwrap_or(PhpMixed::Null),
                        )
                        .unwrap_or_default(),
                        None,
                    ),
                ));
                fclose(
                    job.get("headerHandle")
                        .cloned()
                        .unwrap_or(PhpMixed::Null),
                );

                if status_code == Some(0) {
                    anyhow::bail!(
                        LogicException {
                            message: format!(
                                "Received unexpected http status code 0 without error for {}: headers {} curl info {}",
                                Url::sanitize(
                                    progress
                                        .get("url")
                                        .and_then(|b| b.as_string())
                                        .unwrap_or("")
                                        .to_string()
                                ),
                                var_export(
                                    &PhpMixed::List(
                                        headers
                                            .as_ref()
                                            .unwrap()
                                            .iter()
                                            .map(|s| Box::new(PhpMixed::String(s.clone())))
                                            .collect()
                                    ),
                                    true
                                ),
                                var_export(&PhpMixed::Array(progress.clone()), true)
                            ),
                            code: 0,
                        }
                        .message
                    );
                }

                // prepare response object
                let contents: PhpMixed;
                if let Some(PhpMixed::String(filename)) = job.get("filename") {
                    let mut c: PhpMixed = PhpMixed::String(format!("{}~", filename));
                    if status_code.unwrap_or(0) >= 300 {
                        rewind(
                            job.get("bodyHandle")
                                .cloned()
                                .unwrap_or(PhpMixed::Null),
                        );
                        c = PhpMixed::String(
                            stream_get_contents(
                                job.get("bodyHandle")
                                    .cloned()
                                    .unwrap_or(PhpMixed::Null),
                            )
                            .unwrap_or_default(),
                        );
                    }
                    contents = c;
                    response = Some(CurlResponse::new(
                        {
                            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                            m.insert(
                                "url".to_string(),
                                PhpMixed::String(
                                    job.get("url")
                                        .and_then(|v| v.as_string())
                                        .unwrap_or("")
                                        .to_string(),
                                ),
                            );
                            m
                        },
                        status_code,
                        headers.clone().unwrap_or_default(),
                        contents.as_string().map(|s| s.to_string()),
                        progress.clone(),
                    ));
                    self.io.write_error(
                        PhpMixed::String(format!(
                            "[{}] {}",
                            status_code.unwrap_or(0),
                            Url::sanitize(
                                job.get("url")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                                    .to_string()
                            )
                        )),
                        true,
                        <dyn IOInterface>::DEBUG,
                    );
                } else {
                    let max_file_size: Option<i64> = job
                        .get("options")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("max_file_size"))
                        .and_then(|b| b.as_int());
                    rewind(
                        job.get("bodyHandle")
                            .cloned()
                            .unwrap_or(PhpMixed::Null),
                    );
                    if let Some(max_file_size) = max_file_size {
                        let c = stream_get_contents_with_max(
                            job.get("bodyHandle")
                                .cloned()
                                .unwrap_or(PhpMixed::Null),
                            Some(max_file_size),
                        );
                        // Gzipped responses with missing Content-Length header cannot be detected during the file download
                        // because $progress['size_download'] refers to the gzipped size downloaded, not the actual file size
                        if let Some(c_str) = c.as_deref() {
                            if Platform::strlen(c_str) >= max_file_size {
                                anyhow::bail!(
                                    MaxFileSizeExceededException(TransportException::new(
                                        format!(
                                            "Maximum allowed download size reached. Downloaded {} of allowed {} bytes",
                                            Platform::strlen(c_str),
                                            max_file_size
                                        ),
                                        0,
                                    ))
                                    .0
                                    .message
                                );
                            }
                        }
                        contents = PhpMixed::String(c.unwrap_or_default());
                    } else {
                        contents = PhpMixed::String(
                            stream_get_contents(
                                job.get("bodyHandle")
                                    .cloned()
                                    .unwrap_or(PhpMixed::Null),
                            )
                            .unwrap_or_default(),
                        );
                    }

                    response = Some(CurlResponse::new(
                        {
                            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                            m.insert(
                                "url".to_string(),
                                PhpMixed::String(
                                    job.get("url")
                                        .and_then(|v| v.as_string())
                                        .unwrap_or("")
                                        .to_string(),
                                ),
                            );
                            m
                        },
                        status_code,
                        headers.clone().unwrap_or_default(),
                        contents.as_string().map(|s| s.to_string()),
                        progress.clone(),
                    ));
                    self.io.write_error(
                        PhpMixed::String(format!(
                            "[{}] {}",
                            status_code.unwrap_or(0),
                            Url::sanitize(
                                job.get("url")
                                    .and_then(|v| v.as_string())
                                    .unwrap_or("")
                                    .to_string()
                            )
                        )),
                        true,
                        <dyn IOInterface>::DEBUG,
                    );
                }
                fclose(
                    job.get("bodyHandle")
                        .cloned()
                        .unwrap_or(PhpMixed::Null),
                );

                let response_ref = response.as_ref().unwrap();
                if response_ref.inner.get_status_code() >= 300
                    && response_ref.inner.get_header("content-type").as_deref()
                        == Some("application/json")
                {
                    HttpDownloader::output_warnings(
                        &*self.io,
                        job.get("origin").and_then(|v| v.as_string()).unwrap_or(""),
                        &match json_decode(
                            response_ref.inner.get_body().unwrap_or(""),
                            true,
                        )? {
                            PhpMixed::Array(a) => a.into_iter().map(|(k, v)| (k, *v)).collect(),
                            _ => IndexMap::new(),
                        },
                    )?;
                }

                let result =
                    self.is_authenticated_retry_needed(&job, response.as_ref().unwrap())?;
                let retry = match &result {
                    Ok(r) => r.retry,
                    Err(_) => false,
                };
                if retry {
                    let r = result.unwrap();
                    let mut attrs: IndexMap<String, PhpMixed> = IndexMap::new();
                    attrs.insert(
                        "storeAuth".to_string(),
                        match r.store_auth {
                            StoreAuth::Bool(b) => PhpMixed::Bool(b),
                            StoreAuth::Prompt => PhpMixed::String("prompt".to_string()),
                        },
                    );
                    attrs.insert(
                        "retries".to_string(),
                        PhpMixed::Int(
                            job.get("attributes")
                                .and_then(|v| v.as_array())
                                .and_then(|a| a.get("retries"))
                                .and_then(|b| b.as_int())
                                .unwrap_or(0)
                                + 1,
                        ),
                    );
                    self.restart_job(
                        &job,
                        job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                        attrs,
                    )?;
                    return Ok(Ok(()));
                }

                // handle 3xx redirects, 304 Not Modified is excluded
                let sc = status_code.unwrap_or(0);
                if sc >= 300
                    && sc <= 399
                    && sc != 304
                    && job
                        .get("attributes")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("redirects"))
                        .and_then(|b| b.as_int())
                        .unwrap_or(0)
                        < self.max_redirects
                {
                    let location = self.handle_redirect(&job, response.as_ref().unwrap())?;
                    match location {
                        Ok(loc) if !loc.is_empty() => {
                            let mut attrs: IndexMap<String, PhpMixed> = IndexMap::new();
                            attrs.insert(
                                "redirects".to_string(),
                                PhpMixed::Int(
                                    job.get("attributes")
                                        .and_then(|v| v.as_array())
                                        .and_then(|a| a.get("redirects"))
                                        .and_then(|b| b.as_int())
                                        .unwrap_or(0)
                                        + 1,
                                ),
                            );
                            self.restart_job(&job, &loc, attrs)?;
                            return Ok(Ok(()));
                        }
                        Ok(_) => {}
                        Err(e) => return Ok(Err(e)),
                    }
                }

                // fail 4xx and 5xx responses and capture the response
                if sc >= 400 && sc <= 599 {
                    let method_is_get = !job
                        .get("options")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("http"))
                        .and_then(|b| b.as_array())
                        .map(|a| a.contains_key("method"))
                        .unwrap_or(false)
                        || job
                            .get("options")
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.get("http"))
                            .and_then(|b| b.as_array())
                            .and_then(|a| a.get("method"))
                            .and_then(|b| b.as_string().map(|s| s.to_string()))
                            == Some("GET".to_string());
                    if method_is_get
                        && in_array(
                            PhpMixed::Int(sc),
                            &PhpMixed::List(vec![
                                Box::new(PhpMixed::Int(423)),
                                Box::new(PhpMixed::Int(425)),
                                Box::new(PhpMixed::Int(500)),
                                Box::new(PhpMixed::Int(502)),
                                Box::new(PhpMixed::Int(503)),
                                Box::new(PhpMixed::Int(504)),
                                Box::new(PhpMixed::Int(507)),
                                Box::new(PhpMixed::Int(510)),
                            ]),
                            true,
                        )
                        && job
                            .get("attributes")
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.get("retries"))
                            .and_then(|b| b.as_int())
                            .unwrap_or(0)
                            < self.max_retries
                    {
                        self.io.write_error(
                            PhpMixed::String(format!(
                                "Retrying ({}) {} due to status code {}",
                                job.get("attributes")
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.get("retries"))
                                    .and_then(|b| b.as_int())
                                    .unwrap_or(0)
                                    + 1,
                                Url::sanitize(
                                    job.get("url")
                                        .and_then(|v| v.as_string())
                                        .unwrap_or("")
                                        .to_string()
                                ),
                                sc
                            )),
                            true,
                            <dyn IOInterface>::DEBUG,
                        );
                        let mut attrs: IndexMap<String, PhpMixed> = IndexMap::new();
                        attrs.insert(
                            "retries".to_string(),
                            PhpMixed::Int(
                                job.get("attributes")
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.get("retries"))
                                    .and_then(|b| b.as_int())
                                    .unwrap_or(0)
                                    + 1,
                            ),
                        );
                        self.restart_job_with_delay(
                            &job,
                            job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                            attrs,
                        )?;
                        return Ok(Ok(()));
                    }

                    let status_msg = response_ref
                        .inner
                        .get_status_message()
                        .unwrap_or_default();
                    return Ok(Err(self.fail_response(
                        &job,
                        response.as_ref().unwrap(),
                        &status_msg,
                    )));
                }

                if !matches!(
                    job.get("attributes")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("storeAuth"))
                        .map(|b| (**b).clone()),
                    Some(PhpMixed::Bool(false))
                ) {
                    let store_auth_val = job
                        .get("attributes")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("storeAuth"))
                        .map(|b| (**b).clone())
                        .unwrap_or(PhpMixed::Bool(false));
                    let store_auth = match store_auth_val {
                        PhpMixed::Bool(b) => StoreAuth::Bool(b),
                        PhpMixed::String(ref s) if s == "prompt" => StoreAuth::Prompt,
                        _ => StoreAuth::Bool(false),
                    };
                    self.auth_helper.store_auth(
                        job.get("origin").and_then(|v| v.as_string()).unwrap_or(""),
                        store_auth,
                    )?;
                }

                // resolve promise
                if let Some(PhpMixed::String(filename)) = job.get("filename") {
                    rename(&format!("{}~", filename), filename);
                    // job['resolve']($response);
                    // TODO(phase-b): invoke stored resolve callable
                } else {
                    // job['resolve']($response);
                    // TODO(phase-b): invoke stored resolve callable
                }
                Ok(Ok(()))
            })();
            match try_block {
                Ok(Ok(())) => {}
                Ok(Err(mut e)) => {
                    // PHP catches \Exception; the recoverable branch here is TransportException.
                    if let Some(h) = &headers {
                        e.set_headers(h.clone());
                        e.set_status_code(status_code);
                    }
                    if let Some(r) = &response {
                        e.set_response(r.inner.get_body().map(|s| s.to_string()));
                    }
                    e.set_response_info(
                        progress
                            .iter()
                            .map(|(_, v)| (**v).clone())
                            .collect::<Vec<_>>(),
                    );
                    self.reject_job(&job, anyhow::anyhow!(e.message));
                }
                Err(e) => {
                    // Non-TransportException fatal error: pass through reject_job to mirror PHP catch (\Exception $e).
                    self.reject_job(&job, e);
                }
            }
        }

        let keys: Vec<i64> = self.jobs.keys().cloned().collect();
        for i in keys {
            // $curlHandle = $this->jobs[$i]['curlHandle'];
            // $progress = array_diff_key(curl_getinfo($curlHandle), self::$timeInfo);
            let progress_now = array_diff_key(
                &match curl_getinfo(/* TODO real handle */ &curl_init()) {
                    PhpMixed::Array(a) => a,
                    _ => IndexMap::new(),
                },
                &time_info_static()
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(PhpMixed::Bool(v))))
                    .collect(),
            );

            let prev_progress = self
                .jobs
                .get(&i)
                .and_then(|j| j.get("progress"))
                .cloned()
                .unwrap_or(PhpMixed::Null);
            let prev_progress_map = match &prev_progress {
                PhpMixed::Array(a) => a.clone(),
                _ => IndexMap::new(),
            };

            if !maps_equal(&prev_progress_map, &progress_now) {
                if let Some(job) = self.jobs.get_mut(&i) {
                    job.insert(
                        "progress".to_string(),
                        PhpMixed::Array(progress_now.clone()),
                    );
                }

                let max_file_size = self
                    .jobs
                    .get(&i)
                    .and_then(|j| j.get("options"))
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get("max_file_size"))
                    .and_then(|b| b.as_int());
                if let Some(max_file_size) = max_file_size {
                    // Compare max_file_size with the content-length header this value will be -1 until the header is parsed
                    let download_content_length = progress_now
                        .get("download_content_length")
                        .and_then(|b| b.as_int())
                        .unwrap_or(0);
                    if max_file_size < download_content_length {
                        let job = self.jobs.get(&i).cloned().unwrap_or_default();
                        self.reject_job(
                            &job,
                            anyhow::anyhow!(
                                MaxFileSizeExceededException(TransportException::new(
                                    format!(
                                        "Maximum allowed download size reached. Content-length header indicates {} bytes. Allowed {} bytes",
                                        download_content_length, max_file_size
                                    ),
                                    0,
                                ))
                                .0
                                .message
                            ),
                        );
                    }

                    // Compare max_file_size with the download size in bytes
                    let size_download = progress_now
                        .get("size_download")
                        .and_then(|b| b.as_int())
                        .unwrap_or(0);
                    if max_file_size < size_download {
                        let job = self.jobs.get(&i).cloned().unwrap_or_default();
                        self.reject_job(
                            &job,
                            anyhow::anyhow!(
                                MaxFileSizeExceededException(TransportException::new(
                                    format!(
                                        "Maximum allowed download size reached. Downloaded {} of allowed {} bytes",
                                        size_download, max_file_size
                                    ),
                                    0,
                                ))
                                .0
                                .message
                            ),
                        );
                    }
                }

                let primary_ip = progress_now.get("primary_ip");
                let prev_primary_ip = self
                    .jobs
                    .get(&i)
                    .and_then(|j| j.get("primaryIp"))
                    .and_then(|v| v.as_string())
                    .unwrap_or("")
                    .to_string();
                if let Some(primary_ip) = primary_ip {
                    if primary_ip.as_string() != Some(&prev_primary_ip) {
                        let prevent_ip_access_callable = self
                            .jobs
                            .get(&i)
                            .and_then(|j| j.get("options"))
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.get("prevent_ip_access_callable"))
                            .is_some();
                        // TODO(phase-b): invoke prevent_ip_access_callable
                        let blocked = prevent_ip_access_callable && false;
                        if blocked {
                            let job = self.jobs.get(&i).cloned().unwrap_or_default();
                            self.reject_job(
                                &job,
                                anyhow::anyhow!(TransportException::new(
                                    sprintf(
                                        "IP \"%s\" is blocked for \"%s\".",
                                        &[
                                            (**primary_ip).clone(),
                                            progress_now
                                                .get("url")
                                                .map(|b| (**b).clone())
                                                .unwrap_or(PhpMixed::Null),
                                        ],
                                    ),
                                    0,
                                )
                                .message),
                            );
                        }

                        if let Some(job) = self.jobs.get_mut(&i) {
                            job.insert(
                                "primaryIp".to_string(),
                                PhpMixed::String(
                                    primary_ip.as_string().unwrap_or("").to_string(),
                                ),
                            );
                        }
                    }
                }

                // TODO progress
            }
        }
        Ok(())
    }

    /// @param  Job    $job
    fn handle_redirect(
        &self,
        job: &IndexMap<String, PhpMixed>,
        response: &CurlResponse,
    ) -> anyhow::Result<Result<String, TransportException>> {
        let mut target_url = String::new();
        if let Some(location_header) = response.inner.get_header("location") {
            if !location_header.is_empty() {
                if !parse_url(&location_header, shirabe_php_shim::PHP_URL_SCHEME).is_null() {
                    // Absolute URL; e.g. https://example.com/composer
                    target_url = location_header.clone();
                } else if !parse_url(&location_header, shirabe_php_shim::PHP_URL_HOST).is_null() {
                    // Scheme relative; e.g. //example.com/foo
                    let job_url = job.get("url").and_then(|v| v.as_string()).unwrap_or("");
                    target_url = format!(
                        "{}:{}",
                        parse_url(job_url, shirabe_php_shim::PHP_URL_SCHEME)
                            .as_string()
                            .unwrap_or(""),
                        location_header
                    );
                } else if location_header.starts_with('/') {
                    // Absolute path; e.g. /foo
                    let job_url = job.get("url").and_then(|v| v.as_string()).unwrap_or("");
                    let url_host = parse_url(job_url, shirabe_php_shim::PHP_URL_HOST);
                    let url_host_str = url_host.as_string().unwrap_or("");

                    // Replace path using hostname as an anchor.
                    target_url = Preg::replace(
                        &format!(
                            r"{{^(.+(?://|@){}(?::\d+)?)(?:[/\?].*)?$}}",
                            preg_quote(url_host_str, None)
                        ),
                        &format!("\\1{}", location_header),
                        job_url,
                    );
                } else {
                    // Relative path; e.g. foo
                    // This actually differs from PHP which seems to add duplicate slashes.
                    let job_url = job.get("url").and_then(|v| v.as_string()).unwrap_or("");
                    target_url = Preg::replace(
                        r"{^(.+/)[^/?]*(?:\?.*)?$}",
                        &format!("\\1{}", location_header),
                        job_url,
                    );
                }
            }
        }

        if !target_url.is_empty() {
            self.io.write_error(
                PhpMixed::String(sprintf(
                    "Following redirect (%u) %s",
                    &[
                        PhpMixed::Int(
                            job.get("attributes")
                                .and_then(|v| v.as_array())
                                .and_then(|a| a.get("redirects"))
                                .and_then(|b| b.as_int())
                                .unwrap_or(0)
                                + 1,
                        ),
                        PhpMixed::String(Url::sanitize(target_url.clone())),
                    ],
                )),
                true,
                <dyn IOInterface>::DEBUG,
            );

            return Ok(Ok(target_url));
        }

        Ok(Err(TransportException::new(
            format!(
                "The \"{}\" file could not be downloaded, got redirect without Location ({})",
                job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                response.inner.get_status_message().unwrap_or_default()
            ),
            0,
        )))
    }

    /// @param  Job                                          $job
    /// @return array{retry: bool, storeAuth: 'prompt'|bool}
    fn is_authenticated_retry_needed(
        &mut self,
        job: &IndexMap<String, PhpMixed>,
        response: &CurlResponse,
    ) -> anyhow::Result<Result<PromptAuthResult, TransportException>> {
        if in_array(
            PhpMixed::Int(response.inner.get_status_code()),
            &PhpMixed::List(vec![
                Box::new(PhpMixed::Int(401)),
                Box::new(PhpMixed::Int(403)),
            ]),
            false,
        ) && job
            .get("attributes")
            .and_then(|v| v.as_array())
            .and_then(|a| a.get("retryAuthFailure"))
            .and_then(|b| b.as_bool())
            .unwrap_or(false)
        {
            let result = self.auth_helper.prompt_auth_if_needed(
                job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                job.get("origin").and_then(|v| v.as_string()).unwrap_or(""),
                response.inner.get_status_code(),
                response.inner.get_status_message(),
                response.inner.get_headers().clone(),
                job.get("attributes")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get("retries"))
                    .and_then(|b| b.as_int())
                    .unwrap_or(0),
                response.inner.get_body().map(|s| s.to_string()),
            )?;

            if result.retry {
                return Ok(Ok(result));
            }
        }

        let location_header = response.inner.get_header("location");
        let mut needs_auth_retry: Option<&'static str> = None;

        // check for bitbucket login page asking to authenticate
        if job.get("origin").and_then(|v| v.as_string()) == Some("bitbucket.org")
            && !self.auth_helper.is_public_bit_bucket_download(
                job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
            )
            && substr(
                job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                -4,
                None,
            ) == ".zip"
            && (location_header.is_none()
                || substr(location_header.as_deref().unwrap_or(""), -4, None) != ".zip")
            && Preg::is_match(
                r"{^text/html\b}i",
                &response.inner.get_header("content-type").unwrap_or_default(),
            )
        {
            needs_auth_retry = Some("Bitbucket requires authentication and it was not provided");
        }

        // check for gitlab 404 when downloading archives
        let gitlab_domains = self.config.get("gitlab-domains");
        let gitlab_domains_list: Vec<Box<PhpMixed>> = match gitlab_domains {
            PhpMixed::List(l) => l,
            _ => Vec::new(),
        };
        if response.inner.get_status_code() == 404
            && in_array(
                PhpMixed::String(
                    job.get("origin")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string(),
                ),
                &PhpMixed::List(gitlab_domains_list),
                true,
            )
            && strpos(
                job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                "archive.zip",
            )
            .is_some()
        {
            needs_auth_retry = Some("GitLab requires authentication and it was not provided");
        }

        if let Some(msg) = needs_auth_retry {
            if job
                .get("attributes")
                .and_then(|v| v.as_array())
                .and_then(|a| a.get("retryAuthFailure"))
                .and_then(|b| b.as_bool())
                .unwrap_or(false)
            {
                let result = self.auth_helper.prompt_auth_if_needed(
                    job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                    job.get("origin").and_then(|v| v.as_string()).unwrap_or(""),
                    401,
                    None,
                    Vec::new(),
                    job.get("attributes")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("retries"))
                        .and_then(|b| b.as_int())
                        .unwrap_or(0),
                    None,
                )?;
                if result.retry {
                    return Ok(Ok(result));
                }
            }

            return Ok(Err(self.fail_response(job, response, msg)));
        }

        Ok(Ok(PromptAuthResult {
            retry: false,
            store_auth: StoreAuth::Bool(false),
        }))
    }

    /// @param  Job    $job
    /// @param non-empty-string $url
    ///
    /// @param  array{retryAuthFailure?: bool, redirects?: int<0, max>, storeAuth?: 'prompt'|bool, retries?: int<1, max>, ipResolve?: 4|6} $attributes
    fn restart_job(
        &mut self,
        job: &IndexMap<String, PhpMixed>,
        url: &str,
        attributes: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        if let Some(PhpMixed::String(filename)) = job.get("filename") {
            unlink_silent(&format!("{}~", filename));
        }

        let job_attrs = match job.get("attributes") {
            Some(PhpMixed::Array(a)) => a.clone(),
            _ => IndexMap::new(),
        };
        let merged = array_merge(
            PhpMixed::Array(job_attrs),
            PhpMixed::Array(
                attributes
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            ),
        );
        let attributes: IndexMap<String, PhpMixed> = match merged {
            PhpMixed::Array(a) => a.into_iter().map(|(k, v)| (k, *v)).collect(),
            _ => IndexMap::new(),
        };
        let origin = Url::get_origin(&self.config, url);

        let copy_to = job
            .get("filename")
            .and_then(|v| v.as_string())
            .map(|s| s.to_string());
        let options = match job.get("options") {
            Some(PhpMixed::Array(a)) => a.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect(),
            _ => IndexMap::new(),
        };
        // resolve/reject callables are stored Rust-side; pass placeholders for now.
        // TODO(phase-b): forward stored callables here.
        let resolve: Box<dyn Fn(PhpMixed) + Send + Sync> = Box::new(|_| {});
        let reject: Box<dyn Fn(PhpMixed) + Send + Sync> = Box::new(|_| {});
        self.init_download(
            resolve,
            reject,
            &origin,
            url,
            options,
            copy_to.as_deref(),
            attributes,
        )?;
        Ok(())
    }

    /// @param  Job    $job
    /// @param non-empty-string $url
    ///
    /// @param  array{retryAuthFailure?: bool, redirects?: int<0, max>, storeAuth?: 'prompt'|bool, retries: int<1, max>, ipResolve?: 4|6} $attributes
    fn restart_job_with_delay(
        &mut self,
        job: &IndexMap<String, PhpMixed>,
        url: &str,
        attributes: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let retries = attributes
            .get("retries")
            .and_then(|v| v.as_int())
            .unwrap_or(0);
        if retries >= 3 {
            usleep(500000); // half a second delay for 3rd retry and beyond
        } else if retries >= 2 {
            usleep(100000); // 100ms delay for 2nd retry
        } // no sleep for the first retry

        self.restart_job(job, url, attributes)
    }

    /// @param  Job                $job
    fn fail_response(
        &self,
        job: &IndexMap<String, PhpMixed>,
        response: &CurlResponse,
        error_message: &str,
    ) -> TransportException {
        if let Some(PhpMixed::String(filename)) = job.get("filename") {
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
                Box::new(PhpMixed::String("application/json".to_string())),
                Box::new(PhpMixed::String("application/json; charset=utf-8".to_string())),
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
                job.get("url").and_then(|v| v.as_string()).unwrap_or(""),
                error_message,
                details
            ),
            response.inner.get_status_code(),
        )
    }

    /// @param  Job                $job
    fn reject_job(&mut self, job: &IndexMap<String, PhpMixed>, _e: anyhow::Error) {
        if is_resource(job.get("headerHandle").unwrap_or(&PhpMixed::Null)) {
            fclose(job.get("headerHandle").cloned().unwrap_or(PhpMixed::Null));
        }
        if is_resource(job.get("bodyHandle").unwrap_or(&PhpMixed::Null)) {
            fclose(job.get("bodyHandle").cloned().unwrap_or(PhpMixed::Null));
        }
        if let Some(PhpMixed::String(filename)) = job.get("filename") {
            unlink_silent(&format!("{}~", filename));
        }
        // job['reject']($e);
        // TODO(phase-b): invoke stored reject callable
    }

    fn check_curl_result(&self, code: i64) -> anyhow::Result<()> {
        if code != CURLM_OK && code != CURLM_CALL_MULTI_PERFORM {
            anyhow::bail!(
                RuntimeException {
                    message: if self.multi_errors.contains_key(&code) {
                        let info = self.multi_errors.get(&code).unwrap();
                        format!(
                            "cURL error: {} ({}): cURL message: {}",
                            code,
                            info.first().cloned().unwrap_or_default(),
                            info.get(1).cloned().unwrap_or_default()
                        )
                    } else {
                        format!("Unexpected cURL error: {}", code)
                    },
                    code: 0,
                }
                .message
            );
        }
        Ok(())
    }
}

fn maps_equal(
    a: &IndexMap<String, Box<PhpMixed>>,
    b: &IndexMap<String, Box<PhpMixed>>,
) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (k, v) in a {
        match b.get(k) {
            None => return false,
            Some(bv) => {
                if format!("{:?}", v) != format!("{:?}", bv) {
                    return false;
                }
            }
        }
    }
    true
}
