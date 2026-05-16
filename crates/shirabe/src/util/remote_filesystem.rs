//! ref: composer/src/Composer/Util/RemoteFilesystem.php

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    array_replace_recursive, base64_encode, explode, extension_loaded, file_put_contents,
    filter_var, ini_get, json_decode, parse_url, preg_quote, restore_error_handler,
    set_error_handler, sprintf, strpos, strtolower, strtr, substr, trim,
    PhpMixed, RuntimeException, FILTER_VALIDATE_BOOLEAN, PHP_URL_HOST, PHP_URL_PATH, PHP_VERSION_ID,
};

use crate::config::Config;
use crate::downloader::http_downloader::HttpDownloader;
use crate::downloader::max_file_size_exceeded_exception::MaxFileSizeExceededException;
use crate::downloader::transport_exception::TransportException;
use crate::io::io_interface::IOInterface;
use crate::util::auth_helper::AuthHelper;
use crate::util::http::proxy_manager::ProxyManager;
use crate::util::http::response::Response;
use crate::util::platform::Platform;
use crate::util::stream_context_factory::StreamContextFactory;
use crate::util::url::Url;

/// Result of `RemoteFilesystem::get` — string content, `true` (for copy), or `false`.
#[derive(Debug, Clone)]
pub enum GetResult {
    False,
    True,
    Content(String),
}

#[derive(Debug)]
pub struct RemoteFilesystem {
    io: Box<dyn IOInterface>,
    config: Config,
    scheme: String,
    bytes_max: i64,
    origin_url: String,
    file_url: String,
    file_name: Option<String>,
    retry: bool,
    progress: bool,
    last_progress: Option<i64>,
    options: IndexMap<String, PhpMixed>,
    disable_tls: bool,
    last_headers: Vec<String>,
    store_auth: bool,
    auth_helper: AuthHelper,
    degraded_mode: bool,
    redirects: i64,
    max_redirects: i64,
}

impl RemoteFilesystem {
    pub fn new(
        io: Box<dyn IOInterface>,
        config: Config,
        options: IndexMap<String, PhpMixed>,
        disable_tls: bool,
        auth_helper: Option<AuthHelper>,
    ) -> Self {
        let (computed_options, disable_tls_set) = if !disable_tls {
            (StreamContextFactory::get_tls_defaults(&options, &*io), false)
        } else {
            (IndexMap::new(), true)
        };

        let merged = array_replace_recursive(computed_options, options);
        let auth_helper =
            auth_helper.unwrap_or_else(|| AuthHelper::new(io.clone_box(), config.clone()));
        Self {
            io,
            config,
            scheme: String::new(),
            bytes_max: 0,
            origin_url: String::new(),
            file_url: String::new(),
            file_name: None,
            retry: false,
            progress: false,
            last_progress: None,
            options: merged,
            disable_tls: disable_tls_set,
            last_headers: Vec::new(),
            store_auth: false,
            auth_helper,
            degraded_mode: false,
            redirects: 0,
            max_redirects: 20,
        }
    }

