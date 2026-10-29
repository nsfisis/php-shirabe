//! ref: composer/src/Composer/Repository/Vcs/GitDriver.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::TransportException;
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::repository::vcs::VcsDriverBase;
use crate::util::{Filesystem, Git as GitUtil, ProcessExecutor, Url};
use bstr::{BStr, BString, ByteSlice};
use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Catch as _, InvalidArgumentException, PhpMixed, RuntimeException, bconcat, dirname, is_dir,
    is_writable, legacy_lossy, path_from_bytes, path_to_bytes, php_regex, preg_is_match,
    preg_match, preg_replace, realpath, sys_get_temp_dir, trim, utf8_text,
};

#[derive(Debug)]
pub struct GitDriver {
    pub(crate) inner: VcsDriverBase,
    pub(crate) tags: Option<IndexMap<BString, BString>>,
    pub(crate) branches: Option<IndexMap<BString, BString>>,
    pub(crate) root_identifier: Option<BString>,
    pub(crate) repo_dir: BString,
}

impl GitDriver {
    pub fn new(
        repo_config: IndexMap<BString, PhpMixed>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<crate::util::HttpDownloader>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
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
        let cache_url;
        if Filesystem::is_local_path(&self.inner.url) {
            self.inner.url = BString::from(
                &preg_replace(php_regex!(r"{[\\/]\.git/?$}"), "", &self.inner.url)[..],
            );
            if !is_dir(path_from_bytes(&self.inner.url)) {
                return Err(RuntimeException::new(format!(
                    "Failed to read package information from {} as the path does not exist",
                    self.inner.url.display_lossy()
                ))
                .into());
            }
            self.repo_dir = self.inner.url.clone();
            cache_url = realpath(path_from_bytes(&self.inner.url))
                .map(|p| BString::from(path_to_bytes(&p)))
                .unwrap_or_else(|| self.inner.url.clone());
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
                return Err(RuntimeException::new("GitDriver requires a usable cache directory, and it looks like you set it to be disabled".to_string())
                .into());
            }

            self.repo_dir = bconcat!(
                cache_vcs_dir,
                "/",
                preg_replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(&self.inner.url)),
                "/"
            );

            GitUtil::clean_env(&self.inner.process);

            let mut fs = Filesystem::new(None);
            fs.ensure_directory_exists(path_from_bytes(&dirname(&self.repo_dir)))?;

            if !is_writable(path_from_bytes(&dirname(&self.repo_dir))) {
                return Err(RuntimeException::new(format!(
                    "Can not clone {} to access package information. The \"{}\" directory is not writable by the current user.",
                    self.inner.url.display_lossy(),
                    dirname(&self.repo_dir).display_lossy()
                ))
                .into());
            }

            if preg_is_match(php_regex!(r"{^ssh://[^@]+@[^:]+:[^0-9]+}"), &self.inner.url) {
                return Err(InvalidArgumentException::new(format!(
                    "The source URL {} is invalid, ssh URLs should have a port number after \":\".\nUse ssh://git@example.com:22/path or just git@example.com:path if you do not want to provide a password or custom port.",
                    self.inner.url.display_lossy()
                ))
                .into());
            }

