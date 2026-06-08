//! ref: composer/src/Composer/Plugin/PostFileDownloadEvent.php

use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

use crate::event_dispatcher::Event;
use crate::event_dispatcher::EventInterface;

#[derive(Debug)]
pub struct PostFileDownloadEvent {
    inner: Event,
    file_name: Option<String>,
    checksum: Option<String>,
    url: String,
    context: PhpMixed,
    r#type: String,
}

impl PostFileDownloadEvent {
    pub fn new(
        name: String,
        file_name: Option<String>,
        checksum: Option<String>,
        url: String,
        r#type: String,
        context: PhpMixed,
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

    pub fn get_context(&self) -> &PhpMixed {
        &self.context
    }

    pub fn get_type(&self) -> &str {
        &self.r#type
    }
}

impl EventInterface for PostFileDownloadEvent {
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
