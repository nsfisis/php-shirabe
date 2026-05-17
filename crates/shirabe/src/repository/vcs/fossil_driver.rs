//! ref: composer/src/Composer/Repository/Vcs/FossilDriver.php

use crate::io::io_interface;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{PhpMixed, RuntimeException, dirname, is_dir, is_file, is_writable};

use crate::cache::Cache;
use crate::config::Config;
use crate::io::io_interface::IOInterface;
use crate::repository::vcs::vcs_driver::VcsDriverBase;
use crate::util::filesystem::Filesystem;
use crate::util::process_executor::ProcessExecutor;

#[derive(Debug)]
pub struct FossilDriver {
    pub(crate) inner: VcsDriverBase,
    pub(crate) tags: Option<IndexMap<String, String>>,
    pub(crate) branches: Option<IndexMap<String, String>>,
    pub(crate) root_identifier: Option<String>,
    pub(crate) repo_file: Option<String>,
    pub(crate) checkout_dir: String,
}

impl FossilDriver {
    pub fn initialize(&mut self) -> anyhow::Result<()> {
        // Make sure fossil is installed and reachable.
        self.check_fossil()?;

        // Ensure we are allowed to use this URL by config.
        self.inner
            .config
            .prohibit_url_by_config(&self.inner.url, &*self.inner.io)?;

        // Only if url points to a locally accessible directory, assume it's the checkout directory.
        // Otherwise, it should be something fossil can clone from.
        if Filesystem::is_local_path(&self.inner.url) && is_dir(&self.inner.url) {
            self.checkout_dir = self.inner.url.clone();
        } else {
            let cache_repo_dir = self
                .inner
                .config
                .get("cache-repo-dir")
                .as_string()
                .unwrap_or("")
                .to_string();
            let cache_vcs_dir = self
                .inner
                .config
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or("")
                .to_string();
            if !Cache::is_usable(&cache_repo_dir) || !Cache::is_usable(&cache_vcs_dir) {
                return Err(RuntimeException {
                    message: "FossilDriver requires a usable cache directory, and it looks like you set it to be disabled".to_string(),
                    code: 0,
                }
                .into());
            }

            let local_name = Preg::replace(r"{[^a-z0-9]}i", "-", self.inner.url.clone());
            self.repo_file = Some(format!("{}/{}.fossil", cache_repo_dir, local_name));
            self.checkout_dir = format!("{}/{}/", cache_vcs_dir, local_name);

            self.update_local_repo()?;
        }

        self.get_tags()?;
        self.get_branches()?;

        Ok(())
    }

