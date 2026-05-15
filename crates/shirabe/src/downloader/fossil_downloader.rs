//! ref: composer/src/Composer/Downloader/FossilDownloader.php

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::RuntimeException;
use crate::downloader::vcs_downloader::VcsDownloader;
use crate::package::package_interface::PackageInterface;

#[derive(Debug)]
pub struct FossilDownloader {
    inner: VcsDownloader,
}

impl FossilDownloader {
    pub(crate) fn do_download(
        &self,
        _package: &dyn PackageInterface,
        _path: String,
        _url: String,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    pub(crate) fn do_install(
        &self,
        package: &dyn PackageInterface,
        path: String,
        url: String,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.config.prohibit_url_by_config(&url, &self.inner.io)?;

        let repo_file = format!("{}.fossil", path);
        let real_path = shirabe_php_shim::realpath(&path);

        self.inner.io.write_error(&format!("Cloning {}", package.get_source_reference().unwrap_or_default()));

        let mut output = String::new();
        self.execute(
            vec!["fossil".to_string(), "clone".to_string(), "--".to_string(), url, repo_file.clone()],
            None,
            &mut output,
        )?;
        self.execute(
            vec!["fossil".to_string(), "open".to_string(), "--nested".to_string(), "--".to_string(), repo_file],
            real_path.clone(),
            &mut output,
        )?;
        self.execute(
            vec!["fossil".to_string(), "update".to_string(), "--".to_string(), package.get_source_reference().unwrap_or_default()],
            real_path,
            &mut output,
        )?;

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    pub(crate) fn do_update(
        &self,
        _initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: String,
        url: String,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.inner.config.prohibit_url_by_config(&url, &self.inner.io)?;

        self.inner.io.write_error(&format!(" Updating to {}", target.get_source_reference().unwrap_or_default()));

        if !self.has_metadata_repository(&path) {
            return Err(RuntimeException {
                message: format!(
                    "The .fslckout file is missing from {}, see https://getcomposer.org/commit-deps for more information",
                    path
                ),
                code: 0,
            }.into());
        }

        let real_path = shirabe_php_shim::realpath(&path);
        let mut output = String::new();
        self.execute(
            vec!["fossil".to_string(), "pull".to_string()],
            real_path.clone(),
            &mut output,
        )?;
        self.execute(
            vec!["fossil".to_string(), "up".to_string(), "--".to_string(), target.get_source_reference().unwrap_or_default()],
            real_path,
            &mut output,
        )?;

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    pub fn get_local_changes(&self, _package: &dyn PackageInterface, path: String) -> Option<String> {
        if !self.has_metadata_repository(&path) {
            return None;
        }

        let mut output = String::new();
        self.inner.process.execute(
            &["fossil".to_string(), "changes".to_string()],
            &mut output,
            shirabe_php_shim::realpath(&path),
        );

        let output = output.trim().to_string();

        if output.len() > 0 { Some(output) } else { None }
    }

    pub(crate) fn get_commit_logs(
        &self,
        _from_reference: String,
        to_reference: String,
        path: String,
    ) -> Result<String> {
        let mut output = String::new();
        self.execute(
            vec![
                "fossil".to_string(), "timeline".to_string(),
                "-t".to_string(), "ci".to_string(),
                "-W".to_string(), "0".to_string(),
                "-n".to_string(), "0".to_string(),
                "before".to_string(), to_reference.clone(),
            ],
            shirabe_php_shim::realpath(&path),
            &mut output,
        )?;

        let mut log = String::new();
        let match_pattern = format!("/\\d\\d:\\d\\d:\\d\\d\\s+\\[{}\\]/", to_reference);

        let trimmed = output.trim().to_string();
        let lines: Vec<String> = if trimmed.is_empty() {
            vec![]
        } else {
            Preg::split(r"{\r?\n}", &trimmed)?
        };

        for line in lines {
            if Preg::is_match(&match_pattern, &line)? {
                break;
            }
            log.push_str(&line);
        }

        Ok(log)
    }

    fn execute(&self, command: Vec<String>, cwd: Option<String>, output: &mut String) -> Result<()> {
        if self.inner.process.execute(&command, output, cwd) != 0 {
            return Err(RuntimeException {
                message: format!(
                    "Failed to execute {}\n\n{}",
                    command.join(" "),
                    self.inner.process.get_error_output()
                ),
                code: 0,
            }.into());
        }
        Ok(())
    }

    pub(crate) fn has_metadata_repository(&self, path: &str) -> bool {
        std::path::Path::new(&format!("{}/.fslckout", path)).is_file()
            || std::path::Path::new(&format!("{}/_FOSSIL_", path)).is_file()
    }
}
