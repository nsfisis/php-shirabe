//! ref: composer/src/Composer/Downloader/TransportException.php

use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct TransportException {
    pub message: String,
    pub code: i64,
    pub(crate) headers: Option<Vec<String>>,
    pub(crate) response: Option<String>,
    pub(crate) status_code: Option<i64>,
    pub(crate) response_info: Vec<PhpMixed>,
}

impl TransportException {
    pub fn new(message: String, code: i64) -> Self {
        Self {
            message,
            code,
            headers: None,
            response: None,
            status_code: None,
            response_info: vec![],
        }
    }

    pub fn set_headers(&mut self, headers: Vec<String>) {
        self.headers = Some(headers);
    }

    pub fn get_headers(&self) -> Option<&Vec<String>> {
        self.headers.as_ref()
    }

    pub fn set_response(&mut self, response: Option<String>) {
        self.response = response;
    }

    pub fn get_response(&self) -> Option<&str> {
        self.response.as_deref()
    }

    pub fn set_status_code(&mut self, status_code: Option<i64>) {
        self.status_code = status_code;
    }

    pub fn get_status_code(&self) -> Option<i64> {
        self.status_code
    }

    pub fn get_response_info(&self) -> &Vec<PhpMixed> {
        &self.response_info
    }

    pub fn set_response_info(&mut self, response_info: Vec<PhpMixed>) {
        self.response_info = response_info;
    }
}
