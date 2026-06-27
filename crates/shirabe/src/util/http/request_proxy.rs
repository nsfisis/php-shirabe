//! ref: composer/src/Composer/Util/Http/RequestProxy.php

use indexmap::IndexMap;
use shirabe_php_shim::{
    CURLAUTH_BASIC, CURLOPT_NOPROXY, CURLOPT_PROXY, CURLOPT_PROXY_CAINFO, CURLOPT_PROXY_CAPATH,
    CURLOPT_PROXYAUTH, CURLOPT_PROXYUSERPWD, InvalidArgumentException, PhpMixed,
};

use crate::downloader::TransportException;

// contextOptions = array{http: array{proxy: string, header?: string, request_fulluri?: bool}}
type ContextOptions = IndexMap<String, IndexMap<String, PhpMixed>>;

#[derive(Debug)]
pub struct RequestProxy {
    context_options: Option<ContextOptions>,
    status: Option<String>,
    url: Option<String>,
    auth: Option<String>,
}

impl RequestProxy {
    pub fn new(
        url: Option<String>,
        auth: Option<String>,
        context_options: Option<ContextOptions>,
        status: Option<String>,
    ) -> Self {
        Self {
            url,
            auth,
            context_options,
            status,
        }
    }

    pub fn none() -> Self {
        Self::new(None, None, None, None)
    }

    pub fn no_proxy() -> Self {
        Self::new(None, None, None, Some("excluded by no_proxy".to_string()))
    }

    pub fn get_context_options(&self) -> Option<&ContextOptions> {
        self.context_options.as_ref()
    }

    pub fn get_curl_options(
        &self,
        ssl_options: &IndexMap<String, PhpMixed>,
    ) -> Result<IndexMap<i64, PhpMixed>, TransportException> {
        // PHP guards an HTTPS proxy behind `is_secure() && !supports_secure_proxy()` because
        // libcurl < 7.52.0 cannot speak TLS to a proxy. Shirabe always can (see
        // supports_secure_proxy), so the guard is dropped.

        let mut options: IndexMap<i64, PhpMixed> = IndexMap::new();
        options.insert(
            CURLOPT_PROXY,
            PhpMixed::String(self.url.as_deref().unwrap_or("").to_string()),
        );

        if self.url.is_some() {
            options.insert(CURLOPT_NOPROXY, PhpMixed::String(String::new()));
        }

        if let Some(auth) = &self.auth {
            options.insert(CURLOPT_PROXYAUTH, PhpMixed::Int(CURLAUTH_BASIC));
            options.insert(CURLOPT_PROXYUSERPWD, PhpMixed::String(auth.clone()));
        }

        if self.is_secure() {
            if let Some(cafile) = ssl_options.get("cafile") {
                options.insert(CURLOPT_PROXY_CAINFO, cafile.clone());
            }
            if let Some(capath) = ssl_options.get("capath") {
                options.insert(CURLOPT_PROXY_CAPATH, capath.clone());
            }
        }

        Ok(options)
    }

    pub fn get_status(&self, format: Option<&str>) -> Result<String, InvalidArgumentException> {
        if self.status.is_none() {
            return Ok(String::new());
        }

        let format = format.unwrap_or("%s");
        if format.contains("%s") {
            return Ok(format.replace("%s", self.status.as_deref().unwrap()));
        }

        Err(InvalidArgumentException {
            message: "String format specifier is missing".to_string(),
            code: 0,
        })
    }

    pub fn is_excluded_by_no_proxy(&self) -> bool {
        self.status.is_some() && self.url.is_none()
    }

    pub fn is_secure(&self) -> bool {
        self.url.as_deref().unwrap_or("").starts_with("https://")
    }

    /// Whether a TLS connection to the proxy itself (an `https://` proxy URL) is supported.
    /// In PHP this depends on the libcurl build (`CURL_VERSION_HTTPS_PROXY`, curl >= 7.52.0).
    /// Shirabe's HTTP layer is reqwest with a rustls backend, which always supports it.
    pub fn supports_secure_proxy(&self) -> bool {
        true
    }
}
