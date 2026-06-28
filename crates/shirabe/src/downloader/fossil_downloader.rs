//! ref: composer/src/Composer/Downloader/FossilDownloader.php

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
use crate::util::ProcessExecutor;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{PhpMixed, RuntimeException};

#[derive(Debug)]
pub struct FossilDownloader {
    inner: VcsDownloaderBase,
}

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
        command: Vec<String>,
        cwd: Option<String>,
        output: &mut String,
    ) -> anyhow::Result<()> {
        if self
            .inner
            .process
            .borrow_mut()
            .execute(&command, output, cwd.as_deref())?
            != 0
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
    ) -> anyhow::Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn do_install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.inner.config.borrow_mut().prohibit_url_by_config(
            url,
            Some(self.inner.io.clone()),
            &indexmap::IndexMap::new(),
        )?;

        let repo_file = format!("{}.fossil", path);
        let real_path = shirabe_php_shim::realpath(path);

        self.inner.io.write_error(&format!(
            "Cloning {}",
            package.get_source_reference().unwrap_or_default()
        ));

        let mut output = String::new();
        self.execute(
            vec![
                "fossil".to_string(),
                "clone".to_string(),
                "--".to_string(),
                url.to_string(),
                repo_file.clone(),
            ],
            None,
            &mut output,
        )?;
        self.execute(
            vec![
                "fossil".to_string(),
                "open".to_string(),
                "--nested".to_string(),
                "--".to_string(),
                repo_file,
            ],
            real_path.clone(),
            &mut output,
        )?;
        self.execute(
            vec![
                "fossil".to_string(),
                "update".to_string(),
                "--".to_string(),
                package
                    .get_source_reference()
                    .unwrap_or_default()
                    .to_string(),
            ],
            real_path,
            &mut output,
        )?;

        Ok(None)
    }

    async fn do_update(
        &mut self,
        _initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        self.inner.config.borrow_mut().prohibit_url_by_config(
            url,
            Some(self.inner.io.clone()),
            &indexmap::IndexMap::new(),
        )?;

        self.inner.io.write_error(&format!(
            " Updating to {}",
            target.get_source_reference().unwrap_or_default()
        ));

        if !self.has_metadata_repository(path) {
            return Err(RuntimeException {
                message: format!(
                    "The .fslckout file is missing from {}, see https://getcomposer.org/commit-deps for more information",
                    path
                ),
                code: 0,
            }.into());
        }

        let real_path = shirabe_php_shim::realpath(path);
        let mut output = String::new();
        self.execute(
            vec!["fossil".to_string(), "pull".to_string()],
            real_path.clone(),
            &mut output,
        )?;
        self.execute(
            vec![
                "fossil".to_string(),
                "up".to_string(),
                "--".to_string(),
                target
                    .get_source_reference()
                    .unwrap_or_default()
                    .to_string(),
            ],
            real_path,
            &mut output,
        )?;

        Ok(None)
    }

    fn get_commit_logs(
        &mut self,
        _from_reference: &str,
        to_reference: &str,
        path: &str,
    ) -> anyhow::Result<String> {
        let mut output = String::new();
        self.execute(
            vec![
                "fossil".to_string(),
                "timeline".to_string(),
                "-t".to_string(),
                "ci".to_string(),
                "-W".to_string(),
                "0".to_string(),
                "-n".to_string(),
                "0".to_string(),
                "before".to_string(),
                to_reference.to_string(),
            ],
            shirabe_php_shim::realpath(path),
            &mut output,
        )?;

        let mut log = String::new();
        let match_pattern = format!("/\\d\\d:\\d\\d:\\d\\d\\s+\\[{}\\]/", to_reference);

        let trimmed = output.trim().to_string();
        let lines: Vec<String> = if trimmed.is_empty() {
            vec![]
        } else {
            Preg::split(r"{\r?\n}", &trimmed)
        };

        for line in lines {
            if Preg::is_match(&match_pattern, &line) {
                break;
            }
            log.push_str(&line);
        }

        Ok(log)
    }

    fn has_metadata_repository(&self, path: &str) -> bool {
        std::path::Path::new(&format!("{}/.fslckout", path)).is_file()
            || std::path::Path::new(&format!("{}/_FOSSIL_", path)).is_file()
    }
}

impl ChangeReportInterface for FossilDownloader {
    fn get_local_changes(
        &mut self,
        _package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        if !self.has_metadata_repository(path) {
            return Ok(None);
        }

        let mut output = String::new();
        self.inner.process.borrow_mut().execute_args(
            &["fossil".to_string(), "changes".to_string()],
            &mut output,
            shirabe_php_shim::realpath(path).as_deref(),
        );

        let output = output.trim().to_string();

        Ok(if !output.is_empty() {
            Some(output)
        } else {
            None
        })
    }
}

impl VcsCapableDownloaderInterface for FossilDownloader {
    fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: String) -> Option<String> {
        self.inner.get_vcs_reference(package, &path)
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for FossilDownloader {
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
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::download(self, package, path, prev_package).await
    }

    async fn prepare(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::install(self, package, path).await
    }

    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::update(self, initial, target, path).await
    }

    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::remove(self, package, path).await
    }

    async fn cleanup(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
