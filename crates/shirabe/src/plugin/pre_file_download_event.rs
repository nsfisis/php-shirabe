//! ref: composer/src/Composer/Plugin/PreFileDownloadEvent.php

use crate::event_dispatcher::Event;
use crate::event_dispatcher::EventInterface;
use crate::package::PackageInterfaceHandle;
use crate::util::HttpDownloader;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// The `$context` a file download event carries: the package a dist download is for, or the
/// array a repository passes along with a metadata download.
#[derive(Debug)]
pub enum FileDownloadContext {
    Package(PackageInterfaceHandle),
    /// `['repository' => $this]` (plus `'response' => $response` after the download).
    ///
    /// TODO(plugin): the repository has no handle to itself to put in the array, and
    /// ComposerRepository has no proxy stub, so neither entry is carried.
    Repository,
}

#[derive(Debug)]
pub struct PreFileDownloadEvent {
    inner: Event,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    processed_url: String,
    custom_cache_key: Option<String>,
    r#type: String,
    context: Option<FileDownloadContext>,
    transport_options: IndexMap<String, PhpMixed>,
}

impl PreFileDownloadEvent {
    pub fn new(
        name: String,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        processed_url: String,
        r#type: String,
        context: Option<FileDownloadContext>,
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

    pub fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    pub fn get_http_downloader(&self) -> &std::rc::Rc<std::cell::RefCell<HttpDownloader>> {
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

    pub fn get_context(&self) -> Option<&FileDownloadContext> {
        self.context.as_ref()
    }

    pub fn get_transport_options(&self) -> &IndexMap<String, PhpMixed> {
        &self.transport_options
    }

    pub fn set_transport_options(&mut self, options: IndexMap<String, PhpMixed>) {
        self.transport_options = options;
    }
}

impl EventInterface for PreFileDownloadEvent {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    fn get_arguments(&self) -> &Vec<String> {
        self.inner.get_arguments()
    }

    fn get_flags(&self) -> &IndexMap<String, PhpMixed> {
        self.inner.get_flags()
    }

    fn is_propagation_stopped(&self) -> bool {
        self.inner.is_propagation_stopped()
    }

    fn stop_propagation(&mut self) {
        self.inner.stop_propagation();
    }
}
