//! ref: composer/src/Composer/Repository/Vcs/HgDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::io::io_interface::IOInterface;
use crate::repository::vcs::vcs_driver::VcsDriverBase;
use crate::util::filesystem::Filesystem;
use crate::util::hg::Hg as HgUtils;
use crate::util::url::Url;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{RuntimeException, dirname, is_dir, is_writable};

#[derive(Debug)]
pub struct HgDriver {
    pub(crate) inner: VcsDriverBase,
    pub(crate) tags: Option<IndexMap<String, String>>,
    pub(crate) branches: Option<IndexMap<String, String>>,
    pub(crate) root_identifier: Option<String>,
    pub(crate) repo_dir: String,
}

impl HgDriver {
    pub fn initialize(&mut self) -> anyhow::Result<()> {
        if Filesystem::is_local_path(&self.inner.url) {
            self.repo_dir = self.inner.url.clone();
        } else {
            let cache_vcs_dir = self
                .inner
                .config
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or("")
                .to_string();
            if !Cache::is_usable(&cache_vcs_dir) {
                return Err(RuntimeException {
                    message: "HgDriver requires a usable cache directory, and it looks like you set it to be disabled".to_string(),
                    code: 0,
                }.into());
            }

            let sanitized =
                Preg::replace(r"{[^a-z0-9]}i", "-", Url::sanitize(self.inner.url.clone()));
            self.repo_dir = format!("{}/{}/", cache_vcs_dir, sanitized);

            let fs = Filesystem::new();
            fs.ensure_directory_exists(&cache_vcs_dir)?;

            if !is_writable(&dirname(&self.repo_dir)) {
                return Err(RuntimeException {
                    message: format!(
                        "Can not clone {} to access package information. The \"{}\" directory is not writable by the current user.",
                        self.inner.url, cache_vcs_dir
                    ),
                    code: 0,
                }.into());
            }

            self.inner
                .config
                .prohibit_url_by_config(&self.inner.url, &*self.inner.io)?;

            let hg_utils = HgUtils::new(&*self.inner.io, &self.inner.config, &self.inner.process);

            if is_dir(&self.repo_dir)
                && self.inner.process.execute(
                    &["hg", "summary"].map(|s| s.to_string()).to_vec(),
                    &mut String::new(),
                    Some(self.repo_dir.clone()),
                ) == 0
            {
                if self.inner.process.execute(
                    &["hg", "pull"].map(|s| s.to_string()).to_vec(),
                    &mut String::new(),
                    Some(self.repo_dir.clone()),
                ) != 0
                {
                    self.inner.io.write_error(
                        format!("<error>Failed to update {}, package information from this repository may be outdated ({})</error>", self.inner.url, self.inner.process.get_error_output()).into(),
                        true,
                        crate::io::io_interface::IOInterface::NORMAL,
                    );
                }
            } else {
                let fs2 = Filesystem::new();
                fs2.remove_directory(&self.repo_dir)?;

                let repo_dir = self.repo_dir.clone();
                let command = move |url: String| -> Vec<String> {
                    vec![
                        "hg".to_string(),
                        "clone".to_string(),
                        "--noupdate".to_string(),
                        "--".to_string(),
                        url,
                        repo_dir.clone(),
                    ]
                };

                hg_utils.run_command(command, self.inner.url.clone(), None)?;
            }
        }

        self.get_tags()?;
        self.get_branches()?;

        Ok(())
    }

    pub fn get_root_identifier(&mut self) -> anyhow::Result<String> {
        if self.root_identifier.is_none() {
            let mut output = String::new();
            self.inner.process.execute(
                &["hg", "tip", "--template", "{node}"]
                    .map(|s| s.to_string())
                    .to_vec(),
                &mut output,
                Some(self.repo_dir.clone()),
            );
            let lines = self.inner.process.split_lines(&output);
            self.root_identifier = lines.into_iter().next();
        }

        Ok(self.root_identifier.clone().unwrap_or_default())
    }

    pub fn get_url(&self) -> String {
        self.inner.url.clone()
    }

