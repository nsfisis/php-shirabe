//! ref: composer/src/Composer/Downloader/HgDownloader.php

use crate::config::Config;
use crate::downloader::DownloaderInterface;
use crate::downloader::VcsDownloaderBase;
use crate::io::IOInterface;
use crate::package::PackageInterface;
use crate::util::Filesystem;
use crate::util::Hg as HgUtils;
use crate::util::ProcessExecutor;
use anyhow::Result;
use shirabe_php_shim::{PhpMixed, RuntimeException};

#[derive(Debug)]
pub struct HgDownloader {
    inner: VcsDownloaderBase,
}

impl HgDownloader {
    pub fn new(
        io: Box<dyn IOInterface>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        fs: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    ) -> Self {
        Self {
            inner: VcsDownloaderBase::new(io, config, Some(process), Some(fs)),
        }
    }

    pub(crate) async fn do_download(
        &self,
        package: &dyn PackageInterface,
        path: String,
        url: String,
        prev_package: Option<&dyn PackageInterface>,
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

    pub(crate) async fn do_install(
        &self,
        package: &dyn PackageInterface,
        path: String,
        url: String,
    ) -> Result<Option<PhpMixed>> {
        let hg_utils = HgUtils::new(
            &*self.inner.io,
            &*self.inner.config.borrow(),
            &self.inner.process,
        );

        let path_clone = path.clone();
        let clone_command = move |url: String| -> Vec<String> {
            vec![
                "hg".to_string(),
                "clone".to_string(),
                "--".to_string(),
                url,
                path_clone.clone(),
            ]
        };
        hg_utils.run_command(clone_command, url, Some(path.clone()));

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
            shirabe_php_shim::realpath(&path),
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

    pub(crate) async fn do_update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: String,
        url: String,
    ) -> Result<Option<PhpMixed>> {
        let hg_utils = HgUtils::new(
            &*self.inner.io,
            &*self.inner.config.borrow(),
            &self.inner.process,
        );

        let ref_ = target
            .get_source_reference()
            .unwrap_or_default()
            .to_string();
        self.inner.io.write_error(&format!(
            " Updating to {}",
            target.get_source_reference().unwrap_or_default()
        ));

        if !self.has_metadata_repository(path.clone()) {
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
        hg_utils.run_command(pull_command, url.clone(), Some(path.clone()));

        let ref_clone = ref_.clone();
        let up_command = move |_url: String| -> Vec<String> {
            vec![
                "hg".to_string(),
                "up".to_string(),
                "--".to_string(),
                ref_clone.clone(),
            ]
        };
        hg_utils.run_command(up_command, url, Some(path));

        Ok(None)
    }

    pub fn get_local_changes(
        &self,
        package: &dyn PackageInterface,
        path: String,
    ) -> Option<String> {
        if !std::path::Path::new(&format!("{}/.hg", path)).is_dir() {
            return None;
        }

        let mut output = String::new();
        self.inner.process.borrow_mut().execute_args(
            &["hg".to_string(), "st".to_string()],
            &mut output,
            shirabe_php_shim::realpath(&path),
        );

        let output = output.trim().to_string();

        if !output.is_empty() {
            Some(output)
        } else {
            None
        }
    }

    pub(crate) fn get_commit_logs(
        &self,
        from_reference: String,
        to_reference: String,
        path: String,
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
            shirabe_php_shim::realpath(&path),
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

    pub(crate) fn has_metadata_repository(&self, path: String) -> bool {
        std::path::Path::new(&format!("{}/.hg", path)).is_dir()
    }
}

// TODO(phase-b): wire up VcsDownloader trait properly. HgDownloader extends VcsDownloader which
// implements DownloaderInterface in PHP. Delegating each trait method to todo!() until the inner
// VcsDownloaderBase exposes the matching impl surface.
impl DownloaderInterface for HgDownloader {
    fn get_installation_source(&self) -> String {
        todo!()
    }

    async fn download(
        &self,
        _package: &dyn PackageInterface,
        _path: &str,
        _prev_package: Option<&dyn PackageInterface>,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn prepare(
        &self,
        _type: &str,
        _package: &dyn PackageInterface,
        _path: &str,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn install(
        &self,
        _package: &dyn PackageInterface,
        _path: &str,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn update(
        &self,
        _initial: &dyn PackageInterface,
        _target: &dyn PackageInterface,
        _path: &str,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn remove(
        &self,
        _package: &dyn PackageInterface,
        _path: &str,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn cleanup(
        &self,
        _type: &str,
        _package: &dyn PackageInterface,
        _path: &str,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }
}
