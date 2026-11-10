//! ref: composer/src/Composer/Plugin/PostFileDownloadEvent.php sha1:21d11fc8bbd40bf24affc6731b52fb6c2f2670e2

use crate::event_dispatcher::{Event, EventFlags, delegate_event_interface_to_inner};
use crate::package::PackageInterfaceHandle;
use crate::plugin::FileDownloadContext;
use bstr::{BStr, BString, ByteSlice as _};

#[derive(Debug)]
pub struct PostFileDownloadEvent {
    inner: Event,
    file_name: Option<std::path::PathBuf>,
    checksum: Option<BString>,
    url: BString,
    context: Option<FileDownloadContext>,
    r#type: BString,
}

impl PostFileDownloadEvent {
    pub fn new(
        name: impl Into<BString>,
        file_name: Option<std::path::PathBuf>,
        checksum: Option<BString>,
        url: BString,
        r#type: BString,
        context: Option<FileDownloadContext>,
    ) -> Self {
        Self {
            inner: Event::new(name, vec![], EventFlags::default()),
            file_name,
            checksum,
            url,
            context,
            r#type,
        }
    }

    pub fn get_name(&self) -> &bstr::BStr {
        self.inner.get_name()
    }

    pub fn get_file_name(&self) -> Option<&std::path::Path> {
        self.file_name.as_deref()
    }

    pub fn get_checksum(&self) -> Option<&BStr> {
        self.checksum.as_ref().map(|checksum| checksum.as_bstr())
    }

    pub fn get_url(&self) -> &BStr {
        self.url.as_bstr()
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

    pub fn get_type(&self) -> &BStr {
        self.r#type.as_bstr()
    }
}

delegate_event_interface_to_inner!(PostFileDownloadEvent, inner);
