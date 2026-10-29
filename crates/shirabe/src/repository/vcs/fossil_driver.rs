//! ref: composer/src/Composer/Repository/Vcs/FossilDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::repository::vcs::VcsDriverBase;
use crate::util::{Filesystem, ProcessExecutor};
use bstr::{BStr, BString, ByteSlice};
use chrono::{DateTime, FixedOffset, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, PhpMixed, RuntimeException, bconcat, date_create, dirname, explode_with_limit,
    is_dir, is_file, is_writable, legacy_lossy, path_from_bytes, path_to_bytes, php_regex,
    preg_is_match, preg_replace, trim,
};

#[derive(Debug)]
pub struct FossilDriver {
    inner: VcsDriverBase,
    tags: Option<IndexMap<BString, BString>>,
    branches: Option<IndexMap<BString, BString>>,
    root_identifier: Option<BString>,
    repo_file: Option<BString>,
    checkout_dir: BString,
}

impl FossilDriver {
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
            repo_file: None,
            checkout_dir: BString::default(),
        }
    }

    pub fn initialize(&mut self) -> anyhow::Result<()> {
        // Make sure fossil is installed and reachable.
        self.check_fossil()?;

        // Ensure we are allowed to use this URL by config.
        self.inner.config.borrow_mut().prohibit_url_by_config(
            self.inner.url.as_bstr(),
            Some(self.inner.io.clone()),
            &indexmap::IndexMap::new(),
        )?;

        // Only if url points to a locally accessible directory, assume it's the checkout directory.
        // Otherwise, it should be something fossil can clone from.
        if Filesystem::is_local_path(&self.inner.url) && is_dir(path_from_bytes(&self.inner.url)) {
            self.checkout_dir = self.inner.url.clone();
        } else {
            let cache_repo_dir = BString::from(
                &BString::from(
                    self.inner
                        .config
                        .borrow_mut()
                        .get(&legacy_lossy(b"cache-repo-dir".as_slice()))
                        .as_string()
                        .unwrap_or_default(),
                )[..],
            );
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
            if !Cache::is_usable(path_from_bytes(&cache_repo_dir))
                || !Cache::is_usable(path_from_bytes(&cache_vcs_dir))
            {
                return Err(RuntimeException::new("FossilDriver requires a usable cache directory, and it looks like you set it to be disabled".to_string())
                .into());
            }

            let local_name = preg_replace(php_regex!(r"{[^a-z0-9]}i"), "-", &self.inner.url);
            self.repo_file = Some(bconcat!(cache_repo_dir, "/", local_name, ".fossil"));
            self.checkout_dir = bconcat!(cache_vcs_dir, "/", local_name, "/");

            self.update_local_repo()?;
        }

        self.get_tags()?;
        self.get_branches()?;

        Ok(())
    }

    fn check_fossil(&self) -> anyhow::Result<()> {
        let mut ignored_output = BString::default();
        if self.inner.process.borrow_mut().execute(
            ["fossil", "version"].map(|s| s.to_string()).as_ref(),
            &mut ignored_output,
            None,
        )? != 0
        {
            return Err(RuntimeException::new(format!(
                "fossil was not found, check that it is installed and in your PATH env.\n\n{}",
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

    fn update_local_repo(&mut self) -> anyhow::Result<()> {
        assert!(self.repo_file.is_some());

        let mut fs = Filesystem::new(None);
        fs.ensure_directory_exists(path_from_bytes(&self.checkout_dir))?;

        if !is_writable(path_from_bytes(&dirname(&self.checkout_dir))) {
            return Err(RuntimeException::new(format!(
                "Can not clone {} to access package information. The \"{}\" directory is not writable by the current user.",
                self.inner.url.display_lossy(), self.checkout_dir.display_lossy()
            ))
            .into());
        }

        let repo_file = legacy_lossy(&self.repo_file.as_ref().unwrap().clone()).into_owned();

        // update the repo if it is a valid fossil repository
        if is_file(path_from_bytes(&repo_file))
            && is_dir(path_from_bytes(&self.checkout_dir))
            && self.inner.process.borrow_mut().execute(
                ["fossil", "info"].map(|s| s.to_string()).as_ref(),
                &mut BString::default(),
                Some(path_from_bytes(&self.checkout_dir)),
            )? == 0
        {
            if self.inner.process.borrow_mut().execute(
                ["fossil", "pull"].map(|s| s.to_string()).as_ref(),
                &mut BString::default(),
                Some(path_from_bytes(&self.checkout_dir)),
            )? != 0
            {
                self.inner.io.write_error3(&format!(
                    "<error>Failed to update {}, package information from this repository may be outdated ({})</error>",
                    self.inner.url.display_lossy(),
                    self.inner.process.borrow().get_error_output().display_lossy()
                ), true, io_interface::NORMAL);
            }
        } else {
            // clean up directory and do a fresh clone into it
            fs.remove_directory(path_from_bytes(&self.checkout_dir))?;
            fs.remove(path_from_bytes(&repo_file))?;
            fs.ensure_directory_exists(path_from_bytes(&self.checkout_dir))?;

            let mut output = BString::default();
            if self.inner.process.borrow_mut().execute(
                [
                    "fossil",
                    "clone",
                    "--",
                    &*legacy_lossy(&self.inner.url),
                    &repo_file,
                ]
                .map(|s| s.to_string())
                .as_ref(),
                &mut output,
                None,
            )? != 0
            {
                let output = BString::from(&self.inner.process.borrow().get_error_output()[..]);
                return Err(RuntimeException::new(format!(
                    "Failed to clone {} to repository {}\n\n{}",
                    self.inner.url.display_lossy(),
                    repo_file,
                    output.display_lossy()
                ))
                .into());
            }

            if self.inner.process.borrow_mut().execute(
                ["fossil", "open", "--nested", "--", &repo_file]
                    .map(|s| s.to_string())
                    .as_ref(),
                &mut output,
                Some(path_from_bytes(&self.checkout_dir)),
            )? != 0
            {
                let output = BString::from(&self.inner.process.borrow().get_error_output()[..]);
                return Err(RuntimeException::new(format!(
                    "Failed to open repository {} in {}\n\n{}",
                    repo_file,
                    self.checkout_dir.display_lossy(),
                    output.display_lossy()
                ))
                .into());
            }
        }

        Ok(())
    }

    pub fn get_root_identifier(&mut self) -> BString {
        if self.root_identifier.is_none() {
            self.root_identifier = Some(bstr::BString::from("trunk".to_string()));
        }
        self.root_identifier.clone().unwrap()
    }

    pub fn get_url(&self) -> BString {
        self.inner.url.clone()
    }

    pub fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        let mut map = IndexMap::new();
        map.insert(BString::from("type"), BString::from("fossil"));
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
                "Invalid fossil identifier detected. Identifier must not start with a -, given: {}",
                identifier.display_lossy()
            ))
            .into());
        }

        let mut content = BString::default();
        self.inner.process.borrow_mut().execute(
            [
                "fossil",
                "cat",
                "-r",
                &*legacy_lossy(identifier),
                "--",
                &*legacy_lossy(file),
            ]
            .map(|s| s.to_string())
            .as_ref(),
            &mut content,
            Some(path_from_bytes(&self.checkout_dir)),
        )?;

        if content.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some(content))
    }

    pub fn get_change_date(
        &self,
        _identifier: &str,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        let mut output = BString::default();
        self.inner.process.borrow_mut().execute(
            ["fossil", "finfo", "-b", "-n", "1", "composer.json"]
                .map(|s| s.to_string())
                .as_ref(),
            &mut output,
            Some(path_from_bytes(&self.checkout_dir)),
        )?;
        let parts = explode_with_limit(" ", trim(&output), 3);
        let date = parts.get(1).cloned().unwrap_or_default();

        let date: DateTime<Utc> = date_create(date)?;
        Ok(Some(date.fixed_offset()))
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        if self.tags.is_none() {
            let mut tags: IndexMap<BString, BString> = IndexMap::new();
            let mut output = BString::default();
            self.inner.process.borrow_mut().execute(
                ["fossil", "tag", "list"].map(|s| s.to_string()).as_ref(),
                &mut output,
                Some(path_from_bytes(&self.checkout_dir)),
            )?;
            for tag in self.inner.process.borrow().split_lines(&output) {
                tags.insert(BString::from(&tag.clone()[..]), BString::from(&tag[..]));
            }
            self.tags = Some(tags);
        }
        Ok(self.tags.clone().unwrap_or_default())
    }

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        if self.branches.is_none() {
            let mut branches: IndexMap<BString, BString> = IndexMap::new();
            let mut output = BString::default();
            self.inner.process.borrow_mut().execute(
                ["fossil", "branch", "list"].map(|s| s.to_string()).as_ref(),
                &mut output,
                Some(path_from_bytes(&self.checkout_dir)),
            )?;
            for branch in self.inner.process.borrow().split_lines(&output) {
                let branch = preg_replace(php_regex!(r"/^\*/"), "", branch.trim_ascii());
                let branch = BString::from(&trim(&branch)[..]);
                branches.insert(branch.clone(), branch);
            }
            self.branches = Some(branches);
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
            php_regex!(r"#(^(?:https?|ssh)://(?:[^@]@)?(?:chiselapp\.com|fossil\.))#i"),
            url,
        ) {
            return Ok(true);
        }

        if preg_is_match(php_regex!(r"!/fossil/|\.fossil!"), url) {
            return Ok(true);
        }

        // local filesystem
        if Filesystem::is_local_path(url) {
            let url = Filesystem::get_platform_path(url);
            if !is_dir(&url) {
                return Ok(false);
            }

            let process = ProcessExecutor::new(Some(io));
            let mut output = BString::default();
            if process.execute(
                ["fossil", "info"].map(|s| s.to_string()).as_ref(),
                &mut output,
                Some(std::path::Path::new(&*legacy_lossy(path_to_bytes(&url)))),
            )? == 0
            {
                return Ok(true);
            }
        }

        Ok(false)
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
                self.get_change_date(&legacy_lossy(identifier))
            })?;

        self.inner.write_cached_composer(identifier, composer)
    }
}

impl crate::repository::vcs::VcsDriverInterface for FossilDriver {
    fn initialize(&mut self) -> anyhow::Result<()> {
        FossilDriver::initialize(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        FossilDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        FossilDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        FossilDriver::get_change_date(self, &legacy_lossy(identifier))
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<BString> {
        Ok(FossilDriver::get_root_identifier(self))
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        FossilDriver::get_branches(self)
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        FossilDriver::get_tags(self)
    }

    fn get_dist(&self, identifier: &BStr) -> Option<IndexMap<BString, BString>> {
        FossilDriver::get_dist(self, &legacy_lossy(identifier))
    }

    fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        FossilDriver::get_source(self, identifier)
    }

    fn get_url(&self) -> BString {
        FossilDriver::get_url(self)
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
        FossilDriver::supports(io, config, url, deep)
    }
}
