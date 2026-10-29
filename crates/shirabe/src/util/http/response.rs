//! ref: composer/src/Composer/Util/Http/Response.php

use crate::json::JsonFile;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{PhpMixed, legacy_lossy, php_regex, preg_is_match, preg_match, preg_quote};

#[derive(Debug)]
pub struct Response {
    url: BString,
    code: i64,
    headers: Vec<BString>,
    body: Option<Vec<u8>>,
}

impl Response {
    pub fn new(
        url: BString,
        code: Option<i64>,
        headers: Vec<BString>,
        body: Option<Vec<u8>>,
    ) -> Self {
        Self {
            url,
            code: code.unwrap_or(0),
            headers,
            body,
        }
    }

    /// The url of the request this response answered. PHP keeps the whole request array in a
    /// private property with no getter, and the plugin boundary codec needs the url out of it to
    /// rebuild the value in the child.
    pub(crate) fn request_url(&self) -> &BStr {
        self.url.as_bstr()
    }

    pub fn get_status_code(&self) -> i64 {
        self.code
    }

    pub fn get_status_message(&self) -> Option<BString> {
        let mut value = None;
        for header in &self.headers {
            if preg_is_match(php_regex!(r"{^HTTP/\S+ \d+}i"), header) {
                // In case of redirects, headers contain the headers of all responses
                // so we can not return directly and need to keep iterating
                value = Some(header.clone());
            }
        }
        value
    }

    pub fn get_headers(&self) -> &Vec<BString> {
        &self.headers
    }

    pub fn get_header(&self, name: &str) -> Option<BString> {
        Self::find_header_value(&self.headers, name)
    }

    pub fn get_body(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }

    pub fn decode_json(&self) -> anyhow::Result<PhpMixed> {
        JsonFile::parse_json(self.body.as_deref(), Some(&legacy_lossy(&self.url)))
    }

    pub fn collect(&mut self) {
        self.url = BString::default();
        self.code = 0;
        self.headers = vec![];
        self.body = None;
    }

    pub fn find_header_value(headers: &[BString], name: &str) -> Option<BString> {
        let mut value = None;
        let pattern = format!("{{^{}:\\s*(.+?)\\s*$}}i", preg_quote(name, None));
        for header in headers {
            if let Some(matches) = preg_match(&pattern, header)
                && let Some(s) = matches.get(1)
            {
                value = Some(BString::from(s));
            }
        }
        value
    }
}