            let mut git_util = GitUtil::new(
                self.inner.io.clone(),
                self.inner.config.clone(),
                self.inner.process.clone(),
                std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))),
            );
            if !git_util.sync_mirror(
                &legacy_lossy(&self.inner.url),
                &legacy_lossy(&self.repo_dir),
            )? {
                if !is_dir(path_from_bytes(&self.repo_dir)) {
                    return Err(RuntimeException::new(format!(
                        "Failed to clone {} to read package information from it",
                        self.inner.url.display_lossy()
                    ))
                    .into());
                }
                self.inner.io.write_error3(&format!(
                    "<error>Failed to update {}, package information from this repository may be outdated</error>",
                    self.inner.url.display_lossy()
                ), true, io_interface::NORMAL);
            }

            cache_url = self.inner.url.clone();
        }

        self.get_tags()?;
        self.get_branches()?;

        let cache_repo_dir = BString::from(
            self.inner
                .config
                .borrow_mut()
                .get(&legacy_lossy(b"cache-repo-dir".as_slice()))
                .as_string()
                .unwrap_or_default(),
        );
        self.inner.cache = Some(Cache::new(
            self.inner.io.clone(),
            format!(
                "{}/{}",
                legacy_lossy(&cache_repo_dir),
                legacy_lossy(&preg_replace(
                    r"{[^a-z0-9.]}i",
                    "-",
                    Url::sanitize(BString::from(&cache_url[..]))
                ))
            ),
            None,
            None,
            false,
        ));
        if let Some(c) = self.inner.cache.as_mut() {
            c.set_read_only(
                self.inner
                    .config
                    .borrow_mut()
                    .get(&legacy_lossy(b"cache-read-only".as_slice()))
                    .as_bool()
                    .unwrap_or(false),
            )
        }

        Ok(())
    }

    pub fn get_root_identifier(&mut self) -> anyhow::Result<BString> {
        if self.root_identifier.is_none() {
            self.root_identifier = Some(bstr::BString::from("master".to_string()));

            let mut git_util = GitUtil::new(
                self.inner.io.clone(),
                self.inner.config.clone(),
                self.inner.process.clone(),
                std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))),
            );
            if !Filesystem::is_local_path(&self.inner.url) {
                let default_branch = git_util.get_mirror_default_branch(
                    &legacy_lossy(&self.inner.url),
                    &legacy_lossy(&self.repo_dir),
                    false,
                );
                if let Some(branch) = default_branch {
                    self.root_identifier = Some(bstr::BString::from(branch.clone()));
                    return Ok(bstr::BString::from(branch));
                }
            }

            let mut output = BString::default();
            self.inner.process.borrow_mut().execute(
                &[
                    "git".to_string(),
                    "branch".to_string(),
                    "--no-color".to_string(),
                ],
                &mut output,
                Some(path_from_bytes(&self.repo_dir)),
            )?;
            let branches = self.inner.process.borrow().split_lines(&output);
            if !branches.contains(&bstr::BString::from("* master".to_string().clone())) {
                for branch in &branches {
                    if !branch.is_empty()
                        && let Some(caps) = preg_match(php_regex!(r"{^\* +(\S+)}"), branch)
                        && let Some(name) = caps.get(1)
                    {
                        self.root_identifier = Some(BString::from(name));
                        break;
                    }
                }
            }
        }

        Ok(self.root_identifier.clone().unwrap_or_default())
    }

    pub fn get_url(&self) -> BString {
        self.inner.url.clone()
    }

    pub fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        let mut map = IndexMap::new();
        map.insert(BString::from("type"), BString::from("git"));
        map.insert(BString::from("url"), BString::from(&self.get_url()[..]));
        map.insert(BString::from("reference"), BString::from(identifier));
        map
    }

    pub fn get_dist(&self, _identifier: &str) -> Option<IndexMap<BString, BString>> {
        None
    }

    pub fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        if identifier.starts_with(b"-") {
            return Err(RuntimeException::new(format!(
                "Invalid git identifier detected. Identifier must not start with a -, given: {}",
                identifier.display_lossy()
            ))
            .into());
        }

        let mut content = BString::default();
        self.inner.process.borrow_mut().execute(
            &[
                BString::from("git"),
                BString::from("show"),
                bconcat!(identifier, ":", file),
            ],
            &mut content,
            Some(path_from_bytes(&self.repo_dir)),
        )?;

        if content.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some(content))
    }

    pub fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        if identifier.starts_with(b"-") {
            return Err(RuntimeException::new(format!(
                "Invalid git identifier detected. Identifier must not start with a -, given: {}",
                identifier.display_lossy()
            ))
            .into());
        }

        let command = GitUtil::build_rev_list_command(
            &self.inner.process,
            vec![
                bstr::BString::from("-n1".to_string()),
                bstr::BString::from("--format=%at".to_string()),
                bstr::BString::from(identifier),
            ],
        )?;
        let mut output = bstr::BString::default();
        self.inner.process.borrow_mut().execute(
            &command,
            &mut output,
            Some(path_from_bytes(&self.repo_dir)),
        )?;

        let timestamp_str = trim(GitUtil::parse_rev_list_output(
            &output,
            &self.inner.process,
        )?);
        let timestamp: i64 = utf8_text(&timestamp_str)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Ok(Some(
            Utc.timestamp_opt(timestamp, 0).unwrap().fixed_offset(),
        ))
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        if self.tags.is_none() {
            self.tags = Some(IndexMap::new());

            let mut output = BString::default();
            self.inner.process.borrow_mut().execute(
                &[
                    "git".to_string(),
                    "show-ref".to_string(),
                    "--tags".to_string(),
                    "--dereference".to_string(),
                ],
                &mut output,
                Some(path_from_bytes(&self.repo_dir)),
            )?;
            for tag in self.inner.process.borrow().split_lines(&output) {
                if !tag.is_empty()
                    && let Some(caps) = preg_match(
                        php_regex!(r"{^([a-f0-9]{40}) refs/tags/(\S+?)(\^\{\})?$}"),
                        &tag,
                    )
                    && let (Some(hash), Some(name)) = (caps.get(1), caps.get(2))
                {
                    self.tags
                        .as_mut()
                        .unwrap()
                        .insert(BString::from(name), BString::from(hash));
                }
            }
        }

        Ok(self.tags.clone().unwrap_or_default())
    }

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        if self.branches.is_none() {
            let mut branches = IndexMap::new();

            let mut output = BString::default();
            self.inner.process.borrow_mut().execute(
                &[
                    "git".to_string(),
                    "branch".to_string(),
                    "--no-color".to_string(),
                    "--no-abbrev".to_string(),
                    "-v".to_string(),
                ],
                &mut output,
                Some(path_from_bytes(&self.repo_dir)),
            )?;
            for branch in self.inner.process.borrow().split_lines(&output) {
                if !branch.is_empty()
                    && !preg_is_match(php_regex!(r"{^ *[^/]+/HEAD }"), &branch)
                    && let Some(caps) = preg_match(
                        php_regex!(r"{^(?:\* )? *(\S+) *([a-f0-9]+)(?: .*)?$}"),
                        &branch,
                    )
                    && let (Some(name), Some(hash)) = (caps.get(1), caps.get(2))
                    && !name.starts_with(b"-")
                {
                    branches.insert(BString::from(name), BString::from(hash));
                }
            }

            self.branches = Some(branches);
        }

        Ok(self.branches.clone().unwrap_or_default())
    }

    pub fn supports(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        url: &BStr,
        deep: bool,
    ) -> anyhow::Result<bool> {
        if preg_is_match(
            php_regex!(r"#(^git://|\.git/?$|git(?:olite)?@|//git\.|//github.com/)#i"),
            url,
        ) {
            return Ok(true);
        }

        if Filesystem::is_local_path(url) {
            let url = BString::from(path_to_bytes(&Filesystem::get_platform_path(url)));
            if !is_dir(path_from_bytes(&url)) {
                return Ok(false);
            }

            let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone(),
            ))));
            let mut output = BString::default();
            if process.borrow_mut().execute(
                &["git".to_string(), "tag".to_string()],
                &mut output,
                Some(std::path::Path::new(&*legacy_lossy(path_to_bytes(
                    path_from_bytes(&url),
                )))),
            )? == 0
            {
                return Ok(true);
            }
            GitUtil::check_for_repo_ownership_error(
                &legacy_lossy(&process.borrow().get_error_output()),
                &legacy_lossy(path_to_bytes(path_from_bytes(&url))),
                Some(io.clone()),
            )?;
        }

        if !deep {
            return Ok(false);
        }

        let process = std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
            io.clone(),
        ))));
        let mut git_util = GitUtil::new(
            io.clone(),
            config,
            process.clone(),
            std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(None))),
        );
        GitUtil::clean_env(&process);

        // TODO(bytes): run_commands takes the cwd as a &str.
        let tmp_dir = sys_get_temp_dir()
            .into_os_string()
            .into_string()
            .unwrap_or_else(|dir| panic!("temp dir is not valid UTF-8: {dir:?}"));
        match git_util.run_commands(
            vec![vec![
                "git".to_string(),
                "ls-remote".to_string(),
                "--heads".to_string(),
                "--".to_string(),
                "%url%".to_string(),
            ]],
            &legacy_lossy(url),
            Some(&tmp_dir),
            false,
            (),
        ) {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.is_instanceof::<RuntimeException>() {
                    Ok(false)
                } else {
                    Err(e)
                }
            }
        }
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

    /// For testing only. Mirrors GitDriverTest::setRepoDir, which uses reflection to
    /// set the private `repoDir` property.
    pub fn __set_repo_dir(&mut self, repo_dir: BString) {
        self.repo_dir = repo_dir;
    }
}

