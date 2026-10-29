//! ref: composer/src/Composer/Repository/Vcs/HgDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::repository::vcs::VcsDriverBase;
use crate::util::{Filesystem, Hg as HgUtils, Url};
use bstr::{BStr, BString, ByteSlice};
use chrono::{DateTime, FixedOffset, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, PhpMixed, RuntimeException, bconcat, date_create, dirname, is_dir, is_writable,
    legacy_lossy, path_from_bytes, path_to_bytes, php_regex, preg_is_match, preg_match,
    preg_replace,
};

#[derive(Debug)]
pub struct HgDriver {
    inner: VcsDriverBase,
    tags: Option<IndexMap<BString, BString>>,
    branches: Option<IndexMap<BString, BString>>,
    root_identifier: Option<BString>,
    repo_dir: BString,
}

impl HgDriver {
    pub fn new(
        repo_config: IndexMap<BString, PhpMixed>,
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
            repo_dir: BString::default(),
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<()> {
        if Filesystem::is_local_path(&self.inner.url) {
            self.repo_dir = self.inner.url.clone();
        } else {
            let cache_vcs_dir = BString::from(
                &BString::from(
                    self.inner
                        .config
                        .borrow_mut()
                        .get(&legacy_lossy(b"cache-vcs-dir".as_slice()))
                        .as_string()
                        .unwrap_or_default(),
                )[..],
            );
            if !Cache::is_usable(path_from_bytes(&cache_vcs_dir)) {
                return Err(RuntimeException::new("HgDriver requires a usable cache directory, and it looks like you set it to be disabled".to_string()).into());
            }

            let sanitized = preg_replace(
                php_regex!(r"{[^a-z0-9]}i"),
                "-",
                Url::sanitize(BString::from(&self.inner.url.clone()[..])),
            );
            self.repo_dir = bconcat!(cache_vcs_dir, "/", sanitized, "/");

            let mut fs = Filesystem::new(None);
            fs.ensure_directory_exists(path_from_bytes(&cache_vcs_dir))?;

            if !is_writable(path_from_bytes(&dirname(&self.repo_dir))) {
                return Err(RuntimeException::new(format!(
                    "Can not clone {} to access package information. The \"{}\" directory is not writable by the current user.",
                    self.inner.url.display_lossy(), cache_vcs_dir.display_lossy()
                )).into());
            }

            self.inner.config.borrow_mut().prohibit_url_by_config(
                &legacy_lossy(&self.inner.url),
                Some(self.inner.io.clone()),
                &indexmap::IndexMap::new(),
            )?;

            let hg_utils = HgUtils::new(
                self.inner.io.clone(),
                self.inner.config.clone(),
                self.inner.process.clone(),
            );

            if is_dir(path_from_bytes(&self.repo_dir))
                && self.inner.process.borrow_mut().execute(
                    ["hg", "summary"].map(|s| s.to_string()).as_ref(),
                    &mut BString::default(),
                    Some(path_from_bytes(&self.repo_dir)),
                )? == 0
            {
                if self.inner.process.borrow_mut().execute(
                    ["hg", "pull"].map(|s| s.to_string()).as_ref(),
                    &mut BString::default(),
                    Some(path_from_bytes(&self.repo_dir)),
                )? != 0
                {
                    self.inner.io.write_error3(&format!("<error>Failed to update {}, package information from this repository may be outdated ({})</error>", self.inner.url.display_lossy(), self.inner.process.borrow().get_error_output().display_lossy()), true, crate::io::NORMAL);
                }
            } else {
                let mut fs2 = Filesystem::new(None);
                fs2.remove_directory(path_from_bytes(&self.repo_dir))?;

                let repo_dir = self.repo_dir.clone();
                let command = move |url: BString| -> Vec<BString> {
                    vec![
                        "hg".into(),
                        "clone".into(),
                        "--noupdate".into(),
                        "--".into(),
                        url,
                        repo_dir.clone(),
                    ]
                };

                hg_utils.run_command(command, self.inner.url.as_bstr(), None)?;
            }
        }

        self.get_tags()?;
        self.get_branches()?;

        Ok(())
    }

    pub fn get_root_identifier(&mut self) -> anyhow::Result<BString> {
        if self.root_identifier.is_none() {
            let mut output = BString::default();
            self.inner.process.borrow_mut().execute(
                ["hg", "tip", "--template", "{node}"]
                    .map(|s| s.to_string())
                    .as_ref(),
                &mut output,
                Some(path_from_bytes(&self.repo_dir)),
            )?;
            let lines = self.inner.process.borrow().split_lines(&output);
            self.root_identifier = lines.into_iter().next().map(|s| BString::from(&s[..]));
        }

        Ok(self.root_identifier.clone().unwrap_or_default())
    }

    pub fn get_url(&self) -> BString {
        self.inner.url.clone()
    }

    pub fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        let mut map = IndexMap::new();
        map.insert(BString::from("type"), BString::from("hg"));
        map.insert(BString::from("url"), BString::from(&self.get_url()[..]));
        map.insert(BString::from("reference"), BString::from(identifier));
        map
    }

