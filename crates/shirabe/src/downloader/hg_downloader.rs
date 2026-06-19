//! ref: composer/src/Composer/Downloader/HgDownloader.php

use crate::config::Config;
use crate::downloader::ChangeReportInterface;
use crate::downloader::DownloaderInterface;
use crate::downloader::VcsCapableDownloaderInterface;
use crate::downloader::VcsDownloader;
use crate::downloader::VcsDownloaderBase;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::util::Filesystem;
use crate::util::Hg as HgUtils;
use crate::util::ProcessExecutor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, RuntimeException};

#[derive(Debug)]
pub struct HgDownloader {
    inner: VcsDownloaderBase,
}

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

    fn has_cleaned_changes(&self) -> &IndexMap<String, bool> {
        &self.inner.has_cleaned_changes
    }

    fn has_cleaned_changes_mut(&mut self) -> &mut IndexMap<String, bool> {
        &mut self.inner.has_cleaned_changes
    }

    async fn do_download(
        &mut self,
        _package: PackageInterfaceHandle,
        _path: &str,
        _url: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        if HgUtils::get_version(&self.inner.process).is_none() {
            return Err(RuntimeException {
                message: "hg was not found in your PATH, skipping source download".to_string(),
                code: 0,
            }
            .into());
        }

        Ok(None)
    }

    async fn do_install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> Result<Option<PhpMixed>> {
        let hg_utils = HgUtils::new(
            self.inner.io.clone(),
            self.inner.config.clone(),
            self.inner.process.clone(),
        );

        let path_clone = path.to_string();
        let clone_command = move |url: String| -> Vec<String> {
            vec![
                "hg".to_string(),
                "clone".to_string(),
                "--".to_string(),
                url,
                path_clone.clone(),
            ]
        };
        hg_utils.run_command(clone_command, url.to_string(), Some(path.to_string()));

        let command = vec![
            "hg".to_string(),
            "up".to_string(),
            "--".to_string(),
            package
                .get_source_reference()
                .unwrap_or_default()
                .to_string(),
        ];
        let mut ignored_output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &command,
            &mut ignored_output,
            shirabe_php_shim::realpath(path),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Failed to execute {}\n\n{}",
                    command.join(" "),
                    self.inner.process.borrow().get_error_output()
                ),
                code: 0,
            }
            .into());
        }

        Ok(None)
    }

    async fn do_update(
        &mut self,
        _initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> Result<Option<PhpMixed>> {
        let hg_utils = HgUtils::new(
            self.inner.io.clone(),
            self.inner.config.clone(),
            self.inner.process.clone(),
        );

        let ref_ = target
            .get_source_reference()
            .unwrap_or_default()
            .to_string();
        self.inner.io.write_error(&format!(
            " Updating to {}",
            target.get_source_reference().unwrap_or_default()
        ));

        if !self.has_metadata_repository(path) {
            return Err(RuntimeException {
                message: format!(
                    "The .hg directory is missing from {}, see https://getcomposer.org/commit-deps for more information",
                    path
                ),
                code: 0,
            }.into());
        }

        let pull_command = |url: String| -> Vec<String> {
            vec!["hg".to_string(), "pull".to_string(), "--".to_string(), url]
        };
        hg_utils.run_command(pull_command, url.to_string(), Some(path.to_string()));

        let ref_clone = ref_.clone();
        let up_command = move |_url: String| -> Vec<String> {
            vec![
                "hg".to_string(),
                "up".to_string(),
                "--".to_string(),
                ref_clone.clone(),
            ]
        };
        hg_utils.run_command(up_command, url.to_string(), Some(path.to_string()));

        Ok(None)
    }

    fn get_commit_logs(
        &mut self,
        from_reference: &str,
        to_reference: &str,
        path: &str,
    ) -> Result<String> {
        let command = vec![
            "hg".to_string(),
            "log".to_string(),
            "-r".to_string(),
            format!("{}:{}", from_reference, to_reference),
            "--style".to_string(),
            "compact".to_string(),
        ];

        let mut output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &command,
            &mut output,
            shirabe_php_shim::realpath(path),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Failed to execute {}\n\n{}",
                    command.join(" "),
                    self.inner.process.borrow().get_error_output()
                ),
                code: 0,
            }
            .into());
        }

        Ok(output)
    }

    fn has_metadata_repository(&self, path: &str) -> bool {
        std::path::Path::new(&format!("{}/.hg", path)).is_dir()
    }
}

impl ChangeReportInterface for HgDownloader {
    fn get_local_changes(
        &mut self,
        _package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<String>> {
        if !std::path::Path::new(&format!("{}/.hg", path)).is_dir() {
            return Ok(None);
        }

        let mut output = String::new();
        self.inner.process.borrow_mut().execute_args(
            &["hg".to_string(), "st".to_string()],
            &mut output,
            shirabe_php_shim::realpath(path),
        );

        let output = output.trim().to_string();

        Ok(if !output.is_empty() {
            Some(output)
        } else {
            None
        })
    }
}

impl VcsCapableDownloaderInterface for HgDownloader {
    fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: String) -> Option<String> {
        self.inner.get_vcs_reference(package, &path)
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for HgDownloader {
    fn as_change_report_interface(
        &mut self,
    ) -> Option<&mut dyn crate::downloader::ChangeReportInterface> {
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
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::download(self, package, path, prev_package).await
    }

    async fn prepare(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::install(self, package, path).await
    }

    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::update(self, initial, target, path).await
    }

    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::remove(self, package, path).await
    }

    async fn cleanup(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