    pub fn get_source(&self, identifier: &str) -> IndexMap<String, String> {
        let mut map = IndexMap::new();
        map.insert("type".to_string(), "hg".to_string());
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
                    "Invalid hg identifier detected. Identifier must not start with a -, given: {}",
                    identifier
                ),
                code: 0,
            }
            .into());
        }

        let resource = vec![
            "hg".to_string(),
            "cat".to_string(),
            "-r".to_string(),
            identifier.to_string(),
            "--".to_string(),
            file.to_string(),
        ];
        let mut content = String::new();
        self.inner
            .process
            .execute(&resource, &mut content, Some(self.repo_dir.clone()));

        if content.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some(content))
    }

    pub fn get_change_date(&self, identifier: &str) -> anyhow::Result<Option<DateTime<Utc>>> {
        if identifier.starts_with('-') {
            return Err(RuntimeException {
                message: format!(
                    "Invalid hg identifier detected. Identifier must not start with a -, given: {}",
                    identifier
                ),
                code: 0,
            }
            .into());
        }

        let mut output = String::new();
        self.inner.process.execute(
            &[
                "hg",
                "log",
                "--template",
                "{date|rfc3339date}",
                "-r",
                identifier,
            ]
            .map(|s| s.to_string())
            .to_vec(),
            &mut output,
            Some(self.repo_dir.clone()),
        );

        let date = DateTime::parse_from_rfc3339(output.trim()).map(|d| d.with_timezone(&Utc))?;
        Ok(Some(date))
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        if self.tags.is_none() {
            let mut tags: IndexMap<String, String> = IndexMap::new();
            let mut output = String::new();
            self.inner.process.execute(
                &["hg", "tags"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(self.repo_dir.clone()),
            );
            for tag in self.inner.process.split_lines(&output) {
                if !tag.is_empty() {
                    if let Some(m) = Preg::match_(r"^([^\s]+)\s+\d+:(.*)$", &tag) {
                        tags.insert(
                            m.get("1").cloned().unwrap_or_default(),
                            m.get("2").cloned().unwrap_or_default(),
                        );
                    }
                }
            }
            tags.shift_remove("tip");

            self.tags = Some(tags);
        }

        Ok(self.tags.clone().unwrap_or_default())
    }

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        if self.branches.is_none() {
            let mut branches: IndexMap<String, String> = IndexMap::new();
            let mut bookmarks: IndexMap<String, String> = IndexMap::new();

            let mut output = String::new();
            self.inner.process.execute(
                &["hg", "branches"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(self.repo_dir.clone()),
            );
            for branch in self.inner.process.split_lines(&output) {
                if !branch.is_empty() {
                    if let Some(m) = Preg::match_(r"^([^\s]+)\s+\d+:([a-f0-9]+)", &branch) {
                        let name = m.get("1").cloned().unwrap_or_default();
                        if !name.starts_with('-') {
                            branches.insert(name, m.get("2").cloned().unwrap_or_default());
                        }
                    }
                }
            }

            output.clear();
            self.inner.process.execute(
                &["hg", "bookmarks"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(self.repo_dir.clone()),
            );
            for branch in self.inner.process.split_lines(&output) {
                if !branch.is_empty() {
                    if let Some(m) = Preg::match_(r"^(?:[\s*]*)([^\s]+)\s+\d+:(.*)$", &branch) {
                        let name = m.get("1").cloned().unwrap_or_default();
                        if !name.starts_with('-') {
                            bookmarks.insert(name, m.get("2").cloned().unwrap_or_default());
                        }
                    }
                }
            }

            // Branches will have preference over bookmarks
            bookmarks.extend(branches);
            self.branches = Some(bookmarks);
        }

        Ok(self.branches.clone().unwrap_or_default())
    }

    pub fn supports(io: &dyn IOInterface, config: &Config, url: &str, deep: bool) -> bool {
        if Preg::is_match(
            r"#(^(?:https?|ssh)://(?:[^@]+@)?bitbucket.org|https://(?:.*?)\.kilnhg.com)#i",
            url,
        )
        .unwrap_or(false)
        {
            return true;
        }

        if Filesystem::is_local_path(url) {
            let url = Filesystem::get_platform_path(url);
            if !is_dir(&url) {
                return false;
            }

            let process = crate::util::process_executor::ProcessExecutor::new(io);
            let mut output = String::new();
            if process.execute(
                &["hg", "summary"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(url),
            ) == 0
            {
                return true;
            }
        }

        if !deep {
            return false;
        }

        let process = crate::util::process_executor::ProcessExecutor::new(io);
        let mut ignored = String::new();
        let exit = process.execute(
            &["hg", "identify", "--", url]
                .map(|s| s.to_string())
                .to_vec(),
            &mut ignored,
            None,
        );

        exit == 0
    }
}
