//! ref: composer/src/Composer/Plugin/PreFileDownloadEvent.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::event_dispatcher::event::Event;
use crate::util::http_downloader::HttpDownloader;

#[derive(Debug)]
pub struct PreFileDownloadEvent {
    inner: Event,
    http_downloader: HttpDownloader,
    processed_url: String,
    custom_cache_key: Option<String>,
    r#type: String,
    context: PhpMixed,
    transport_options: IndexMap<String, Box<PhpMixed>>,
}

impl PreFileDownloadEvent {
    pub fn new(
        name: String,
        http_downloader: HttpDownloader,
        processed_url: String,
        r#type: String,
        context: PhpMixed,
    ) -> Self {
        Self {
            inner: Event::new(name, vec![], IndexMap::new()),
            http_downloader,
            processed_url,
            custom_cache_key: None,
            r#type,
            context,
            transport_options: IndexMap::new(),
        }
    }

    pub fn get_http_downloader(&self) -> &HttpDownloader {
        &self.http_downloader
    }

    pub fn get_processed_url(&self) -> &str {
        &self.processed_url
    }

    pub fn set_processed_url(&mut self, processed_url: String) {
        self.processed_url = processed_url;
    }

    pub fn get_custom_cache_key(&self) -> Option<&str> {
        self.custom_cache_key.as_deref()
    }

    pub fn set_custom_cache_key(&mut self, custom_cache_key: Option<String>) {
        self.custom_cache_key = custom_cache_key;
    }

    pub fn get_type(&self) -> &str {
        &self.r#type
    }

    pub fn get_context(&self) -> &PhpMixed {
        &self.context
    }

    pub fn get_transport_options(&self) -> &IndexMap<String, Box<PhpMixed>> {
        &self.transport_options
    }

    pub fn set_transport_options(&mut self, options: IndexMap<String, Box<PhpMixed>>) {
        self.transport_options = options;
    }
}