    pub fn copy(
        &mut self,
        origin_url: &str,
        file_url: &str,
        file_name: &str,
        progress: bool,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<GetResult> {
        self.get(origin_url, file_url, options, Some(file_name.to_string()), progress)
    }

    pub fn get_contents(
        &mut self,
        origin_url: &str,
        file_url: &str,
        progress: bool,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<GetResult> {
        self.get(origin_url, file_url, options, None, progress)
    }

    pub fn get_options(&self) -> &IndexMap<String, PhpMixed> {
        &self.options
    }

    pub fn set_options(&mut self, options: IndexMap<String, PhpMixed>) {
        self.options = array_replace_recursive(self.options.clone(), options);
    }

    pub fn is_tls_disabled(&self) -> bool {
        self.disable_tls
    }

    pub fn get_last_headers(&self) -> &[String] {
        &self.last_headers
    }

    pub fn find_status_code(headers: &[String]) -> Option<i64> {
        let mut value: Option<i64> = None;
        for header in headers {
            if let Ok(Some(m)) =
                Preg::is_match_strict_groups("{^HTTP/\\S+ (\\d+)}i", header)
            {
                value = Some(m["1"].parse().unwrap_or(0));
            }
        }

        value
    }

    pub fn find_status_message(&self, headers: &[String]) -> Option<String> {
        let mut value: Option<String> = None;
        for header in headers {
            if Preg::is_match("{^HTTP/\\S+ \\d+}i", header).unwrap_or(false) {
                value = Some(header.clone());
            }
        }

        value
    }

    fn get(
        &mut self,
        origin_url: &str,
        file_url: &str,
        additional_options: IndexMap<String, PhpMixed>,
        file_name: Option<String>,
        progress: bool,
    ) -> anyhow::Result<GetResult> {
        // TODO(phase-b): PHP_URL_SCHEME constant isn't yet in the shim; PHP_URL_HOST stands in.
        self.scheme = parse_url(&strtr(file_url, "\\", "/"), PHP_URL_HOST)
            .as_string()
            .unwrap_or("")
            .to_string();
        self.bytes_max = 0;
        self.origin_url = origin_url.to_string();
        self.file_url = file_url.to_string();
        self.file_name = file_name.clone();
        self.progress = progress;
        self.last_progress = None;
        let mut retry_auth_failure = true;
        self.last_headers = Vec::new();
        self.redirects = 1; // The first request counts.

        let mut temp_additional_options = additional_options.clone();
        if let Some(v) = temp_additional_options.get("retry-auth-failure").cloned() {
            retry_auth_failure = v.as_bool().unwrap_or(true);
            temp_additional_options.shift_remove("retry-auth-failure");
        }

        let mut is_redirect = false;
        if let Some(v) = temp_additional_options.get("redirects").cloned() {
            self.redirects = v.as_int().unwrap_or(self.redirects);
            is_redirect = true;
            temp_additional_options.shift_remove("redirects");
        }

        let mut options = self.get_options_for_url(origin_url, temp_additional_options);

        let orig_file_url = file_url.to_string();
        let mut file_url = file_url.to_string();

        if options.contains_key("prevent_ip_access_callable") {
            return Err(anyhow::anyhow!(RuntimeException {
                message:
                    "RemoteFilesystem doesn't support the 'prevent_ip_access_callable' config."
                        .to_string(),
                code: 0,
            }));
        }

        if let Some(token) = options.get("gitlab-token").cloned() {
            let separator = if strpos(&file_url, "?").is_none() {
                "?"
            } else {
                "&"
            };
            file_url = format!(
                "{}{}access_token={}",
                file_url,
                separator,
                token.as_string().unwrap_or("")
            );
            options.shift_remove("gitlab-token");
        }

        if let Some(http_opts) = options.get_mut("http") {
            if let PhpMixed::Array(m) = http_opts {
                m.insert(
                    "ignore_errors".to_string(),
                    Box::new(PhpMixed::Bool(true)),
                );
            }
        }

        let mut degraded_packagist = false;
        if self.degraded_mode && strpos(&file_url, "http://repo.packagist.org/") == Some(0) {
            file_url = format!(
                "http://{}{}",
                Platform::gethostbyname("repo.packagist.org"),
                substr(&file_url, 20, None)
            );
            degraded_packagist = true;
        }

        let mut max_file_size: Option<i64> = None;
        if let Some(v) = options.get("max_file_size").cloned() {
            max_file_size = v.as_int();
            options.shift_remove("max_file_size");
        }

        // TODO(plugin): `Closure::fromCallable([$this, 'callbackGet'])` for stream notification.
        let ctx = StreamContextFactory::get_context(&file_url, &options, &IndexMap::new());

        let proxy = ProxyManager::get_instance().get_proxy_for_request(&file_url);
        let using_proxy = proxy.get_status(" using proxy (%s)");
        self.io.write_error(
            PhpMixed::String(format!(
                "{}{}{}",
                if strpos(&orig_file_url, "http") == Some(0) {
                    "Downloading "
                } else {
                    "Reading "
                },
                Url::sanitize(&orig_file_url),
                using_proxy
            )),
            true,
            <dyn IOInterface>::DEBUG,
        );

        if (!Preg::is_match("{^http://(repo\\.)?packagist\\.org/p/}", &file_url).unwrap_or(false)
            || (strpos(&file_url, "$").is_none() && strpos(&file_url, "%24").is_none()))
            && !degraded_packagist
        {
            self.config.prohibit_url_by_config(&file_url, &*self.io);
        }

        if self.progress && !is_redirect {
            self.io.write_error(
                PhpMixed::String("Downloading (<comment>connecting...</comment>)".to_string()),
                false,
                <dyn IOInterface>::NORMAL,
            );
        }

        let mut error_message = String::new();
        let error_code = 0_i64;
        let mut result: Option<String> = None;
        // TODO(phase-b): set_error_handler with a closure capturing `error_message` by reference.
        set_error_handler(|_code, _msg, _file, _line| true);

        let mut http_response_header: Vec<String> = Vec::new();
        let inner_result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
            result = self.get_remote_contents(
                origin_url,
                &file_url,
                &ctx,
                &mut http_response_header,
                max_file_size,
            )?;

            if !http_response_header.is_empty() && !http_response_header[0].is_empty() {
                let status_code = Self::find_status_code(&http_response_header);
                if let Some(code) = status_code {
                    if code >= 300
                        && Response::find_header_value(&http_response_header, "content-type")
                            .as_deref()
                            == Some("application/json")
                    {
                        let parsed = result
                            .as_deref()
                            .map(|s| json_decode(s, true).unwrap_or(PhpMixed::Null))
                            .unwrap_or(PhpMixed::Null);
                        HttpDownloader::output_warnings(&*self.io, origin_url, &parsed);
                    }

                    if [401_i64, 403].contains(&code) && retry_auth_failure {
                        let status_message = self.find_status_message(&http_response_header);
                        self.prompt_auth_and_retry(
                            code,
                            status_message,
                            http_response_header.clone(),
                        )?;
                    }
                }
            }

            let content_length =
                if !http_response_header.is_empty() && !http_response_header[0].is_empty() {
                    Response::find_header_value(&http_response_header, "content-length")
                } else {
                    None
                };
            if let Some(cl) = content_length {
                let cl_int: i64 = cl.parse().unwrap_or(0);
                if cl_int > 0 && Platform::strlen(result.as_deref().unwrap_or("")) < cl_int {
                    let mut e = TransportException::new(format!(
                        "Content-Length mismatch, received {} bytes out of the expected {}",
                        Platform::strlen(result.as_deref().unwrap_or("")),
                        cl_int
                    ));
                    e.set_headers(http_response_header.clone());
                    e.set_status_code(Self::find_status_code(&http_response_header));
                    let decoded = self
                        .decode_result(result.as_deref(), &http_response_header)
                        .unwrap_or_else(|_| self.normalize_result(result.as_deref()));
                    e.set_response(decoded);

                    self.io.write_error(
                        PhpMixed::String(format!(
                            "Content-Length mismatch, received {} out of {} bytes: ({})",
                            Platform::strlen(result.as_deref().unwrap_or("")),
                            cl_int,
                            base64_encode(result.as_deref().unwrap_or(""))
                        )),
                        true,
                        <dyn IOInterface>::DEBUG,
                    );

                    return Err(anyhow::anyhow!(e));
                }
            }
            Ok(())
        })();
        let mut caught_e: Option<anyhow::Error> = None;
        if let Err(mut e) = inner_result {
            if let Some(te) = e.downcast_mut::<TransportException>() {
                if !http_response_header.is_empty() && !http_response_header[0].is_empty() {
                    te.set_headers(http_response_header.clone());
                    te.set_status_code(Self::find_status_code(&http_response_header));
                }
                if result.is_some() {
                    if let Ok(decoded) =
                        self.decode_result(result.as_deref(), &http_response_header)
                    {
                        te.set_response(decoded);
                    }
                }
            }
            caught_e = Some(e);
            result = None;
        }
        if !error_message.is_empty()
            && !filter_var(
                &ini_get("allow_url_fopen").unwrap_or_default(),
                FILTER_VALIDATE_BOOLEAN,
            )
        {
            error_message = format!("allow_url_fopen must be enabled in php.ini ({})", error_message);
        }
        restore_error_handler();
        if let Some(e) = caught_e {
            if !self.retry {
                let msg_owned = format!("{}", e);
                if !self.degraded_mode && strpos(&msg_owned, "Operation timed out").is_some() {
                    self.degraded_mode = true;
                    self.io.write_error(
                        PhpMixed::String("".to_string()),
                        true,
                        <dyn IOInterface>::NORMAL,
                    );
                    self.io.write_error(
                        PhpMixed::List(vec![
                            Box::new(PhpMixed::String(format!("<error>{}</error>", msg_owned))),
                            Box::new(PhpMixed::String(
                                "<error>Retrying with degraded mode, check https://getcomposer.org/doc/articles/troubleshooting.md#degraded-mode for more info</error>"
                                    .to_string(),
                            )),
                        ]),
                        true,
                        <dyn IOInterface>::NORMAL,
                    );

                    return self.get(
                        &self.origin_url.clone(),
                        &self.file_url.clone(),
                        additional_options,
                        self.file_name.clone(),
                        self.progress,
                    );
                }

                return Err(e);
            }
        }

        let mut status_code: Option<i64> = None;
        let mut content_type: Option<String> = None;
        let mut location_header: Option<String> = None;
        if !http_response_header.is_empty() && !http_response_header[0].is_empty() {
            status_code = Self::find_status_code(&http_response_header);
            content_type = Response::find_header_value(&http_response_header, "content-type");
            location_header = Response::find_header_value(&http_response_header, "location");
        }

        let bitbucket_login_match = origin_url == "bitbucket.org"
            && !self.auth_helper.is_public_bit_bucket_download(&self.file_url)
            && substr(&self.file_url, -4, None) == ".zip"
            && (location_header.is_none()
                || substr(
                    &parse_url(location_header.as_deref().unwrap_or(""), PHP_URL_PATH)
                        .as_string()
                        .unwrap_or("")
                        .to_string(),
                    -4,
                    None,
                ) != ".zip")
            && content_type.is_some()
            && Preg::is_match("{^text/html\\b}i", content_type.as_deref().unwrap_or(""))
                .unwrap_or(false);
        if bitbucket_login_match {
            result = None;
            if retry_auth_failure {
                self.prompt_auth_and_retry(401, None, Vec::new())?;
            }
        }

        let gitlab_domains: Vec<String> = self
            .config
            .get("gitlab-domains")
            .and_then(|v| v.as_list())
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if status_code == Some(404)
            && gitlab_domains.iter().any(|d| d == origin_url)
            && strpos(&self.file_url, "archive.zip").is_some()
        {
            result = None;
            if retry_auth_failure {
                self.prompt_auth_and_retry(401, None, Vec::new())?;
            }
        }

        let mut has_followed_redirect = false;
        if let Some(code) = status_code {
            if code >= 300 && code <= 399 && code != 304 && self.redirects < self.max_redirects {
                has_followed_redirect = true;
                result = self.handle_redirect(
                    &http_response_header,
                    additional_options.clone(),
                    result.clone(),
                )?;
            }
        }

        if let Some(code) = status_code {
            if code >= 400 && code <= 599 {
                if !self.retry {
                    if self.progress && !is_redirect {
                        self.io.overwrite_error(
                            PhpMixed::String("Downloading (<error>failed</error>)".to_string()),
                            false,
                            None,
                            <dyn IOInterface>::NORMAL,
                        );
                    }

                    let mut e = TransportException::new_with_code(
                        format!(
                            "The \"{}\" file could not be downloaded ({})",
                            self.file_url, http_response_header[0]
                        ),
                        code,
                    );
                    e.set_headers(http_response_header.clone());
                    let decoded = self
                        .decode_result(result.as_deref(), &http_response_header)
                        .unwrap_or(None);
                    e.set_response(decoded);
                    e.set_status_code(Some(code));
                    return Err(anyhow::anyhow!(e));
                }
                result = None;
            }
        }

        if self.progress && !self.retry && !is_redirect {
            self.io.overwrite_error(
                PhpMixed::String(format!(
                    "Downloading ({})",
                    if result.is_none() {
                        "<error>failed</error>"
                    } else {
                        "<comment>100%</comment>"
                    }
                )),
                false,
                None,
                <dyn IOInterface>::NORMAL,
            );
        }

        if result.is_some()
            && extension_loaded("zlib")
            && strpos(&file_url, "http") == Some(0)
            && !has_followed_redirect
        {
            match self.decode_result(result.as_deref(), &http_response_header) {
                Ok(decoded) => {
                    result = decoded;
                }
                Err(e) => {
                    if self.degraded_mode {
                        return Err(e);
                    }

                    self.degraded_mode = true;
                    self.io.write_error(
                        PhpMixed::List(vec![
                            Box::new(PhpMixed::String("".to_string())),
                            Box::new(PhpMixed::String(format!(
                                "<error>Failed to decode response: {}</error>",
                                e
                            ))),
                            Box::new(PhpMixed::String(
                                "<error>Retrying with degraded mode, check https://getcomposer.org/doc/articles/troubleshooting.md#degraded-mode for more info</error>"
                                    .to_string(),
                            )),
                        ]),
                        true,
                        <dyn IOInterface>::NORMAL,
                    );

                    return self.get(
                        &self.origin_url.clone(),
                        &self.file_url.clone(),
                        additional_options,
                        self.file_name.clone(),
                        self.progress,
                    );
                }
            }
        }

        if result.is_some() && file_name.is_some() && !is_redirect {
            let result_str = result.as_deref().unwrap();
            if result_str.is_empty() {
                return Err(anyhow::anyhow!(TransportException::new(format!(
                    "\"{}\" appears broken, and returned an empty 200 response",
                    self.file_url
                ))));
            }

            let put_error_message = String::new();
            // TODO(phase-b): set_error_handler closure that captures `put_error_message` by reference
            set_error_handler(|_code, _msg, _file, _line| true);
            let write_result = file_put_contents(
                file_name.as_deref().unwrap(),
                result_str.as_bytes(),
            );
            restore_error_handler();
            if write_result.is_none() {
                return Err(anyhow::anyhow!(TransportException::new(format!(
                    "The \"{}\" file could not be written to {}: {}",
                    self.file_url,
                    file_name.as_deref().unwrap(),
                    put_error_message
                ))));
            }
            let _ = put_error_message;
        }

        if self.retry {
            self.retry = false;

            let new_result = self.get(
                &self.origin_url.clone(),
                &self.file_url.clone(),
                additional_options,
                self.file_name.clone(),
                self.progress,
            )?;

            if self.store_auth {
                self.auth_helper
                    .store_auth(&self.origin_url, PhpMixed::Bool(self.store_auth));
                self.store_auth = false;
            }

            return Ok(new_result);
        }

        if result.is_none() {
            let mut e = TransportException::new_with_code(
                format!(
                    "The \"{}\" file could not be downloaded: {}",
                    self.file_url, error_message
                ),
                error_code,
            );
            if !http_response_header.is_empty() && !http_response_header[0].is_empty() {
                e.set_headers(http_response_header.clone());
            }

            let msg_owned = format!("{}", e);
            if !self.degraded_mode && strpos(&msg_owned, "Operation timed out").is_some() {
                self.degraded_mode = true;
                self.io.write_error(
                    PhpMixed::String("".to_string()),
                    true,
                    <dyn IOInterface>::NORMAL,
                );
                self.io.write_error(
                    PhpMixed::List(vec![
                        Box::new(PhpMixed::String(format!("<error>{}</error>", msg_owned))),
                        Box::new(PhpMixed::String(
                            "<error>Retrying with degraded mode, check https://getcomposer.org/doc/articles/troubleshooting.md#degraded-mode for more info</error>"
                                .to_string(),
                        )),
                    ]),
                    true,
                    <dyn IOInterface>::NORMAL,
                );

                return self.get(
                    &self.origin_url.clone(),
                    &self.file_url.clone(),
                    additional_options,
                    self.file_name.clone(),
                    self.progress,
                );
            }

            return Err(anyhow::anyhow!(e));
        }

        if !http_response_header.is_empty() && !http_response_header[0].is_empty() {
            self.last_headers = http_response_header.clone();
        }

        match result {
            Some(s) => {
                if file_name.is_some() && !is_redirect {
                    Ok(GetResult::True)
                } else {
                    Ok(GetResult::Content(s))
                }
            }
            None => Ok(GetResult::False),
        }
    }

