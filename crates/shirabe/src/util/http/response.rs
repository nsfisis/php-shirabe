//! ref: composer/src/Composer/Util/Http/Response.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{LogicException, PhpMixed, preg_quote};
use crate::json::json_file::JsonFile;

#[derive(Debug)]
pub struct Response {
    request: IndexMap<String, PhpMixed>,
    code: i64,
    headers: Vec<String>,
    body: Option<String>,
}

impl Response {
    pub fn new(
        request: IndexMap<String, PhpMixed>,
        code: Option<i64>,
        headers: Vec<String>,
        body: Option<String>,
    ) -> anyhow::Result<Result<Self, LogicException>> {
        if !request.contains_key("url") {
            return Ok(Err(LogicException {
                message: "url key missing from request array".to_string(),
                code: 0,
            }));
        }
        Ok(Ok(Self {
            request,
            code: code.unwrap_or(0),
            headers,
            body,
        }))
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
        let url = self.request.get("url")
            .and_then(|u| u.as_string())
            .unwrap_or("");
        JsonFile::parse_json(self.body.as_deref(), Some(url))
    }

    pub fn collect(&mut self) {
        self.request = IndexMap::new();
        self.code = 0;
        self.headers = vec![];
        self.body = None;
    }

    pub fn find_header_value(headers: &[String], name: &str) -> Option<String> {
        let mut value = None;
        let pattern = format!("(?i)^{}:\\s*(.+?)\\s*$", preg_quote(name, None));
        for header in headers {
            if let Some(m) = Preg::match_(&pattern, header) {
                value = Some(m[1].clone());
            }
        }
        value
    }
}
