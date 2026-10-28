//! ref: composer/src/Composer/Util/Http/CurlResponse.php
//!
//! The response `NativeHttpDownloader` builds, in the place of Composer's `CurlResponse`.

use super::Response;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct NativeHttpResponse {
    pub(crate) inner: Response,
    transfer_info: IndexMap<BString, PhpMixed>,
}

impl NativeHttpResponse {
    pub fn new(
        url: String,
        code: Option<i64>,
        headers: Vec<String>,
        body: Option<String>,
        transfer_info: IndexMap<BString, PhpMixed>,
    ) -> Self {
        let inner = Response::new(url, code, headers, body);
        Self {
            inner,
            transfer_info,
        }
    }

    pub fn get_transfer_info(&self) -> &IndexMap<BString, PhpMixed> {
        &self.transfer_info
    }
}
