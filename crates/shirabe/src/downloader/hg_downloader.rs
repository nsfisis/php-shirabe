//! ref: composer/src/Composer/Downloader/HgDownloader.php

use crate::downloader::vcs_downloader::VcsDownloaderBase;
use crate::package::package_interface::PackageInterface;
use crate::util::hg::Hg as HgUtils;
use anyhow::Result;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::RuntimeException;

#[derive(Debug)]
pub struct HgDownloader {
    inner: VcsDownloaderBase,
}

impl HgDownloader {
    pub(crate) fn do_download(
        &self,
        package: &dyn PackageInterface,
        path: String,
        url: String,
        prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        if HgUtils::get_version(&self.inner.process).is_none() {
            return Err(RuntimeException {
                message: "hg was not found in your PATH, skipping source download".to_string(),
                code: 0,
            }
            .into());
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    pub(crate) fn do_install(
        &self,
        package: &dyn PackageInterface,
        path: String,
        url: String,
    ) -> Result<Box<dyn PromiseInterface>> {
        let hg_utils = HgUtils::new(&self.inner.io, &self.inner.config, &self.inner.process);

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
            package.get_source_reference().unwrap_or_default(),
        ];
        let mut ignored_output = String::new();
        if self.inner.process.execute(
            &command,
            &mut ignored_output,
            shirabe_php_shim::realpath(&path),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Failed to execute {}\n\n{}",
                    command.join(" "),
                    self.inner.process.get_error_output()
                ),
                code: 0,
            }
            .into());
        }

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    pub(crate) fn do_update(
        &self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: String,
        url: String,
    ) -> Result<Box<dyn PromiseInterface>> {
        let hg_utils = HgUtils::new(&self.inner.io, &self.inner.config, &self.inner.process);

        let ref_ = target.get_source_reference().unwrap_or_default();
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

        Ok(shirabe_external_packages::react::promise::resolve(None))
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
        self.inner.process.execute(
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
        if self
            .inner
            .process
            .execute(&command, &mut output, shirabe_php_shim::realpath(&path))
            != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Failed to execute {}\n\n{}",
                    command.join(" "),
                    self.inner.process.get_error_output()
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
