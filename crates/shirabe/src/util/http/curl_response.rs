//! ref: composer/src/Composer/Util/Http/CurlResponse.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use super::response::Response;

#[derive(Debug)]
pub struct CurlResponse {
    pub(crate) inner: Response,
    curl_info: IndexMap<String, PhpMixed>,
}

impl CurlResponse {
    pub fn new(
        request: IndexMap<String, PhpMixed>,
        code: Option<i64>,
        headers: Vec<String>,
        body: Option<String>,
        curl_info: IndexMap<String, PhpMixed>,
    ) -> Self {
        Self {
            inner: Response::new(request, code, headers, body),
            curl_info,
        }
    }

    pub fn get_curl_info(&self) -> &IndexMap<String, PhpMixed> {
        &self.curl_info
    }
}
