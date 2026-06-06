//! ref: composer/src/Composer/Util/Http/CurlResponse.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use super::Response;

#[derive(Debug)]
pub struct CurlResponse {
    pub(crate) inner: Response,
    curl_info: IndexMap<String, PhpMixed>,
}

impl CurlResponse {
    pub fn new(
        url: String,
        code: Option<i64>,
        headers: Vec<String>,
        body: Option<String>,
        curl_info: IndexMap<String, PhpMixed>,
    ) -> Self {
        let inner = Response::new(url, code, headers, body);
        Self { inner, curl_info }
    }

    pub fn get_curl_info(&self) -> &IndexMap<String, PhpMixed> {
        &self.curl_info
    }
}