impl crate::repository::vcs::VcsDriverInterface for GitDriver {
    fn initialize(&mut self) -> anyhow::Result<()> {
        GitDriver::initialize(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        GitDriver::get_composer_information(self, identifier)
    }

    fn get_file_content(
        &mut self,
        file: &BStr,
        identifier: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        GitDriver::get_file_content(self, file, identifier)
    }

    fn get_change_date(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<DateTime<FixedOffset>>> {
        GitDriver::get_change_date(self, identifier)
    }

    fn get_root_identifier(&mut self) -> anyhow::Result<BString> {
        GitDriver::get_root_identifier(self)
    }

    fn get_branches(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        GitDriver::get_branches(self)
    }

    fn get_tags(&mut self) -> anyhow::Result<IndexMap<BString, BString>> {
        GitDriver::get_tags(self)
    }

    fn get_dist(&self, identifier: &BStr) -> Option<IndexMap<BString, BString>> {
        GitDriver::get_dist(self, &legacy_lossy(identifier))
    }

    fn get_source(&self, identifier: &BStr) -> IndexMap<BString, BString> {
        GitDriver::get_source(self, identifier)
    }

    fn get_url(&self) -> BString {
        GitDriver::get_url(self)
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
        GitDriver::supports(io, config, url, deep)
    }
}
