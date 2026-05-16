//! ref: composer/src/Composer/Repository/Vcs/GitDriver.php

use chrono::TimeZone;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, RuntimeException, dirname, is_dir, is_writable, realpath,
    sys_get_temp_dir,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::io::io_interface::IOInterface;
use crate::repository::vcs::vcs_driver::VcsDriver;
use crate::util::filesystem::Filesystem;
use crate::util::git::Git as GitUtil;
use crate::util::process_executor::ProcessExecutor;
use crate::util::url::Url;

#[derive(Debug)]
pub struct GitDriver {
    pub(crate) inner: VcsDriver,
    pub(crate) tags: Option<IndexMap<String, String>>,
    pub(crate) branches: Option<IndexMap<String, String>>,
    pub(crate) root_identifier: Option<String>,
    pub(crate) repo_dir: String,
}

impl GitDriver {
    pub fn initialize(&mut self) -> anyhow::Result<()> {
        let cache_url;
        if Filesystem::is_local_path(&self.inner.url) {
            self.inner.url = Preg::replace(r"{[\\/]\.git/?$}", "", self.inner.url.clone())?;
            if !is_dir(&self.inner.url) {
                return Err(RuntimeException {
                    message: format!(
                        "Failed to read package information from {} as the path does not exist",
                        self.inner.url
                    ),
                    code: 0,
                }
                .into());
            }
            self.repo_dir = self.inner.url.clone();
            cache_url = realpath(&self.inner.url).unwrap_or_else(|| self.inner.url.clone());
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
                    message: "GitDriver requires a usable cache directory, and it looks like you set it to be disabled".to_string(),
                    code: 0,
                }
                .into());
            }

            self.repo_dir = format!(
                "{}/{}/",
                cache_vcs_dir,
                Preg::replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(self.inner.url.clone()))?
            );

            GitUtil::clean_env(&self.inner.process);

            let fs = Filesystem::new();
            fs.ensure_directory_exists(&dirname(&self.repo_dir))?;

            if !is_writable(&dirname(&self.repo_dir)) {
                return Err(RuntimeException {
                    message: format!(
                        "Can not clone {} to access package information. The \"{}\" directory is not writable by the current user.",
                        self.inner.url,
                        dirname(&self.repo_dir)
                    ),
                    code: 0,
                }
                .into());
            }

            if Preg::is_match(r"{^ssh://[^@]+@[^:]+:[^0-9]+}", &self.inner.url).unwrap_or(false) {
                return Err(InvalidArgumentException {
                    message: format!(
                        "The source URL {} is invalid, ssh URLs should have a port number after \":\".\nUse ssh://git@example.com:22/path or just git@example.com:path if you do not want to provide a password or custom port.",
                        self.inner.url
                    ),
                    code: 0,
                }
                .into());
            }

            let git_util = GitUtil::new(
                &*self.inner.io,
                &self.inner.config,
                &self.inner.process,
                &Filesystem::new(),
            );
            if !git_util.sync_mirror(&self.inner.url, &self.repo_dir)? {
                if !is_dir(&self.repo_dir) {
                    return Err(RuntimeException {
                        message: format!(
                            "Failed to clone {} to read package information from it",
                            self.inner.url
                        ),
                        code: 0,
                    }
                    .into());
                }
                self.inner.io.write_error(
                    shirabe_php_shim::PhpMixed::String(format!(
                        "<error>Failed to update {}, package information from this repository may be outdated</error>",
                        self.inner.url
                    )),
                    true,
                    IOInterface::NORMAL,
                );
            }

            cache_url = self.inner.url.clone();
        }

        self.get_tags()?;
        self.get_branches()?;

        let cache_repo_dir = self
            .inner
            .config
            .get("cache-repo-dir")
            .as_string()
            .unwrap_or("")
            .to_string();
        self.inner.cache = Some(Cache::new(
            &*self.inner.io,
            format!(
                "{}/{}",
                cache_repo_dir,
                Preg::replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(cache_url))?
            ),
        ));
        self.inner.cache.as_mut().map(|c| {
            c.set_read_only(
                self.inner
                    .config
                    .get("cache-read-only")
                    .as_bool()
                    .unwrap_or(false),
            )
        });

        Ok(())
    }

    pub fn get_root_identifier(&mut self) -> anyhow::Result<String> {
        if self.root_identifier.is_none() {
            self.root_identifier = Some("master".to_string());

            let git_util = GitUtil::new(
                &*self.inner.io,
                &self.inner.config,
                &self.inner.process,
                &Filesystem::new(),
            );
            if !Filesystem::is_local_path(&self.inner.url) {
                let default_branch =
                    git_util.get_mirror_default_branch(&self.inner.url, &self.repo_dir, false)?;
                if let Some(branch) = default_branch {
                    self.root_identifier = Some(branch.clone());
                    return Ok(branch);
                }
            }

            let mut output = String::new();
            self.inner.process.execute(
                &[
                    "git".to_string(),
                    "branch".to_string(),
                    "--no-color".to_string(),
                ],
                &mut output,
                Some(self.repo_dir.clone()),
            );
            let branches = self.inner.process.split_lines(&output);
            if !branches.contains(&"* master".to_string()) {
                for branch in &branches {
                    if !branch.is_empty() {
                        if let Some(caps) = Preg::match_strict_groups(r"{^\* +(\S+)}", branch) {
                            if let Some(name) = caps.get("1") {
                                self.root_identifier = Some(name.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(self.root_identifier.clone().unwrap_or_default())
    }

    pub fn get_url(&self) -> String {
        self.inner.url.clone()
    }

    pub fn get_source(&self, identifier: &str) -> IndexMap<String, String> {
        let mut map = IndexMap::new();
        map.insert("type".to_string(), "git".to_string());
        map.insert("url".to_string(), self.get_url());
        map.insert("reference".to_string(), identifier.to_string());
        map
    }

    pub fn get_dist(&self, _identifier: &str) -> Option<IndexMap<String, String>> {
        None
    }

    pub fn get_file_content(
        &mut self,
        file: &str,
        identifier: &str,
    ) -> anyhow::Result<Option<String>> {
        if identifier.starts_with('-') {
            return Err(RuntimeException {
                message: format!(
                    "Invalid git identifier detected. Identifier must not start with a -, given: {}",
                    identifier
                ),
                code: 0,
            }
            .into());
        }

        let mut content = String::new();
        self.inner.process.execute(
            &[
                "git".to_string(),
                "show".to_string(),
                format!("{}:{}", identifier, file),
            ],
            &mut content,
            Some(self.repo_dir.clone()),
        );

        if content.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some(content))
    }

    pub fn get_change_date(&mut self, identifier: &str) -> anyhow::Result<Option<DateTime<Utc>>> {
        if identifier.starts_with('-') {
            return Err(RuntimeException {
                message: format!(
                    "Invalid git identifier detected. Identifier must not start with a -, given: {}",
                    identifier
                ),
                code: 0,
            }
            .into());
        }

        let command = GitUtil::build_rev_list_command(
            &self.inner.process,
            &[
                "-n1".to_string(),
                "--format=%at".to_string(),
                identifier.to_string(),
            ],
        );
        let mut output = String::new();
        self.inner
            .process
            .execute(&command, &mut output, Some(self.repo_dir.clone()));

        let timestamp_str = GitUtil::parse_rev_list_output(&output, &self.inner.process);
        let timestamp: i64 = timestamp_str.trim().parse().unwrap_or(0);
        Ok(Some(Utc.timestamp_opt(timestamp, 0).unwrap()))
    }

    pub fn get_tags(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        if self.tags.is_none() {
            self.tags = Some(IndexMap::new());

            let mut output = String::new();
            self.inner.process.execute(
                &[
                    "git".to_string(),
                    "show-ref".to_string(),
                    "--tags".to_string(),
                    "--dereference".to_string(),
                ],
                &mut output,
                Some(self.repo_dir.clone()),
            );
            for tag in self.inner.process.split_lines(&output) {
                if !tag.is_empty() {
                    if let Some(caps) = Preg::match_strict_groups(
                        r"{^([a-f0-9]{40}) refs/tags/(\S+?)(\^\{\})?$}",
                        &tag,
                    ) {
                        if let (Some(hash), Some(name)) = (caps.get("1"), caps.get("2")) {
                            self.tags
                                .as_mut()
                                .unwrap()
                                .insert(name.clone(), hash.clone());
                        }
                    }
                }
            }
        }

        Ok(self.tags.clone().unwrap_or_default())
    }

    pub fn get_branches(&mut self) -> anyhow::Result<IndexMap<String, String>> {
        if self.branches.is_none() {
            let mut branches = IndexMap::new();

            let mut output = String::new();
            self.inner.process.execute(
                &[
                    "git".to_string(),
                    "branch".to_string(),
                    "--no-color".to_string(),
                    "--no-abbrev".to_string(),
                    "-v".to_string(),
                ],
                &mut output,
                Some(self.repo_dir.clone()),
            );
            for branch in self.inner.process.split_lines(&output) {
                if !branch.is_empty()
                    && !Preg::is_match(r"{^ *[^/]+/HEAD }", &branch).unwrap_or(false)
                {
                    if let Some(caps) = Preg::match_strict_groups(
                        r"{^(?:\* )? *(\S+) *([a-f0-9]+)(?: .*)?$}",
                        &branch,
                    ) {
                        if let (Some(name), Some(hash)) = (caps.get("1"), caps.get("2")) {
                            if !name.starts_with('-') {
                                branches.insert(name.clone(), hash.clone());
                            }
                        }
                    }
                }
            }

            self.branches = Some(branches);
        }

        Ok(self.branches.clone().unwrap_or_default())
    }

    pub fn supports(
        io: &dyn IOInterface,
        _config: &Config,
        url: &str,
        deep: bool,
    ) -> anyhow::Result<bool> {
        if Preg::is_match(
            r"#(^git://|\.git/?$|git(?:olite)?@|//git\.|//github.com/)#i",
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

            let process = ProcessExecutor::new(io);
            let mut output = String::new();
            if process.execute(
                &["git".to_string(), "tag".to_string()],
                &mut output,
                Some(url.clone()),
            ) == 0
            {
                return Ok(true);
            }
            GitUtil::check_for_repo_ownership_error(&process.get_error_output(), &url);
        }

        if !deep {
            return Ok(false);
        }

        let process = ProcessExecutor::new(io);
        let git_util = GitUtil::new(io, _config, &process, &Filesystem::new());
        GitUtil::clean_env(&process);

        let result = git_util.run_commands(
            &[vec![
                "git".to_string(),
                "ls-remote".to_string(),
                "--heads".to_string(),
                "--".to_string(),
                "%url%".to_string(),
            ]],
            url,
            &sys_get_temp_dir(),
        );
        match result {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}
