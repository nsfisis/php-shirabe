//! ref: composer/src/Composer/Util/Http/Response.php

use crate::json::JsonFile;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{PhpMixed, preg_quote};

#[derive(Debug)]
pub struct Response {
    url: String,
    code: i64,
    headers: Vec<String>,
    body: Option<String>,
}

impl Response {
    pub fn new(url: String, code: Option<i64>, headers: Vec<String>, body: Option<String>) -> Self {
        Self {
            url,
            code: code.unwrap_or(0),
            headers,
            body,
        }
    }

    pub fn get_status_code(&self) -> i64 {
        self.code
    }

    pub fn get_status_message(&self) -> Option<String> {
        let mut value = None;
        for header in &self.headers {
            if Preg::is_match(r"(?i)^HTTP/\S+ \d+", header) {
                // In case of redirects, headers contain the headers of all responses
                // so we can not return directly and need to keep iterating
                value = Some(header.clone());
            }
        }
        value
    }

    pub fn get_headers(&self) -> &Vec<String> {
        &self.headers
    }

    pub fn get_header(&self, name: &str) -> Option<String> {
        Self::find_header_value(&self.headers, name)
    }

    pub fn get_body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    pub fn decode_json(&self) -> anyhow::Result<PhpMixed> {
        JsonFile::parse_json(self.body.as_deref(), Some(self.url.as_str()))
    }

    pub fn collect(&mut self) {
        self.url = String::new();
        self.code = 0;
        self.headers = vec![];
        self.body = None;
    }

    pub fn find_header_value(headers: &[String], name: &str) -> Option<String> {
        let mut value = None;
        let pattern = format!("{{^{}:\\s*(.+?)\\s*$}}i", preg_quote(name, None));
        for header in headers {
            let mut matches: indexmap::IndexMap<
                shirabe_external_packages::composer::pcre::CaptureKey,
                String,
            > = indexmap::IndexMap::new();
            if Preg::match3(&pattern, header, Some(&mut matches))
                && let Some(s) =
                    matches.get(&shirabe_external_packages::composer::pcre::CaptureKey::ByIndex(1))
            {
                value = Some(s.clone());
            }
        }
        value
    }
}
