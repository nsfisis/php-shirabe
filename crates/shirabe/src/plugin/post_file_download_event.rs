//! ref: composer/src/Composer/Plugin/PostFileDownloadEvent.php

use shirabe_php_shim::PhpMixed;

use crate::event_dispatcher::event::Event;

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

    // TODO(plugin): getPackage is deprecated since Composer 2.1, use getContext instead

    pub fn get_type(&self) -> &str {
        &self.r#type
    }
}
