//! ref: composer/src/Composer/Plugin/PostFileDownloadEvent.php

use crate::event_dispatcher::{Event, EventInterface};
use crate::package::PackageInterfaceHandle;
use crate::plugin::FileDownloadContext;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct PostFileDownloadEvent {
    inner: Event,
    file_name: Option<String>,
    checksum: Option<String>,
    url: String,
    context: Option<FileDownloadContext>,
    r#type: String,
}

impl PostFileDownloadEvent {
    pub fn new(
        name: String,
        file_name: Option<String>,
        checksum: Option<String>,
        url: String,
        r#type: String,
        context: Option<FileDownloadContext>,
    ) -> Self {
        Self {
            inner: Event::new(name, vec![], indexmap::IndexMap::new()),
            file_name,
            checksum,
            url,
            context,
            r#type,
        }
    }

    pub fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    pub fn get_file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }

    pub fn get_checksum(&self) -> Option<&str> {
        self.checksum.as_deref()
    }

    pub fn get_url(&self) -> &str {
        &self.url
    }

    pub fn get_context(&self) -> Option<&FileDownloadContext> {
        self.context.as_ref()
    }

    pub fn get_package(&self) -> Option<PackageInterfaceHandle> {
        // TODO(php-runtime): upstream raises an E_USER_DEPRECATED here ("PostFileDownloadEvent::
        // getPackage is deprecated since Composer 2.1, use getContext instead."), which Composer's
        // ErrorHandler prints as a deprecation notice; the error subsystem is not modeled.
        match self.get_context() {
            Some(FileDownloadContext::Package(package)) => Some(package.clone()),
            _ => None,
        }
    }

    pub fn get_type(&self) -> &str {
        &self.r#type
    }
}

impl EventInterface for PostFileDownloadEvent {
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

    fn get_flags(&self) -> &IndexMap<BString, PhpMixed> {
        self.inner.get_flags()
    }

    fn is_propagation_stopped(&self) -> bool {
        self.inner.is_propagation_stopped()
    }

    fn stop_propagation(&mut self) {
        self.inner.stop_propagation();
    }
}