    pub(crate) fn check_fossil(&self) -> anyhow::Result<()> {
        let mut ignored_output = String::new();
        if self.inner.process.execute(
            &["fossil", "version"].map(|s| s.to_string()).to_vec(),
            &mut ignored_output,
            None,
        ) != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "fossil was not found, check that it is installed and in your PATH env.\n\n{}",
                    self.inner.process.get_error_output()
                ),
                code: 0,
            }
            .into());
        }
        Ok(())
    }

    pub(crate) fn update_local_repo(&mut self) -> anyhow::Result<()> {
        assert!(self.repo_file.is_some());

        let fs = Filesystem::new();
        fs.ensure_directory_exists(&self.checkout_dir)?;

        if !is_writable(&dirname(&self.checkout_dir)) {
            return Err(RuntimeException {
                message: format!(
                    "Can not clone {} to access package information. The \"{}\" directory is not writable by the current user.",
                    self.inner.url, self.checkout_dir
                ),
                code: 0,
            }
            .into());
        }

        let repo_file = self.repo_file.as_ref().unwrap().clone();

        // update the repo if it is a valid fossil repository
        if is_file(&repo_file)
            && is_dir(&self.checkout_dir)
            && self.inner.process.execute(
                &["fossil", "info"].map(|s| s.to_string()).to_vec(),
                &mut String::new(),
                Some(self.checkout_dir.clone()),
            ) == 0
        {
            if self.inner.process.execute(
                &["fossil", "pull"].map(|s| s.to_string()).to_vec(),
                &mut String::new(),
                Some(self.checkout_dir.clone()),
            ) != 0
            {
                self.inner.io.write_error(
                    PhpMixed::String(format!(
                        "<error>Failed to update {}, package information from this repository may be outdated ({})</error>",
                        self.inner.url,
                        self.inner.process.get_error_output()
                    )),
                    true,
                    io_interface::NORMAL,
                );
            }
        } else {
            // clean up directory and do a fresh clone into it
            fs.remove_directory(&self.checkout_dir)?;
            fs.remove(&repo_file)?;
            fs.ensure_directory_exists(&self.checkout_dir)?;

            let mut output = String::new();
            if self.inner.process.execute(
                &["fossil", "clone", "--", &self.inner.url, &repo_file]
                    .map(|s| s.to_string())
                    .to_vec(),
                &mut output,
                None,
            ) != 0
            {
                let output = self.inner.process.get_error_output();
                return Err(RuntimeException {
                    message: format!(
                        "Failed to clone {} to repository {}\n\n{}",
                        self.inner.url, repo_file, output
                    ),
                    code: 0,
                }
                .into());
            }

            if self.inner.process.execute(
                &["fossil", "open", "--nested", "--", &repo_file]
                    .map(|s| s.to_string())
                    .to_vec(),
                &mut output,
                Some(self.checkout_dir.clone()),
            ) != 0
            {
                let output = self.inner.process.get_error_output();
                return Err(RuntimeException {
                    message: format!(
                        "Failed to open repository {} in {}\n\n{}",
                        repo_file, self.checkout_dir, output
                    ),
                    code: 0,
                }
                .into());
            }
        }

        Ok(())
    }

    pub fn get_root_identifier(&mut self) -> String {
        if self.root_identifier.is_none() {
            self.root_identifier = Some("trunk".to_string());
        }
        self.root_identifier.clone().unwrap()
    }

    pub fn get_url(&self) -> String {
        self.inner.url.clone()
    }

    pub fn get_source(&self, identifier: &str) -> IndexMap<String, String> {
        let mut map = IndexMap::new();
        map.insert("type".to_string(), "fossil".to_string());
        map.insert("url".to_string(), self.get_url());
        map.insert("reference".to_string(), identifier.to_string());
        map
    }

    pub fn get_dist(&self, _identifier: &str) -> Option<IndexMap<String, String>> {
        None
    }

    pub fn get_file_content(&self, file: &str, identifier: &str) -> anyhow::Result<Option<String>> {
        if identifier.starts_with('-') {
            return Err(RuntimeException {
                message: format!(
                    "Invalid fossil identifier detected. Identifier must not start with a -, given: {}",
                    identifier
                ),
                code: 0,
            }
            .into());
        }

        let mut content = String::new();
        self.inner.process.execute(
            &["fossil", "cat", "-r", identifier, "--", file]
                .map(|s| s.to_string())
                .to_vec(),
            &mut content,
            Some(self.checkout_dir.clone()),
        );

        if content.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some(content))
    }

    pub fn get_change_date(&self, _identifier: &str) -> anyhow::Result<Option<DateTime<Utc>>> {
        let mut output = String::new();
        self.inner.process.execute(
            &["fossil", "finfo", "-b", "-n", "1", "composer.json"]
                .map(|s| s.to_string())
                .to_vec(),
            &mut output,
            Some(self.checkout_dir.clone()),
        );
        let parts: Vec<&str> = output.trim().splitn(3, ' ').collect();
        let date = parts.get(1).copied().unwrap_or("");

        let date = DateTime::parse_from_rfc3339(date).map(|d| d.with_timezone(&Utc))?;
        Ok(Some(date))
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        if self.tags.is_none() {
            let mut tags: IndexMap<String, String> = IndexMap::new();
            let mut output = String::new();
            self.inner.process.execute(
                &["fossil", "tag", "list"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(self.checkout_dir.clone()),
            );
            for tag in self.inner.process.split_lines(&output) {
                tags.insert(tag.clone(), tag);
            }
            self.tags = Some(tags);
        }
        Ok(self.tags.clone().unwrap_or_default())
    }

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        if self.branches.is_none() {
            let mut branches: IndexMap<String, String> = IndexMap::new();
            let mut output = String::new();
            self.inner.process.execute(
                &["fossil", "branch", "list"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(self.checkout_dir.clone()),
            );
            for branch in self.inner.process.split_lines(&output) {
                let branch = Preg::replace(r"/^\*/", "", branch.trim().to_string());
                let branch = branch.trim().to_string();
                branches.insert(branch.clone(), branch);
            }
            self.branches = Some(branches);
        }
        Ok(self.branches.clone().unwrap_or_default())
    }

    pub fn supports(io: &dyn IOInterface, config: &Config, url: &str, deep: bool) -> bool {
        if Preg::is_match(
            r"#(^(?:https?|ssh)://(?:[^@]@)?(?:chiselapp\.com|fossil\.))#i",
            url,
        )
        .unwrap_or(false)
        {
            return true;
        }

        if Preg::is_match(r"!/fossil/|\.fossil!", url).unwrap_or(false) {
            return true;
        }

        // local filesystem
        if Filesystem::is_local_path(url) {
            let url = Filesystem::get_platform_path(url);
            if !is_dir(&url) {
                return false;
            }

            let process = ProcessExecutor::new(io);
            let mut output = String::new();
            if process.execute(
                &["fossil", "info"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(url),
            ) == 0
            {
                return true;
            }
        }

        false
    }
}
