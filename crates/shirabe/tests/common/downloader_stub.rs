//! No-op DownloaderInterface stub for installer tests.
//!
//! PHP mocks `Composer\Downloader\DownloadManager` directly and asserts its
//! `install`/`update`/`remove` calls. The Rust DownloadManager is a concrete type
//! that dispatches to a registered DownloaderInterface, so the equivalent stub
//! lives one level down: a downloader registered under some dist type that records
//! the calls it receives and resolves to null, the same way the PHP mock returns
//! `\React\Promise\resolve(null)`.
#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use shirabe::downloader::DownloaderInterface;
use shirabe::package::PackageInterfaceHandle;
use shirabe_php_shim::PhpMixed;

/// One recorded downloader operation, capturing the package pretty-names and the
/// target path so tests can assert what the LibraryInstaller forwarded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloaderCall {
    Install {
        package: String,
        path: String,
    },
    Update {
        initial: String,
        target: String,
        path: String,
    },
    Remove {
        package: String,
        path: String,
    },
}

#[derive(Debug, Default)]
pub struct DownloaderStub {
    calls: Rc<RefCell<Vec<DownloaderCall>>>,
}

impl DownloaderStub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Shared handle to the recorded call log, so tests can inspect it after the
    /// stub has been moved into the DownloadManager.
    pub fn calls(&self) -> Rc<RefCell<Vec<DownloaderCall>>> {
        self.calls.clone()
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for DownloaderStub {
    fn get_installation_source(&self) -> String {
        "dist".to_string()
    }

    async fn download(
        &mut self,
        _package: PackageInterfaceHandle,
        _path: &str,
        _prev_package: Option<PackageInterfaceHandle>,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn prepare(
        &mut self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _path: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.calls.borrow_mut().push(DownloaderCall::Install {
            package: package.get_pretty_name(),
            path: path.to_string(),
        });
        Ok(None)
    }

    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.calls.borrow_mut().push(DownloaderCall::Update {
            initial: initial.get_pretty_name(),
            target: target.get_pretty_name(),
            path: path.to_string(),
        });
        Ok(None)
    }

    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.calls.borrow_mut().push(DownloaderCall::Remove {
            package: package.get_pretty_name(),
            path: path.to_string(),
        });
        Ok(None)
    }

    async fn cleanup(
        &mut self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _path: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }
}