    fn get_remote_contents(
        &self,
        _origin_url: &str,
        _file_url: &str,
        _context: &PhpMixed,
        response_headers: &mut Vec<String>,
        max_file_size: Option<i64>,
    ) -> anyhow::Result<Option<String>> {
        let mut result: Option<String> = None;

        if PHP_VERSION_ID >= 80400 {
            Platform::http_clear_last_response_headers();
        }

        let mut caught_e: Option<anyhow::Error> = None;
        // TODO(phase-b): wrap PHP's `file_get_contents` with stream context and error capture.
        let outer: Result<Option<String>, anyhow::Error> = Ok(None);
        match outer {
            Ok(v) => result = v,
            Err(e) => caught_e = Some(e),
        }

        if let Some(ref r) = result {
            if let Some(max) = max_file_size {
                if Platform::strlen(r) >= max {
                    return Err(anyhow::anyhow!(MaxFileSizeExceededException::new(format!(
                        "Maximum allowed download size reached. Downloaded {} of allowed {} bytes",
                        Platform::strlen(r),
                        max
                    ))));
                }
            }
        }

        if PHP_VERSION_ID >= 80400 {
            *response_headers = Platform::http_get_last_response_headers().unwrap_or_default();
            Platform::http_clear_last_response_headers();
        } else {
            // TODO(phase-b): read the magic `$http_response_header` PHP variable.
            *response_headers = Vec::new();
        }

        if let Some(e) = caught_e {
            return Err(e);
        }

        Ok(result)
    }

