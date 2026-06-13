//! ref: composer/src/Composer/Repository/Vcs/HgDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::io_interface;
use crate::repository::vcs::VcsDriverBase;
use crate::util::Filesystem;
use crate::util::Hg as HgUtils;
use crate::util::Url;
use chrono::{DateTime, FixedOffset, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{DATE_RFC3339, PhpMixed, RuntimeException, dirname, is_dir, is_writable};

#[derive(Debug)]
pub struct HgDriver {
    pub(crate) inner: VcsDriverBase,
    pub(crate) tags: Option<IndexMap<String, String>>,
    pub(crate) branches: Option<IndexMap<String, String>>,
    pub(crate) root_identifier: Option<String>,
    pub(crate) repo_dir: String,
}

impl HgDriver {
    pub fn new(
        repo_config: IndexMap<String, shirabe_php_shim::PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
        process: std::rc::Rc<std::cell::RefCell<crate::util::ProcessExecutor>>,
    ) -> Self {
        Self {
            inner: VcsDriverBase::new(repo_config, io, config, http_downloader, process),
            tags: None,
            branches: None,
            root_identifier: None,
            repo_dir: String::new(),
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<()> {
        if Filesystem::is_local_path(&self.inner.url) {
            self.repo_dir = self.inner.url.clone();
        } else {
            let cache_vcs_dir = self
                .inner
                .config
                .borrow_mut()
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
                Preg::replace(r"{[^a-z0-9]}i", "-", &Url::sanitize(self.inner.url.clone()))?;
            self.repo_dir = format!("{}/{}/", cache_vcs_dir, sanitized);

            let mut fs = Filesystem::new(None);
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

            self.inner.config.borrow_mut().prohibit_url_by_config(
                &self.inner.url,
                Some(self.inner.io.clone()),
                &indexmap::IndexMap::new(),
            )?;

            let hg_utils = HgUtils::new(
                self.inner.io.clone(),
                &*self.inner.config.borrow(),
                &self.inner.process,
            );

            if is_dir(&self.repo_dir)
                && self.inner.process.borrow_mut().execute_args(
                    &["hg", "summary"].map(|s| s.to_string()).to_vec(),
                    &mut String::new(),
                    Some(self.repo_dir.clone()),
                ) == 0
            {
                if self.inner.process.borrow_mut().execute_args(
                    &["hg", "pull"].map(|s| s.to_string()).to_vec(),
                    &mut String::new(),
                    Some(self.repo_dir.clone()),
                ) != 0
                {
                    self.inner.io.write_error3(&format!("<error>Failed to update {}, package information from this repository may be outdated ({})</error>", self.inner.url, self.inner.process.borrow().get_error_output()), true, crate::io::NORMAL);
                }
            } else {
                let mut fs2 = Filesystem::new(None);
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
            self.inner.process.borrow_mut().execute_args(
                &["hg", "tip", "--template", "{node}"]
                    .map(|s| s.to_string())
                    .to_vec(),
                &mut output,
                Some(self.repo_dir.clone()),
            );
            let lines = self.inner.process.borrow().split_lines(&output);
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
        self.inner.process.borrow_mut().execute_args(
            &resource,
            &mut content,
            Some(self.repo_dir.clone()),
        );

        if content.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some(content))
    }

    pub fn get_change_date(
        &self,
        identifier: &str,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
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
        self.inner.process.borrow_mut().execute_args(
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

        let date: DateTime<Utc> = shirabe_php_shim::date_create(output.trim())?;
        Ok(Some(date.fixed_offset()))
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        if self.tags.is_none() {
            let mut tags: IndexMap<String, String> = IndexMap::new();
            let mut output = String::new();
            self.inner.process.borrow_mut().execute_args(
                &["hg", "tags"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(self.repo_dir.clone()),
            );
            for tag in self.inner.process.borrow().split_lines(&output) {
                if !tag.is_empty() {
                    let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::match3(r"^([^\s]+)\s+\d+:(.*)$", &tag, Some(&mut m)).unwrap_or(false) {
                        tags.insert(
                            m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default(),
                            m.get(&CaptureKey::ByIndex(2)).cloned().unwrap_or_default(),
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
            self.inner.process.borrow_mut().execute_args(
                &["hg", "branches"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(self.repo_dir.clone()),
            );
            for branch in self.inner.process.borrow().split_lines(&output) {
                if !branch.is_empty() {
                    let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::match3(r"^([^\s]+)\s+\d+:([a-f0-9]+)", &branch, Some(&mut m))
                        .unwrap_or(false)
                    {
                        let name = m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default();
                        if !name.starts_with('-') {
                            branches.insert(
                                name,
                                m.get(&CaptureKey::ByIndex(2)).cloned().unwrap_or_default(),
                            );
                        }
                    }
                }
            }

            output.clear();
            self.inner.process.borrow_mut().execute_args(
                &["hg", "bookmarks"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(self.repo_dir.clone()),
            );
            for branch in self.inner.process.borrow().split_lines(&output) {
                if !branch.is_empty() {
                    let mut m: IndexMap<CaptureKey, String> = IndexMap::new();
                    if Preg::match3(r"^(?:[\s*]*)([^\s]+)\s+\d+:(.*)$", &branch, Some(&mut m))
                        .unwrap_or(false)
                    {
                        let name = m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default();
                        if !name.starts_with('-') {
                            bookmarks.insert(
                                name,
                                m.get(&CaptureKey::ByIndex(2)).cloned().unwrap_or_default(),
                            );
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

    pub fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        _config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool> {
        if Preg::is_match(
            r"#(^(?:https?|ssh)://(?:[^@]+@)?bitbucket.org|https://(?:.*?)\.kilnhg.com)#i",
            url,
        )
        .unwrap_or(false)
        {
            return Ok(true);
        }

        if Filesystem::is_local_path(url) {
            let url = Filesystem::get_platform_path(url);
            if !is_dir(&url) {
                return Ok(false);
            }

            let mut process = crate::util::ProcessExecutor::new(Some(io.clone()));
            let mut output = String::new();
            if process.execute_args(
                &["hg", "summary"].map(|s| s.to_string()).to_vec(),
                &mut output,
                Some(url),
            ) == 0
            {
                return Ok(true);
            }
        }

        if !deep {
            return Ok(false);
        }

        let mut process = crate::util::ProcessExecutor::new(Some(io));
        let mut ignored = String::new();
        let exit = process.execute_args(
            &["hg", "identify", "--", url]
                .map(|s| s.to_string())
                .to_vec(),
            &mut ignored,
            (),
        );

        Ok(exit == 0)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        if let Some(cached) = self.inner.read_cached_composer(identifier)? {
            return Ok(cached);
        }

        let file_content = self.get_file_content("composer.json", identifier)?;
        let composer =
            VcsDriverBase::finish_base_composer_information(identifier, file_content, || {
                self.get_change_date(identifier)
            })?;

        self.inner.write_cached_composer(identifier, composer)
    }
}

impl crate::repository::vcs::VcsDriverInterface for HgDriver {
    fn initialize(&mut self) -> anyhow::Result<()> {
        HgDriver::initialize(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<IndexMap<String, PhpMixed>>> {
        HgDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(&mut self, file: &str, identifier: &str) -> anyhow::Result<Option<String>> {
        HgDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &str,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        HgDriver::get_change_date(self, identifier)
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<String> {
        HgDriver::get_root_identifier(self)
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        HgDriver::get_branches(self)
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        HgDriver::get_tags(self)
    }

    fn get_dist(&self, identifier: &str) -> anyhow::Result<Option<IndexMap<String, String>>> {
        Ok(HgDriver::get_dist(self, identifier))
    }

    fn get_source(&self, identifier: &str) -> anyhow::Result<IndexMap<String, String>> {
        Ok(HgDriver::get_source(self, identifier))
    }

    fn get_url(&self) -> String {
        HgDriver::get_url(self)
    }

    fn has_composer_file(&mut self, identifier: &str) -> anyhow::Result<bool> {
        match self.get_composer_information(identifier) {
            Ok(info) => Ok(info.is_some()),
            Err(e) => {
                if e.downcast_ref::<TransportException>().is_some() {
                    Ok(false)
                } else {
                    Err(e)
                }
            }
        }
    }

    fn cleanup(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool> {
        HgDriver::supports(io, config, url, deep)
    }
}
