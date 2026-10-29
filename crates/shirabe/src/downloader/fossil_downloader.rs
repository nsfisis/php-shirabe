//! ref: composer/src/Composer/Downloader/FossilDownloader.php

use crate::config::Config;
use crate::downloader::{
    ChangeReportInterface, DownloaderInterface, VcsCapableDownloaderInterface, VcsDownloader,
    VcsDownloaderBase,
};
use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::package::PackageInterfaceHandle;
use crate::util::{Filesystem, ProcessExecutor};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, bconcat, impl_php_class, implode, path_from_bytes, php_regex,
    preg_is_match, preg_split, realpath, trim,
};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct FossilDownloader {
    inner: VcsDownloaderBase,
}

impl_php_class!(FossilDownloader, r"Composer\Downloader\FossilDownloader");

impl FossilDownloader {
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

    fn execute(
        &self,
        command: Vec<BString>,
        cwd: Option<&Path>,
        output: &mut BString,
    ) -> anyhow::Result<()> {
        if self
            .inner
            .process
            .borrow_mut()
            .execute(&command, output, cwd)?
            != 0
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
        Ok(())
    }
}

impl VcsDownloader for FossilDownloader {
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
        _url: &bstr::BStr,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn do_install(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        url: &bstr::BStr,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.inner.config.borrow_mut().prohibit_url_by_config(
            url,
            Some(self.inner.io.clone()),
            &indexmap::IndexMap::new(),
        )?;

        let repo_file = bconcat!(path, ".fossil");
        let real_path: Option<PathBuf> = realpath(path);

        self.inner.io.write_error(&format!(
            "Cloning {}",
            package
                .get_source_reference()
                .unwrap_or_default()
                .display_lossy()
        ));

        let mut output = BString::default();
        self.execute(
            vec![
                "fossil".into(),
                "clone".into(),
                "--".into(),
                url.into(),
                repo_file.clone(),
            ],
            None,
            &mut output,
        )?;
        self.execute(
            vec![
                "fossil".into(),
                "open".into(),
                "--nested".into(),
                "--".into(),
                repo_file,
            ],
            real_path.as_deref(),
            &mut output,
        )?;
        self.execute(
            vec![
                "fossil".into(),
                "update".into(),
                "--".into(),
                package.get_source_reference().unwrap_or_default(),
            ],
            real_path.as_deref(),
            &mut output,
        )?;

        Ok(None)
    }

    async fn do_update(
        &self,
        _initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &Path,
        url: &bstr::BStr,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.inner.config.borrow_mut().prohibit_url_by_config(
            url,
            Some(self.inner.io.clone()),
            &indexmap::IndexMap::new(),
        )?;

        self.inner.io.write_error(&format!(
            " Updating to {}",
            target
                .get_source_reference()
                .unwrap_or_default()
                .display_lossy()
        ));

        if !self.has_metadata_repository(path) {
            return Err(RuntimeException::new(format!(
                "The .fslckout file is missing from {}, see https://getcomposer.org/commit-deps for more information",
                path.display()
            )).into());
        }

        let real_path: Option<PathBuf> = realpath(path);
        let mut output = BString::default();
        self.execute(
            vec!["fossil".into(), "pull".into()],
            real_path.as_deref(),
            &mut output,
        )?;
        self.execute(
            vec![
                "fossil".into(),
                "up".into(),
                "--".into(),
                target.get_source_reference().unwrap_or_default(),
            ],
            real_path.as_deref(),
            &mut output,
        )?;

        Ok(None)
    }

    fn get_commit_logs(
        &self,
        _from_reference: &bstr::BStr,
        to_reference: &bstr::BStr,
        path: &Path,
    ) -> anyhow::Result<bstr::BString> {
        let mut output = BString::default();
        self.execute(
            vec![
                "fossil".into(),
                "timeline".into(),
                "-t".into(),
                "ci".into(),
                "-W".into(),
                "0".into(),
                "-n".into(),
                "0".into(),
                "before".into(),
                to_reference.to_owned(),
            ],
            realpath(path).as_deref(),
            &mut output,
        )?;

        let mut log = BString::default();
        let match_pattern = bconcat!("/\\d\\d:\\d\\d:\\d\\d\\s+\\[", to_reference, "\\]/");

        let trimmed = trim(&output);
        let lines: Vec<BString> = if trimmed.is_empty() {
            vec![]
        } else {
            preg_split(php_regex!(r"{\r?\n}"), &trimmed)
        };

        for line in lines {
            if preg_is_match(&match_pattern, &line) {
                break;
            }
            log.extend_from_slice(&line);
        }

        Ok(log)
    }

    fn has_metadata_repository(&self, path: &Path) -> bool {
        path_from_bytes(&bconcat!(path, "/.fslckout")).is_file()
            || path_from_bytes(&bconcat!(path, "/_FOSSIL_")).is_file()
    }
}

#[async_trait::async_trait(?Send)]
impl ChangeReportInterface for FossilDownloader {
    async fn get_local_changes(
        &self,
        _package: PackageInterfaceHandle,
        path: &Path,
    ) -> anyhow::Result<Option<bstr::BString>> {
        if !self.has_metadata_repository(path) {
            return Ok(None);
        }

        let mut output = BString::default();
        self.inner.process.borrow_mut().execute(
            &["fossil", "changes"],
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

impl VcsCapableDownloaderInterface for FossilDownloader {
    fn get_vcs_reference(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
    ) -> Option<bstr::BString> {
        self.inner.get_vcs_reference(package, path)
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for FossilDownloader {
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