    fn callback_get(
        &mut self,
        notification_code: i64,
        _severity: i64,
        message: Option<String>,
        message_code: i64,
        bytes_transferred: i64,
        bytes_max: i64,
    ) -> anyhow::Result<()> {
        match notification_code {
            x if x == Platform::STREAM_NOTIFY_FAILURE => {
                if 400 == message_code {
                    return Err(anyhow::anyhow!(TransportException::new_with_code(
                        format!(
                            "The '{}' URL could not be accessed: {}",
                            self.file_url,
                            message.unwrap_or_default()
                        ),
                        message_code,
                    )));
                }
            }
            x if x == Platform::STREAM_NOTIFY_FILE_SIZE_IS => {
                self.bytes_max = bytes_max;
            }
            x if x == Platform::STREAM_NOTIFY_PROGRESS => {
                if self.bytes_max > 0 && self.progress {
                    let progression = std::cmp::min(
                        100_i64,
                        ((bytes_transferred as f64 / self.bytes_max as f64) * 100.0).round() as i64,
                    );

                    if 0 == progression % 5
                        && 100 != progression
                        && Some(progression) != self.last_progress
                    {
                        self.last_progress = Some(progression);
                        self.io.overwrite_error(
                            PhpMixed::String(format!(
                                "Downloading (<comment>{}%</comment>)",
                                progression
                            )),
                            false,
                            None,
                            <dyn IOInterface>::NORMAL,
                        );
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn prompt_auth_and_retry(
        &mut self,
        http_status: i64,
        reason: Option<String>,
        headers: Vec<String>,
    ) -> anyhow::Result<()> {
        let result = self.auth_helper.prompt_auth_if_needed(
            &self.file_url,
            &self.origin_url,
            http_status,
            reason,
            headers,
            1,
        );

        self.store_auth = result.store_auth;
        self.retry = result.retry;

        if self.retry {
            return Err(anyhow::anyhow!(TransportException::new("RETRY".to_string())));
        }
        Ok(())
    }

    fn get_options_for_url(
        &self,
        origin_url: &str,
        additional_options: IndexMap<String, PhpMixed>,
    ) -> IndexMap<String, PhpMixed> {
        let tls_options: IndexMap<String, PhpMixed> = IndexMap::new();
        let mut headers: Vec<String> = Vec::new();

        if extension_loaded("zlib") {
            headers.push("Accept-Encoding: gzip".to_string());
        }

        let mut options = array_replace_recursive(self.options.clone(), tls_options);
        options = array_replace_recursive(options, additional_options);
        if !self.degraded_mode {
            let http_entry = options
                .entry("http".to_string())
                .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
            if let PhpMixed::Array(m) = http_entry {
                m.insert(
                    "protocol_version".to_string(),
                    Box::new(PhpMixed::Float(1.1)),
                );
            }
            headers.push("Connection: close".to_string());
        }

        let header_is_string = options
            .get("http")
            .and_then(|v| v.as_array())
            .and_then(|m| m.get("header"))
            .map(|v| matches!(v.as_ref(), PhpMixed::String(_)))
            .unwrap_or(false);
        if header_is_string {
            let header_str = options["http"].as_array().unwrap()["header"]
                .as_string()
                .unwrap_or("")
                .to_string();
            let split = explode("\r\n", &trim(&header_str, "\r\n"));
            if let Some(PhpMixed::Array(m)) = options.get_mut("http") {
                m.insert(
                    "header".to_string(),
                    Box::new(PhpMixed::List(
                        split
                            .into_iter()
                            .map(|s| Box::new(PhpMixed::String(s)))
                            .collect(),
                    )),
                );
            }
        }
        options =
            self.auth_helper
                .add_authentication_options(options, origin_url, &self.file_url);

        let http_entry = options
            .entry("http".to_string())
            .or_insert_with(|| PhpMixed::Array(IndexMap::new()));
        if let PhpMixed::Array(m) = http_entry {
            m.insert("follow_location".to_string(), Box::new(PhpMixed::Int(0)));
        }

        for header in headers {
            if let Some(PhpMixed::Array(m)) = options.get_mut("http") {
                let header_list = m
                    .entry("header".to_string())
                    .or_insert_with(|| Box::new(PhpMixed::List(Vec::new())));
                if let PhpMixed::List(l) = header_list.as_mut() {
                    l.push(Box::new(PhpMixed::String(header)));
                }
            }
        }

        options
    }

    fn handle_redirect(
        &mut self,
        response_headers: &[String],
        mut additional_options: IndexMap<String, PhpMixed>,
        result: Option<String>,
    ) -> anyhow::Result<Option<String>> {
        let mut target_url: Option<String> = None;
        if let Some(location_header) =
            Response::find_header_value(response_headers, "location")
        {
            // TODO(phase-b): use PHP_URL_SCHEME once available to detect absolute URLs.
            if !parse_url(&location_header, PHP_URL_HOST)
                .as_string()
                .unwrap_or("")
                .is_empty()
                && location_header.contains("://")
            {
                target_url = Some(location_header);
            } else if parse_url(&location_header, PHP_URL_HOST)
                .as_string()
                .map(|s| !s.is_empty())
                .unwrap_or(false)
            {
                target_url = Some(format!("{}:{}", self.scheme, location_header));
            } else if location_header.starts_with('/') {
                let url_host = parse_url(&self.file_url, PHP_URL_HOST)
                    .as_string()
                    .unwrap_or("")
                    .to_string();

                target_url = Some(
                    Preg::replace(
                        &format!(
                            "{{^(.+(?://|@){}(?::\\d+)?)(?:[/\\?].*)?$}}",
                            preg_quote(&url_host, None)
                        ),
                        &format!("\\1{}", location_header),
                        &self.file_url,
                    )
                    .unwrap_or_else(|_| self.file_url.clone()),
                );
            } else {
                target_url = Some(
                    Preg::replace(
                        "{^(.+/)[^/?]*(?:\\?.*)?$}",
                        &format!("\\1{}", location_header),
                        &self.file_url,
                    )
                    .unwrap_or_else(|_| self.file_url.clone()),
                );
            }
        }

        if let Some(target_url) = target_url {
            self.redirects += 1;

            self.io.write_error(
                PhpMixed::String("".to_string()),
                true,
                <dyn IOInterface>::DEBUG,
            );
            self.io.write_error(
                PhpMixed::String(sprintf(
                    "Following redirect (%u) %s",
                    &[
                        PhpMixed::Int(self.redirects),
                        PhpMixed::String(Url::sanitize(&target_url)),
                    ],
                )),
                true,
                <dyn IOInterface>::DEBUG,
            );

            additional_options.insert(
                "redirects".to_string(),
                PhpMixed::Int(self.redirects),
            );

            let host = parse_url(&target_url, PHP_URL_HOST)
                .as_string()
                .unwrap_or("")
                .to_string();
            let res = self.get(
                &host,
                &target_url,
                additional_options,
                self.file_name.clone(),
                self.progress,
            )?;
            return Ok(match res {
                GetResult::Content(s) => Some(s),
                _ => None,
            });
        }

        if !self.retry {
            let mut e = TransportException::new(format!(
                "The \"{}\" file could not be downloaded, got redirect without Location ({})",
                self.file_url,
                response_headers.first().map(|s| s.as_str()).unwrap_or("")
            ));
            e.set_headers(response_headers.to_vec());
            let decoded = self.decode_result(result.as_deref(), response_headers)?;
            e.set_response(decoded);

            return Err(anyhow::anyhow!(e));
        }

        Ok(None)
    }

    fn decode_result(
        &self,
        result: Option<&str>,
        response_headers: &[String],
    ) -> anyhow::Result<Option<String>> {
        let mut result = result.map(|s| s.to_string());
        if result.is_some() && extension_loaded("zlib") {
            let content_encoding =
                Response::find_header_value(response_headers, "content-encoding");
            let decode = content_encoding
                .as_deref()
                .map(|s| "gzip" == strtolower(s))
                .unwrap_or(false);

            if decode {
                let decoded = Platform::zlib_decode(result.as_deref().unwrap_or(""));

                result = match decoded {
                    Some(d) => Some(d),
                    None => {
                        return Err(anyhow::anyhow!(TransportException::new(
                            "Failed to decode zlib stream".to_string()
                        )));
                    }
                };
            }
        }

        Ok(self.normalize_result(result.as_deref()))
    }

    fn normalize_result(&self, result: Option<&str>) -> Option<String> {
        result.map(|s| s.to_string())
    }
}