    pub fn get_dist(&self, _identifier: &str) -> Option<IndexMap<BString, BString>> {
        None
    }

    pub fn get_file_content(
        &self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        if identifier.starts_with(b"-") {
            return Err(RuntimeException::new(format!(
                "Invalid hg identifier detected. Identifier must not start with a -, given: {}",
                identifier.display_lossy()
            ))
            .into());
        }

        let resource = vec![
            "hg".to_string(),
            "cat".to_string(),
            "-r".to_string(),
            legacy_lossy(&identifier).into_owned(),
            "--".to_string(),
            legacy_lossy(&file).into_owned(),
        ];
        let mut content = BString::default();
        self.inner.process.borrow_mut().execute(
            &resource,
            &mut content,
            Some(path_from_bytes(&self.repo_dir)),
        )?;

        if content.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some(content))
    }

    pub fn get_change_date(
        &self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        if identifier.starts_with(b"-") {
            return Err(RuntimeException::new(format!(
                "Invalid hg identifier detected. Identifier must not start with a -, given: {}",
                identifier.display_lossy()
            ))
            .into());
        }

        let mut output = BString::default();
        self.inner.process.borrow_mut().execute(
            [
                "hg",
                "log",
                "--template",
                "{date|rfc3339date}",
                "-r",
                &*legacy_lossy(identifier),
            ]
            .map(|s| s.to_string())
            .as_ref(),
            &mut output,
            Some(path_from_bytes(&self.repo_dir)),
        )?;

        let date: DateTime<Utc> = date_create(output.trim())?;
        Ok(Some(date.fixed_offset()))
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        if self.tags.is_none() {
            let mut tags: IndexMap<BString, BString> = IndexMap::new();
            let mut output = BString::default();
            self.inner.process.borrow_mut().execute(
                ["hg", "tags"].map(|s| s.to_string()).as_ref(),
                &mut output,
                Some(path_from_bytes(&self.repo_dir)),
            )?;
            for tag in self.inner.process.borrow().split_lines(&output) {
                if !tag.is_empty()
                    && let Some(m) = preg_match(php_regex!(r"(^([^\s]+)\s+\d+:(.*)$)"), &tag)
                {
                    tags.insert(
                        BString::from(m.get(1).unwrap_or_default()),
                        BString::from(m.get(2).unwrap_or_default()),
                    );
                }
            }
            tags.shift_remove(b"tip".as_slice());

            self.tags = Some(tags);
        }

        Ok(self.tags.clone().unwrap_or_default())
    }

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        if self.branches.is_none() {
            let mut branches: IndexMap<BString, BString> = IndexMap::new();
            let mut bookmarks: IndexMap<BString, BString> = IndexMap::new();

            let mut output = BString::default();
            self.inner.process.borrow_mut().execute(
                ["hg", "branches"].map(|s| s.to_string()).as_ref(),
                &mut output,
                Some(path_from_bytes(&self.repo_dir)),
            )?;
            for branch in self.inner.process.borrow().split_lines(&output) {
                if !branch.is_empty()
                    && let Some(m) =
                        preg_match(php_regex!(r"(^([^\s]+)\s+\d+:([a-f0-9]+))"), &branch)
                {
                    let name = BString::from(m.get(1).unwrap_or_default());
                    if !name.starts_with(b"-") {
                        branches.insert(name, BString::from(m.get(2).unwrap_or_default()));
                    }
                }
            }

            output.clear();
            self.inner.process.borrow_mut().execute(
                ["hg", "bookmarks"].map(|s| s.to_string()).as_ref(),
                &mut output,
                Some(path_from_bytes(&self.repo_dir)),
            )?;
            for branch in self.inner.process.borrow().split_lines(&output) {
                if !branch.is_empty()
                    && let Some(m) =
                        preg_match(php_regex!(r"(^(?:[\s*]*)([^\s]+)\s+\d+:(.*)$)"), &branch)
                {
                    let name = BString::from(m.get(1).unwrap_or_default());
                    if !name.starts_with(b"-") {
                        bookmarks.insert(name, BString::from(m.get(2).unwrap_or_default()));
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
        url: &BStr,
        deep: bool,
    ) -> anyhow::Result<bool> {
        if preg_is_match(
            php_regex!(
                r"#(^(?:https?|ssh)://(?:[^@]+@)?bitbucket.org|https://(?:.*?)\.kilnhg.com)#i"
            ),
            url,
        ) {
            return Ok(true);
        }

        if Filesystem::is_local_path(url) {
            let url = BString::from(path_to_bytes(&Filesystem::get_platform_path(url)));
            if !is_dir(path_from_bytes(&url)) {
                return Ok(false);
            }

            let process = crate::util::ProcessExecutor::new(Some(io.clone()));
            let mut output = BString::default();
            if process.execute(
                ["hg", "summary"].map(|s| s.to_string()).as_ref(),
                &mut output,
                Some(std::path::Path::new(&*legacy_lossy(path_to_bytes(
                    path_from_bytes(&url),
                )))),
            )? == 0
            {
                return Ok(true);
            }
        }

        if !deep {
            return Ok(false);
        }

        let process = crate::util::ProcessExecutor::new(Some(io));
        let mut ignored = BString::default();
        let exit = process.execute(
            ["hg", "identify", "--", &*legacy_lossy(url)]
                .map(|s| s.to_string())
                .as_ref(),
            &mut ignored,
            None,
        )?;

        Ok(exit == 0)
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        if let Some(cached) = self.inner.read_cached_composer(identifier)? {
            return Ok(cached);
        }

        let file_content = self.get_file_content(bstr::BStr::new("composer.json"), identifier)?;
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
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        HgDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        HgDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        HgDriver::get_change_date(self, identifier)
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<BString> {
        HgDriver::get_root_identifier(self)
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        HgDriver::get_branches(self)
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        HgDriver::get_tags(self)
    }

    fn get_dist(&self, identifier: &BStr) -> Option<IndexMap<BString, BString>> {
        HgDriver::get_dist(self, &legacy_lossy(identifier))
    }

    fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        HgDriver::get_source(self, identifier)
    }

    fn get_url(&self) -> BString {
        HgDriver::get_url(self)
    }

    fn has_composer_file(&mut self, identifier: &BStr) -> anyhow::Result<bool> {
        match self.get_composer_information(identifier) {
            Ok(info) => Ok(info.is_some()),
            Err(e) => {
                if e.is_instanceof::<TransportException>() {
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
        url: &BStr,
        deep: bool,
    ) -> anyhow::Result<bool> {
        HgDriver::supports(io, config, url, deep)
    }
}
