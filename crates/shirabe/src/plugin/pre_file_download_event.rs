//! ref: composer/src/Composer/Plugin/PreFileDownloadEvent.php sha1:5d144f4864b2e2e881588411b82f74bad32b6cd6

use crate::event_dispatcher::{Event, EventFlags, delegate_event_interface_to_inner};
use crate::package::PackageInterfaceHandle;
use crate::util::{HttpDownloader, StreamContextOptions};
use bstr::{BStr, BString, ByteSlice};

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
    processed_url: BString,
    custom_cache_key: Option<BString>,
    r#type: BString,
    context: Option<FileDownloadContext>,
    transport_options: StreamContextOptions,
}

impl PreFileDownloadEvent {
    pub fn new(
        name: impl Into<BString>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        processed_url: BString,
        r#type: BString,
        context: Option<FileDownloadContext>,
    ) -> Self {
        Self {
            inner: Event::new(name, vec![], EventFlags::default()),
            http_downloader,
            processed_url,
            custom_cache_key: None,
            r#type,
            context,
            transport_options: StreamContextOptions::new(),
        }
    }

    pub fn get_name(&self) -> &bstr::BStr {
        self.inner.get_name()
    }

    pub fn get_http_downloader(&self) -> &std::rc::Rc<std::cell::RefCell<HttpDownloader>> {
        &self.http_downloader
    }

    pub fn get_processed_url(&self) -> &BStr {
        self.processed_url.as_bstr()
    }

    pub fn set_processed_url(&mut self, processed_url: BString) {
        self.processed_url = processed_url;
    }

    pub fn get_custom_cache_key(&self) -> Option<&BStr> {
        self.custom_cache_key.as_ref().map(|key| key.as_bstr())
    }

    pub fn set_custom_cache_key(&mut self, custom_cache_key: Option<BString>) {
        self.custom_cache_key = custom_cache_key;
    }

    pub fn get_type(&self) -> &BStr {
        self.r#type.as_bstr()
    }

    pub fn get_context(&self) -> Option<&FileDownloadContext> {
        self.context.as_ref()
    }

    pub fn get_transport_options(&self) -> &StreamContextOptions {
        &self.transport_options
    }

    pub fn set_transport_options(&mut self, options: StreamContextOptions) {
        self.transport_options = options;
    }
}

delegate_event_interface_to_inner!(PreFileDownloadEvent, inner);
