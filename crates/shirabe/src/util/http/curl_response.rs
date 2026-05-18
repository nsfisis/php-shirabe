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
    ) -> anyhow::Result<Result<Self, shirabe_php_shim::LogicException>> {
        match Response::new(request, code, headers, body)? {
            Ok(inner) => Ok(Ok(Self { inner, curl_info })),
            Err(e) => Ok(Err(e)),
        }
    }

    pub fn get_curl_info(&self) -> &IndexMap<String, PhpMixed> {
        &self.curl_info
    }
}
