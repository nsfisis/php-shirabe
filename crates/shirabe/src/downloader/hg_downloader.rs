//! ref: composer/src/Composer/Downloader/HgDownloader.php

use crate::config::Config;
use crate::downloader::{
    ChangeReportInterface, DownloaderInterface, VcsCapableDownloaderInterface, VcsDownloader,
    VcsDownloaderBase,
};
use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::package::PackageInterfaceHandle;
use crate::util::{Filesystem, Hg as HgUtils, ProcessExecutor};
use bstr::{BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, bconcat, impl_php_class, implode, path_from_bytes, path_to_bytes,
    realpath, trim,
};
use std::path::Path;

#[derive(Debug)]
pub struct HgDownloader {
    inner: VcsDownloaderBase,
}

impl_php_class!(HgDownloader, r"Composer\Downloader\HgDownloader");

impl HgDownloader {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        fs: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    ) -> Self {
        Self {
            inner: VcsDownloaderBase::new(io, config, Some(process), Some(fs)),
        }
    }
}

impl VcsDownloader for HgDownloader {
    fn io(&self) -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
        self.inner.io.clone()
    }

    fn config(&self) -> &std::rc::Rc<std::cell::RefCell<Config>> {
        &self.inner.config
    }

    fn process(&self) -> &std::rc::Rc<std::cell::RefCell<ProcessExecutor>> {
        &self.inner.process
    }

    fn filesystem(&self) -> &std::rc::Rc<std::cell::RefCell<Filesystem>> {
        &self.inner.filesystem
    }

    fn has_cleaned_changes(&self) -> &std::cell::RefCell<IndexMap<bstr::BString, bool>> {
        &self.inner.has_cleaned_changes
    }

    async fn do_download(
        &self,
        _package: PackageInterfaceHandle,
        _path: &Path,
        _url: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        if HgUtils::get_version(&self.inner.process)?.is_none() {
            return Err(RuntimeException::new(
                "hg was not found in your PATH, skipping source download".to_string(),
            )
            .into());
        }

        Ok(None)
    }

    async fn do_install(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        url: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let hg_utils = HgUtils::new(
            self.inner.io.clone(),
            self.inner.config.clone(),
            self.inner.process.clone(),
        );

        let path_clone = BString::from(path_to_bytes(path));
        let clone_command = move |url: BString| -> Vec<BString> {
            vec![
                "hg".into(),
                "clone".into(),
                "--".into(),
                url,
                path_clone.clone(),
            ]
        };
        hg_utils.run_command(clone_command, url.as_bytes().as_bstr(), Some(path))?;

        let command: Vec<BString> = vec![
            "hg".into(),
            "up".into(),
            "--".into(),
            package.get_source_reference().unwrap_or_default(),
        ];
        let mut ignored_output = BString::default();
        if self.inner.process.borrow_mut().execute(
            &command,
            &mut ignored_output,
            realpath(path).as_deref(),
        )? != 0
        {
            return Err(RuntimeException::new(format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command).display_lossy(),
                self.inner
                    .process
                    .borrow()
                    .get_error_output()
                    .display_lossy()
            ))
            .into());
        }

        Ok(None)
    }

    async fn do_update(
        &self,
        _initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &Path,
        url: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let hg_utils = HgUtils::new(
            self.inner.io.clone(),
            self.inner.config.clone(),
            self.inner.process.clone(),
        );

        let ref_ = target.get_source_reference().unwrap_or_default();
        self.inner.io.write_error(&format!(
            " Updating to {}",
            target
                .get_source_reference()
                .unwrap_or_default()
                .display_lossy()
        ));

        if !self.has_metadata_repository(path) {
            return Err(RuntimeException::new(format!(
                "The .hg directory is missing from {}, see https://getcomposer.org/commit-deps for more information",
                path.display()
            )).into());
        }

        let pull_command =
            |url: BString| -> Vec<BString> { vec!["hg".into(), "pull".into(), "--".into(), url] };
        hg_utils.run_command(pull_command, url.as_bytes().as_bstr(), Some(path))?;

        let up_command = move |_url: BString| -> Vec<BString> {
            vec!["hg".into(), "up".into(), "--".into(), ref_.clone()]
        };
        hg_utils.run_command(up_command, url.as_bytes().as_bstr(), Some(path))?;

        Ok(None)
    }

    fn get_commit_logs(
        &self,
        from_reference: &bstr::BStr,
        to_reference: &bstr::BStr,
        path: &Path,
    ) -> anyhow::Result<bstr::BString> {
        let command: Vec<BString> = vec![
            "hg".into(),
            "log".into(),
            "-r".into(),
            bconcat!(from_reference, ":", to_reference),
            "--style".into(),
            "compact".into(),
        ];

        let mut output = BString::default();
        if self.inner.process.borrow_mut().execute(
            &command,
            &mut output,
            realpath(path).as_deref(),
        )? != 0
        {
            return Err(RuntimeException::new(format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command).display_lossy(),
                self.inner
                    .process
                    .borrow()
                    .get_error_output()
                    .display_lossy()
            ))
            .into());
        }

        Ok(output)
    }

    fn has_metadata_repository(&self, path: &Path) -> bool {
        path_from_bytes(&bconcat!(path, "/.hg")).is_dir()
    }
}

#[async_trait::async_trait(?Send)]
impl ChangeReportInterface for HgDownloader {
    async fn get_local_changes(
        &self,
        _package: PackageInterfaceHandle,
        path: &Path,
    ) -> anyhow::Result<Option<bstr::BString>> {
        if !path_from_bytes(&bconcat!(path, "/.hg")).is_dir() {
            return Ok(None);
        }

        let mut output = BString::default();
        self.inner.process.borrow_mut().execute(
            &["hg", "st"],
            &mut output,
            realpath(path).as_deref(),
        )?;

        let output = trim(&output);

        Ok(if !output.is_empty() {
            Some(output)
        } else {
            None
        })
    }
}

impl VcsCapableDownloaderInterface for HgDownloader {
    fn get_vcs_reference(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
    ) -> Option<bstr::BString> {
        self.inner.get_vcs_reference(package, path)
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for HgDownloader {
    fn as_change_report_interface(&self) -> Option<&dyn crate::downloader::ChangeReportInterface> {
        Some(self)
    }

    fn as_vcs_capable_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::VcsCapableDownloaderInterface> {
        Some(self)
    }

    fn get_installation_source(&self) -> String {
        <Self as VcsDownloader>::get_installation_source(self)
    }

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        prev_package: Option<PackageInterfaceHandle>,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::download(self, package, path, prev_package).await
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::install(self, package, path).await
    }

    async fn update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::update(self, initial, target, path).await
    }

    async fn remove(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::remove(self, package, path).await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
