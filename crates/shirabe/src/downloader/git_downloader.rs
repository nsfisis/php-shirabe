//! ref: composer/src/Composer/Downloader/GitDownloader.php

use crate::io::io_interface;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{
    PhpMixed, RuntimeException, array_map, basename, dirname, implode, in_array, is_dir,
    preg_quote, realpath, rtrim, sprintf, strlen, strpos, substr, trim, version_compare,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::ChangeReportInterface;
use crate::downloader::DvcsDownloaderInterface;
use crate::downloader::VcsCapableDownloaderInterface;
use crate::downloader::VcsDownloader;
use crate::downloader::VcsDownloaderBase;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::util::Filesystem;
use crate::util::Git as GitUtil;
use crate::util::Platform;
use crate::util::ProcessExecutor;
use crate::util::Url;

#[derive(Debug)]
pub struct GitDownloader {
    inner: VcsDownloaderBase,
    /// @var array<string, bool>
    has_stashed_changes: IndexMap<String, bool>,
    /// @var array<string, bool>
    has_discarded_changes: IndexMap<String, bool>,
    git_util: GitUtil,
    /// @var array<int, array<string, bool>>
    cached_packages: IndexMap<i64, IndexMap<String, bool>>,
}

impl GitDownloader {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
        fs: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
    ) -> Self {
        let inner = VcsDownloaderBase::new(io, config, process, fs);
        let git_util = GitUtil::new(
            inner.io.clone(),
            inner.config.clone(),
            inner.process.clone(),
            inner.filesystem.clone(),
        );
        Self {
            inner,
            has_stashed_changes: IndexMap::new(),
            has_discarded_changes: IndexMap::new(),
            git_util,
            cached_packages: IndexMap::new(),
        }
    }

    pub fn get_unpushed_changes(
        &self,
        _package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<String>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);
        if !self.has_metadata_repository(&path) {
            return Ok(None);
        }

        let command = vec![
            "git".to_string(),
            "show-ref".to_string(),
            "--head".to_string(),
            "-d".to_string(),
        ];
        let mut output = String::new();
        if self
            .inner
            .process
            .borrow_mut()
            .execute_args(&command, &mut output, Some(path.clone()))
            != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Failed to execute {}\n\n{}",
                    implode(" ", &command),
                    self.inner.process.borrow().get_error_output(),
                ),
                code: 0,
            }
            .into());
        }

        let mut refs = trim(&output, None);
        let mut head_match: IndexMap<CaptureKey, String> = IndexMap::new();
        if !Preg::is_match_strict_groups3(r"{^([a-f0-9]+) HEAD$}mi", &refs, Some(&mut head_match))
            .unwrap_or(false)
        {
            // could not match the HEAD for some reason
            return Ok(None);
        }
        let head_ref = head_match
            .get(&CaptureKey::ByIndex(1))
            .cloned()
            .unwrap_or_default();

        let mut branches_match: IndexMap<CaptureKey, Vec<String>> = IndexMap::new();
        if !Preg::is_match_all_strict_groups3(
            &format!("{{^{} refs/heads/(.+)$}}mi", preg_quote(&head_ref, None)),
            &refs,
            Some(&mut branches_match),
        )
        .unwrap_or(false)
        {
            // not on a branch, we are either on a not-modified tag or some sort of detached head, so skip this
            return Ok(None);
        }
        let candidate_branches: Vec<String> = branches_match
            .get(&CaptureKey::ByIndex(1))
            .cloned()
            .unwrap_or_default();

        // use the first match as branch name for now
        let mut branch = candidate_branches[0].clone();
        let mut unpushed_changes: Option<String> = None;
        let mut branch_not_found_error = false;

        // do two passes, as if we find anything we want to fetch and then re-try
        for i in 0..=1 {
            let mut remote_branches: Vec<String> = vec![];

            // try to find matching branch names in remote repos
            for candidate in &candidate_branches {
                let mut m: IndexMap<CaptureKey, Vec<String>> = IndexMap::new();
                if Preg::is_match_all_strict_groups3(
                    &format!(
                        "{{^[a-f0-9]+ refs/remotes/((?:[^/]+)/{})$}}mi",
                        preg_quote(candidate, None)
                    ),
                    &refs,
                    Some(&mut m),
                )
                .unwrap_or(false)
                {
                    let matches: Vec<String> =
                        m.get(&CaptureKey::ByIndex(1)).cloned().unwrap_or_default();
                    for match_ in matches {
                        branch = candidate.clone();
                        remote_branches.push(match_);
                    }
                    break;
                }
            }

            // if it doesn't exist, then we assume it is an unpushed branch
            // this is bad as we have no reference point to do a diff so we just bail listing
            // the branch as being unpushed
            if remote_branches.is_empty() {
                unpushed_changes = Some(format!(
                    "Branch {} could not be found on any remote and appears to be unpushed",
                    branch
                ));
                branch_not_found_error = true;
            } else {
                // if first iteration found no remote branch but it has now found some, reset $unpushedChanges
                // so we get the real diff output no matter its length
                if branch_not_found_error {
                    unpushed_changes = None;
                }
                for remote_branch in &remote_branches {
                    let command = vec![
                        "git".to_string(),
                        "diff".to_string(),
                        "--name-status".to_string(),
                        format!("{}...{}", remote_branch, branch),
                        "--".to_string(),
                    ];
                    let mut output = String::new();
                    if self.inner.process.borrow_mut().execute_args(
                        &command,
                        &mut output,
                        Some(path.clone()),
                    ) != 0
                    {
                        return Err(RuntimeException {
                            message: format!(
                                "Failed to execute {}\n\n{}",
                                implode(" ", &command),
                                self.inner.process.borrow().get_error_output(),
                            ),
                            code: 0,
                        }
                        .into());
                    }

                    let output = trim(&output, None);
                    // keep the shortest diff from all remote branches we compare against
                    if unpushed_changes.is_none()
                        || strlen(&output) < strlen(unpushed_changes.as_deref().unwrap_or(""))
                    {
                        unpushed_changes = Some(output);
                    }
                }
            }

            // first pass and we found unpushed changes, fetch from all remotes to make sure we have up to date
            // remotes and then try again as outdated remotes can sometimes cause false-positives
            if unpushed_changes.is_some() && i == 0 {
                let mut output = String::new();
                self.inner.process.borrow_mut().execute_args(
                    &vec!["git".to_string(), "fetch".to_string(), "--all".to_string()],
                    &mut output,
                    Some(path.clone()),
                );

                // update list of refs after fetching
                let command = vec![
                    "git".to_string(),
                    "show-ref".to_string(),
                    "--head".to_string(),
                    "-d".to_string(),
                ];
                let mut output = String::new();
                if self.inner.process.borrow_mut().execute_args(
                    &command,
                    &mut output,
                    Some(path.clone()),
                ) != 0
                {
                    return Err(RuntimeException {
                        message: format!(
                            "Failed to execute {}\n\n{}",
                            implode(" ", &command),
                            self.inner.process.borrow().get_error_output(),
                        ),
                        code: 0,
                    }
                    .into());
                }
                refs = trim(&output, None);
            }

            // abort after first pass if we didn't find anything
            if unpushed_changes.is_none() {
                break;
            }
        }

        Ok(unpushed_changes)
    }

    /// Updates the given path to the given commit ref
    ///
    /// @throws \RuntimeException
    /// @return null|string       if a string is returned, it is the commit reference that was checked out if the original could not be found
    pub(crate) fn update_to_commit(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        reference: &str,
        pretty_version: &str,
    ) -> Result<Option<String>> {
        let force: Vec<String> = if self
            .has_discarded_changes
            .get(path)
            .copied()
            .unwrap_or(false)
            || self.has_stashed_changes.get(path).copied().unwrap_or(false)
        {
            vec!["-f".to_string()]
        } else {
            vec![]
        };

        // This uses the "--" sequence to separate branch from file parameters.
        //
        // Otherwise git tries the branch name as well as file name.
        // If the non-existent branch is actually the name of a file, the file
        // is checked out.

        let mut branch = Preg::replace(r"{(?:^dev-|(?:\.x)?-dev$)}i", "", &pretty_version)?;

        // Closure equivalent: $execute = function(array $command) use (&$output, $path) { ... };
        // Inlined below at each call site.

        let mut branches: Option<String> = None;
        {
            let mut output = String::new();
            if self.inner.process.borrow_mut().execute_args(
                &vec!["git".to_string(), "branch".to_string(), "-r".to_string()],
                &mut output,
                Some(path.to_string()),
            ) == 0
            {
                branches = Some(output);
            }
        }

        // check whether non-commitish are branches or tags, and fetch branches with the remote name
        let git_ref = reference.to_string();
        if !Preg::is_match(r"{^[a-f0-9]{40}$}", reference).unwrap_or(false)
            && branches.is_some()
            && Preg::is_match(
                &format!("{{^\\s+composer/{}$}}m", preg_quote(reference, None)),
                branches.as_deref().unwrap_or(""),
            )
            .unwrap_or(false)
        {
            let mut command1: Vec<String> = vec!["git".to_string(), "checkout".to_string()];
            command1.extend(force.clone());
            command1.extend(vec![
                "-B".to_string(),
                branch.clone(),
                format!("composer/{}", reference),
                "--".to_string(),
            ]);
            let command2 = vec![
                "git".to_string(),
                "reset".to_string(),
                "--hard".to_string(),
                format!("composer/{}", reference),
                "--".to_string(),
            ];

            let mut output = String::new();
            let ok1 = self.inner.process.borrow_mut().execute_args(
                &command1,
                &mut output,
                Some(path.to_string()),
            ) == 0;
            let ok2 = if ok1 {
                let mut output = String::new();
                self.inner.process.borrow_mut().execute_args(
                    &command2,
                    &mut output,
                    Some(path.to_string()),
                ) == 0
            } else {
                false
            };
            if ok1 && ok2 {
                return Ok(None);
            }
        }

        // try to checkout branch by name and then reset it so it's on the proper branch name
        if Preg::is_match(r"{^[a-f0-9]{40}$}", reference).unwrap_or(false) {
            // add 'v' in front of the branch if it was stripped when generating the pretty name
            if branches.is_some()
                && !Preg::is_match(
                    &format!("{{^\\s+composer/{}$}}m", preg_quote(&branch, None)),
                    branches.as_deref().unwrap_or(""),
                )
                .unwrap_or(false)
                && Preg::is_match(
                    &format!("{{^\\s+composer/v{}$}}m", preg_quote(&branch, None)),
                    branches.as_deref().unwrap_or(""),
                )
                .unwrap_or(false)
            {
                branch = format!("v{}", branch);
            }

            let command = vec![
                "git".to_string(),
                "checkout".to_string(),
                branch.clone(),
                "--".to_string(),
            ];
            let mut fallback_command: Vec<String> = vec!["git".to_string(), "checkout".to_string()];
            fallback_command.extend(force.clone());
            fallback_command.extend(vec![
                "-B".to_string(),
                branch.clone(),
                format!("composer/{}", branch),
                "--".to_string(),
            ]);
            let reset_command = vec![
                "git".to_string(),
                "reset".to_string(),
                "--hard".to_string(),
                reference.to_string(),
                "--".to_string(),
            ];

            let mut output = String::new();
            let ok_command = self.inner.process.borrow_mut().execute_args(
                &command,
                &mut output,
                Some(path.to_string()),
            ) == 0;
            let ok_fallback = if !ok_command {
                let mut output = String::new();
                self.inner.process.borrow_mut().execute_args(
                    &fallback_command,
                    &mut output,
                    Some(path.to_string()),
                ) == 0
            } else {
                false
            };
            let ok_reset = if ok_command || ok_fallback {
                let mut output = String::new();
                self.inner.process.borrow_mut().execute_args(
                    &reset_command,
                    &mut output,
                    Some(path.to_string()),
                ) == 0
            } else {
                false
            };
            if (ok_command || ok_fallback) && ok_reset {
                return Ok(None);
            }
        }

        let mut command1: Vec<String> = vec!["git".to_string(), "checkout".to_string()];
        command1.extend(force.clone());
        command1.extend(vec![git_ref.clone(), "--".to_string()]);
        let command2 = vec![
            "git".to_string(),
            "reset".to_string(),
            "--hard".to_string(),
            git_ref.clone(),
            "--".to_string(),
        ];
        {
            let mut output = String::new();
            let ok1 = self.inner.process.borrow_mut().execute_args(
                &command1,
                &mut output,
                Some(path.to_string()),
            ) == 0;
            let ok2 = if ok1 {
                let mut output = String::new();
                self.inner.process.borrow_mut().execute_args(
                    &command2,
                    &mut output,
                    Some(path.to_string()),
                ) == 0
            } else {
                false
            };
            if ok1 && ok2 {
                return Ok(None);
            }
        }

        let mut exception_extra = String::new();

        // reference was not found (prints "fatal: reference is not a tree: $ref")
        if strpos(self.inner.process.borrow().get_error_output(), reference).is_some() {
            self.inner.io.write_error3(
                &format!(
                    "    <warning>{} is gone (history was rewritten?)</warning>",
                    reference
                ),
                true,
                io_interface::NORMAL,
            );
            exception_extra = format!(
                "\nIt looks like the commit hash is not available in the repository, maybe {}? Run \"composer update {}\" to resolve this.",
                if package.is_dev() {
                    "the commit was removed from the branch"
                } else {
                    "the tag was recreated"
                },
                package.get_pretty_name(),
            );
        }

        let command = format!("{} && {}", implode(" ", &command1), implode(" ", &command2));

        Err(RuntimeException {
            message: Url::sanitize(format!(
                "Failed to execute {}\n\n{}{}",
                command,
                self.inner.process.borrow().get_error_output(),
                exception_extra,
            )),
            code: 0,
        }
        .into())
    }

    pub(crate) fn update_origin_url(&mut self, path: &str, url: &str) {
        let mut output = String::new();
        self.inner.process.borrow_mut().execute_args(
            &vec![
                "git".to_string(),
                "remote".to_string(),
                "set-url".to_string(),
                "origin".to_string(),
                "--".to_string(),
                url.to_string(),
            ],
            &mut output,
            Some(path.to_string()),
        );
        self.set_push_url(path, url);
    }

    pub(crate) fn set_push_url(&mut self, path: &str, url: &str) {
        // set push url for github projects
        let mut match_: IndexMap<CaptureKey, String> = IndexMap::new();
        if Preg::is_match3(
            &format!(
                "{{^(?:https?|git)://{}/([^/]+)/([^/]+?)(?:\\.git)?$}}",
                GitUtil::get_github_domains_regex(&*self.inner.config.borrow())
            ),
            url,
            Some(&mut match_),
        )
        .unwrap_or(false)
        {
            let protocols = self.inner.config.borrow_mut().get("github-protocols");
            let m1 = match_
                .get(&CaptureKey::ByIndex(1))
                .cloned()
                .unwrap_or_default();
            let m2 = match_
                .get(&CaptureKey::ByIndex(2))
                .cloned()
                .unwrap_or_default();
            let m3 = match_
                .get(&CaptureKey::ByIndex(3))
                .cloned()
                .unwrap_or_default();
            let mut push_url = format!("git@{}:{}/{}.git", m1, m2, m3);
            if !in_array(PhpMixed::String("ssh".to_string()), &protocols, true) {
                push_url = format!("https://{}/{}/{}.git", m1, m2, m3);
            }
            let cmd = vec![
                "git".to_string(),
                "remote".to_string(),
                "set-url".to_string(),
                "--push".to_string(),
                "origin".to_string(),
                "--".to_string(),
                push_url,
            ];
            let mut ignored_output = String::new();
            self.inner.process.borrow_mut().execute_args(
                &cmd,
                &mut ignored_output,
                Some(path.to_string()),
            );
        }
    }

    /// @phpstan-return PromiseInterface<void|null>
    /// @throws \RuntimeException
    pub(crate) async fn discard_changes(&mut self, path: &str) -> Result<Option<PhpMixed>> {
        let path = self.normalize_path(path);
        let mut output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &vec!["git".to_string(), "clean".to_string(), "-df".to_string()],
            &mut output,
            Some(path.clone()),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!("Could not reset changes\n\n:{}", output),
                code: 0,
            }
            .into());
        }
        let mut output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &vec!["git".to_string(), "reset".to_string(), "--hard".to_string()],
            &mut output,
            Some(path.clone()),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!("Could not reset changes\n\n:{}", output),
                code: 0,
            }
            .into());
        }

        self.has_discarded_changes.insert(path, true);

        Ok(None)
    }

    /// @phpstan-return PromiseInterface<void|null>
    /// @throws \RuntimeException
    pub(crate) async fn stash_changes(&mut self, path: &str) -> Result<Option<PhpMixed>> {
        let path = self.normalize_path(path);
        let mut output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &vec![
                "git".to_string(),
                "stash".to_string(),
                "--include-untracked".to_string(),
            ],
            &mut output,
            Some(path.clone()),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!("Could not stash changes\n\n:{}", output),
                code: 0,
            }
            .into());
        }

        self.has_stashed_changes.insert(path, true);

        Ok(None)
    }

    /// @throws \RuntimeException
    pub(crate) fn view_diff(&mut self, path: &str) -> Result<()> {
        let path = self.normalize_path(path);
        let mut output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &vec!["git".to_string(), "diff".to_string(), "HEAD".to_string()],
            &mut output,
            Some(path.clone()),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!("Could not view diff\n\n:{}", output),
                code: 0,
            }
            .into());
        }

        self.inner
            .io
            .write_error3(&output, true, io_interface::NORMAL);

        Ok(())
    }

    pub(crate) fn normalize_path(&self, path: &str) -> String {
        let mut path = path.to_string();
        if Platform::is_windows() && strlen(&path) > 0 {
            let mut base_path = path.clone();
            let mut removed: Vec<String> = vec![];

            while !is_dir(&base_path) && base_path != "\\" {
                let mut new_removed = vec![basename(&base_path)];
                new_removed.extend(removed);
                removed = new_removed;
                base_path = dirname(&base_path);
            }

            if base_path == "\\" {
                return path;
            }

            path = rtrim(
                &format!(
                    "{}/{}",
                    realpath(&base_path).unwrap_or_default(),
                    implode("/", &removed),
                ),
                Some("/"),
            );
        }

        path
    }

    pub(crate) fn get_short_hash(&self, reference: &str) -> String {
        if !self.inner.io.is_verbose()
            && Preg::is_match(r"{^[0-9a-f]{40}$}", reference).unwrap_or(false)
        {
            return substr(reference, 0, Some(10));
        }

        reference.to_string()
    }

    /// The default `VcsDownloader::clean_changes()` behavior: fail if the working copy has
    /// local changes.
    fn fail_on_local_changes(&mut self, package: PackageInterfaceHandle, path: &str) -> Result<()> {
        if self.get_local_changes(package, path)?.is_some() {
            return Err(RuntimeException {
                message: format!("Source directory {} has uncommitted changes.", path),
                code: 0,
            }
            .into());
        }

        Ok(())
    }
}

impl DvcsDownloaderInterface for GitDownloader {
    fn get_unpushed_changes(
        &self,
        package: PackageInterfaceHandle,
        path: String,
    ) -> Result<Option<String>> {
        GitDownloader::get_unpushed_changes(self, package, &path)
    }
}

impl ChangeReportInterface for GitDownloader {
    fn get_local_changes(
        &mut self,
        _package: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<String>> {
        GitUtil::clean_env(&self.inner.process);
        if !self.has_metadata_repository(path) {
            return Ok(None);
        }

        let command = vec![
            "git".to_string(),
            "status".to_string(),
            "--porcelain".to_string(),
            "--untracked-files=no".to_string(),
        ];
        let mut output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &command,
            &mut output,
            Some(path.to_string()),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Failed to execute {}\n\n{}",
                    implode(" ", &command),
                    self.inner.process.borrow().get_error_output(),
                ),
                code: 0,
            }
            .into());
        }

        let output = trim(&output, None);

        Ok(if strlen(&output) > 0 {
            Some(output)
        } else {
            None
        })
    }
}

impl VcsCapableDownloaderInterface for GitDownloader {
    fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: String) -> Option<String> {
        self.inner.get_vcs_reference(package, &path)
    }
}

impl VcsDownloader for GitDownloader {
    fn io(&self) -> std::rc::Rc<std::cell::RefCell<dyn IOInterface>> {
        self.inner.io.clone()
    }

    fn config(&self) -> &std::rc::Rc<std::cell::RefCell<Config>> {
        &self.inner.config
    }

    fn process(&self) -> &std::rc::Rc<std::cell::RefCell<ProcessExecutor>> {
        &self.inner.process
    }

    fn filesystem(&self) -> &std::rc::Rc<std::cell::RefCell<Filesystem>> {
        &self.inner.filesystem
    }

    fn has_cleaned_changes(&self) -> &IndexMap<String, bool> {
        &self.inner.has_cleaned_changes
    }

    fn has_cleaned_changes_mut(&mut self) -> &mut IndexMap<String, bool> {
        &mut self.inner.has_cleaned_changes
    }

    async fn do_download(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        url: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        // Do not create an extra local cache when repository is already local
        if Filesystem::is_local_path(url) {
            return Ok(None);
        }

        GitUtil::clean_env(&self.inner.process);

        let cache_path = format!(
            "{}/{}/",
            self.inner
                .config
                .borrow_mut()
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or(""),
            Preg::replace(r"{[^a-z0-9.]}i", "-", &Url::sanitize(url.to_string()))?,
        );
        let git_version = GitUtil::get_version(&self.inner.process);

        // --dissociate option is only available since git 2.3.0-rc0
        if git_version.is_some()
            && version_compare(git_version.as_deref().unwrap_or(""), "2.3.0-rc0", ">=")
            && Cache::is_usable(&cache_path)
        {
            self.inner.io.write_error3(
                &format!(
                    "  - Syncing <info>{}</info> (<comment>{}</comment>) into cache",
                    package.get_name(),
                    package
                        .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev),
                ),
                true,
                io_interface::NORMAL,
            );
            self.inner.io.write_error3(
                &sprintf(
                    "    Cloning to cache at %s",
                    &[PhpMixed::String(cache_path.clone())],
                ),
                true,
                io_interface::DEBUG,
            );
            let r#ref = package.get_source_reference();
            let pretty_version = package.get_pretty_version();
            if self.git_util.fetch_ref_or_sync_mirror(
                url,
                &cache_path,
                r#ref.as_deref().unwrap_or(""),
                Some(&pretty_version),
            )? && is_dir(&cache_path)
            {
                self.cached_packages
                    .entry(package.get_id())
                    .or_insert_with(IndexMap::new)
                    .insert(r#ref.as_deref().unwrap_or("").to_string(), true);
            }
        } else if git_version.is_none() {
            return Err(RuntimeException {
                message: "git was not found in your PATH, skipping source download".to_string(),
                code: 0,
            }
            .into());
        }

        Ok(None)
    }

    async fn do_install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> Result<Option<PhpMixed>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);
        let cache_path = format!(
            "{}/{}/",
            self.inner
                .config
                .borrow_mut()
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or(""),
            Preg::replace(r"{[^a-z0-9.]}i", "-", &Url::sanitize(url.to_string()))?,
        );
        let r#ref = package.get_source_reference().unwrap_or_default();

        let msg;
        let commands: Vec<Vec<String>>;
        let has_cached = self
            .cached_packages
            .get(&package.get_id())
            .and_then(|m| m.get(&r#ref))
            .copied()
            .unwrap_or(false);
        if has_cached {
            msg = format!("Cloning {} from cache", self.get_short_hash(&r#ref));

            let mut clone_flags: Vec<String> = vec![
                "--dissociate".to_string(),
                "--reference".to_string(),
                cache_path.clone(),
            ];
            let transport_options = package.get_transport_options();
            if let Some(git_opts) = transport_options.get("git").and_then(|v| v.as_array()) {
                if let Some(single) = git_opts.get("single_use_clone").and_then(|v| v.as_bool()) {
                    if single {
                        clone_flags = vec![];
                    }
                }
            }

            commands = vec![
                {
                    let mut base = vec![
                        "git".to_string(),
                        "clone".to_string(),
                        "--no-checkout".to_string(),
                        cache_path.clone(),
                        path.clone(),
                    ];
                    base.extend(clone_flags);
                    base
                },
                vec![
                    "git".to_string(),
                    "remote".to_string(),
                    "set-url".to_string(),
                    "origin".to_string(),
                    "--".to_string(),
                    "%sanitizedUrl%".to_string(),
                ],
                vec![
                    "git".to_string(),
                    "remote".to_string(),
                    "add".to_string(),
                    "composer".to_string(),
                    "--".to_string(),
                    "%sanitizedUrl%".to_string(),
                ],
            ];
        } else {
            msg = format!("Cloning {}", self.get_short_hash(&r#ref));
            commands = vec![
                vec![
                    "git".to_string(),
                    "clone".to_string(),
                    "--no-checkout".to_string(),
                    "--".to_string(),
                    "%url%".to_string(),
                    path.clone(),
                ],
                vec![
                    "git".to_string(),
                    "remote".to_string(),
                    "add".to_string(),
                    "composer".to_string(),
                    "--".to_string(),
                    "%url%".to_string(),
                ],
                vec![
                    "git".to_string(),
                    "fetch".to_string(),
                    "composer".to_string(),
                ],
                vec![
                    "git".to_string(),
                    "remote".to_string(),
                    "set-url".to_string(),
                    "origin".to_string(),
                    "--".to_string(),
                    "%sanitizedUrl%".to_string(),
                ],
                vec![
                    "git".to_string(),
                    "remote".to_string(),
                    "set-url".to_string(),
                    "composer".to_string(),
                    "--".to_string(),
                    "%sanitizedUrl%".to_string(),
                ],
            ];
            if Platform::get_env("COMPOSER_DISABLE_NETWORK").is_some() {
                return Err(RuntimeException {
                    message: format!(
                        "The required git reference for {} is not in cache and network is disabled, aborting",
                        package.get_name(),
                    ),
                    code: 0,
                }
                .into());
            }
        }

        self.inner.io.write_error3(&msg, true, io_interface::NORMAL);

        self.git_util
            .run_commands(commands, url, Some(&path), true, None)?;

        let source_url = package.get_source_url();
        if Some(url) != source_url.as_deref() && source_url.is_some() {
            self.update_origin_url(&path, source_url.as_deref().unwrap());
        } else {
            self.set_push_url(&path, url);
        }

        let pretty_version = package.get_pretty_version();
        if let Some(new_ref) =
            self.update_to_commit(package.clone(), &path, &r#ref, &pretty_version)?
        {
            if package.get_dist_reference() == package.get_source_reference() {
                package.set_dist_reference(Some(new_ref.clone()));
            }
            package.set_source_reference(Some(new_ref));
        }

        Ok(None)
    }

    async fn do_update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> Result<Option<PhpMixed>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);
        if !self.has_metadata_repository(&path) {
            return Err(RuntimeException {
                message: format!(
                    "The .git directory is missing from {}, see https://getcomposer.org/commit-deps for more information",
                    path
                ),
                code: 0,
            }
            .into());
        }

        let cache_path = format!(
            "{}/{}/",
            self.inner
                .config
                .borrow_mut()
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or(""),
            Preg::replace(r"{[^a-z0-9.]}i", "-", &Url::sanitize(url.to_string()))?,
        );
        let r#ref = target.get_source_reference().unwrap_or_default();

        let msg;
        let remote_url;
        let has_cached = self
            .cached_packages
            .get(&target.get_id())
            .and_then(|m| m.get(&r#ref))
            .copied()
            .unwrap_or(false);
        if has_cached {
            msg = format!("Checking out {} from cache", self.get_short_hash(&r#ref));
            remote_url = cache_path.clone();
        } else {
            msg = format!("Checking out {}", self.get_short_hash(&r#ref));
            remote_url = "%url%".to_string();
            if Platform::get_env("COMPOSER_DISABLE_NETWORK").is_some() {
                return Err(RuntimeException {
                    message: format!(
                        "The required git reference for {} is not in cache and network is disabled, aborting",
                        target.get_name(),
                    ),
                    code: 0,
                }
                .into());
            }
        }

        self.inner.io.write_error3(&msg, true, io_interface::NORMAL);

        let mut output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &vec![
                "git".to_string(),
                "rev-parse".to_string(),
                "--quiet".to_string(),
                "--verify".to_string(),
                format!("{}^{{commit}}", r#ref),
            ],
            &mut output,
            Some(path.clone()),
        ) != 0
        {
            let commands = vec![
                vec![
                    "git".to_string(),
                    "remote".to_string(),
                    "set-url".to_string(),
                    "composer".to_string(),
                    "--".to_string(),
                    remote_url.clone(),
                ],
                vec![
                    "git".to_string(),
                    "fetch".to_string(),
                    "composer".to_string(),
                ],
                vec![
                    "git".to_string(),
                    "fetch".to_string(),
                    "--tags".to_string(),
                    "composer".to_string(),
                ],
            ];

            self.git_util
                .run_commands(commands, url, Some(&path), false, None)?;
        }

        let command = vec![
            "git".to_string(),
            "remote".to_string(),
            "set-url".to_string(),
            "composer".to_string(),
            "--".to_string(),
            "%sanitizedUrl%".to_string(),
        ];
        self.git_util
            .run_commands(vec![command], url, Some(&path), false, None)?;

        let pretty_version = target.get_pretty_version();
        if let Some(new_ref) =
            self.update_to_commit(target.clone(), &path, &r#ref, &pretty_version)?
        {
            if target.get_dist_reference() == target.get_source_reference() {
                target.set_dist_reference(Some(new_ref.clone()));
            }
            target.set_source_reference(Some(new_ref));
        }

        let mut update_origin_url = false;
        let mut output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &vec!["git".to_string(), "remote".to_string(), "-v".to_string()],
            &mut output,
            Some(path.clone()),
        ) == 0
        {
            let mut origin_match: IndexMap<CaptureKey, String> = IndexMap::new();
            let mut composer_match: IndexMap<CaptureKey, String> = IndexMap::new();
            if Preg::is_match3(
                r"{^origin\s+(?P<url>\S+)}m",
                &output,
                Some(&mut origin_match),
            )
            .unwrap_or(false)
                && Preg::is_match3(
                    r"{^composer\s+(?P<url>\S+)}m",
                    &output,
                    Some(&mut composer_match),
                )
                .unwrap_or(false)
            {
                let origin_url = origin_match
                    .get(&CaptureKey::ByName("url".to_string()))
                    .cloned()
                    .unwrap_or_default();
                let composer_url = composer_match
                    .get(&CaptureKey::ByName("url".to_string()))
                    .cloned()
                    .unwrap_or_default();
                if origin_url == composer_url
                    && Some(composer_url.as_str()) != target.get_source_url().as_deref()
                {
                    update_origin_url = true;
                }
            }
        }
        if update_origin_url && target.get_source_url().is_some() {
            self.update_origin_url(&path, &target.get_source_url().unwrap());
        }

        Ok(None)
    }

    async fn clean_changes(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        update: bool,
    ) -> Result<Option<PhpMixed>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);

        let unpushed = self.get_unpushed_changes(package.clone(), &path)?;
        if let Some(unpushed) = unpushed.as_deref() {
            if self.inner.io.is_interactive()
                || self
                    .inner
                    .config
                    .borrow_mut()
                    .get("discard-changes")
                    .as_bool()
                    != Some(true)
            {
                return Err(RuntimeException {
                    message: format!(
                        "Source directory {} has unpushed changes on the current branch: \n{}",
                        path, unpushed
                    ),
                    code: 0,
                }
                .into());
            }
        }

        let changes = match self.get_local_changes(package.clone(), &path)? {
            Some(c) => c,
            None => return Ok(None),
        };

        if !self.inner.io.is_interactive() {
            let discard_changes = self.inner.config.borrow_mut().get("discard-changes");
            if discard_changes.as_bool() == Some(true) {
                return self.discard_changes(&path).await;
            }
            if discard_changes.as_string() == Some("stash") {
                if !update {
                    self.fail_on_local_changes(package.clone(), &path)?;
                    return Ok(None);
                }

                return self.stash_changes(&path).await;
            }

            self.fail_on_local_changes(package, &path)?;
            return Ok(None);
        }

        let changes: Vec<String> = array_map(
            |elem: &String| format!("    {}", elem),
            &Preg::split(r"{\s*\r?\n\s*}", &changes)?,
        );
        self.inner.io.write_error3(
            &format!(
                "    <error>{} has modified files:</error>",
                package.get_pretty_name()
            ),
            true,
            io_interface::NORMAL,
        );
        let slice_end = 10_usize.min(changes.len());
        self.inner
            .io
            .write_error3(&changes[..slice_end].join("\n"), true, io_interface::NORMAL);
        if (changes.len() as i64) > 10 {
            self.inner.io.write_error3(
                &format!(
                    "    <info>{} more files modified, choose \"v\" to view the full list</info>",
                    changes.len() as i64 - 10
                ),
                true,
                io_interface::NORMAL,
            );
        }

        'outer: loop {
            let answer = self
                .inner
                .io
                .ask(
                    format!(
                        "    <info>Discard changes [y,n,v,{}?]?</info> ",
                        if update { "s," } else { "" }
                    ),
                    PhpMixed::String("?".to_string()),
                )
                .as_string()
                .map(|s| s.to_string());
            let mut do_help = false;
            match answer.as_deref() {
                Some("y") => {
                    self.discard_changes(&path).await?;
                    break 'outer;
                }
                Some("s") => {
                    if !update {
                        // goto help;
                        do_help = true;
                    } else {
                        self.stash_changes(&path).await?;
                        break 'outer;
                    }
                }
                Some("n") => {
                    return Err(RuntimeException {
                        message: "Update aborted".to_string(),
                        code: 0,
                    }
                    .into());
                }
                Some("v") => {
                    self.inner
                        .io
                        .write_error3(&changes.join("\n"), true, io_interface::NORMAL);
                }
                Some("d") => {
                    self.view_diff(&path)?;
                }
                _ => {
                    // case '?': default:
                    do_help = true;
                }
            }

            if do_help {
                // help:
                self.inner.io.write_error3(
                    &[
                        format!(
                            "    y - discard changes and apply the {}",
                            if update { "update" } else { "uninstall" }
                        ),
                        format!(
                            "    n - abort the {} and let you manually clean things up",
                            if update { "update" } else { "uninstall" }
                        ),
                        "    v - view modified files".to_string(),
                        "    d - view local modifications (diff)".to_string(),
                    ]
                    .join("\n"),
                    true,
                    io_interface::NORMAL,
                );
                if update {
                    self.inner.io.write_error3(
                        "    s - stash changes and try to reapply them after the update",
                        true,
                        io_interface::NORMAL,
                    );
                }
                self.inner
                    .io
                    .write_error3("    ? - print help", true, io_interface::NORMAL);
            }
        }

        Ok(None)
    }

    fn reapply_changes(&mut self, path: &str) -> Result<()> {
        let path = self.normalize_path(path);
        if self
            .has_stashed_changes
            .get(&path)
            .copied()
            .unwrap_or(false)
        {
            self.has_stashed_changes.shift_remove(&path);
            self.inner.io.write_error3(
                "    <info>Re-applying stashed changes</info>",
                true,
                io_interface::NORMAL,
            );
            let mut output = String::new();
            if self.inner.process.borrow_mut().execute_args(
                &vec!["git".to_string(), "stash".to_string(), "pop".to_string()],
                &mut output,
                Some(path.clone()),
            ) != 0
            {
                return Err(RuntimeException {
                    message: format!(
                        "Failed to apply stashed changes:\n\n{}",
                        self.inner.process.borrow().get_error_output()
                    ),
                    code: 0,
                }
                .into());
            }
        }

        self.has_discarded_changes.shift_remove(&path);
        Ok(())
    }

    fn get_commit_logs(
        &mut self,
        from_reference: &str,
        to_reference: &str,
        path: &str,
    ) -> Result<String> {
        let path = self.normalize_path(path);
        let mut args = vec![
            "--format=%h - %an: %s".to_string(),
            format!("{}..{}", from_reference, to_reference),
        ];
        args.extend(GitUtil::get_no_show_signature_flags(&self.inner.process));
        let command = GitUtil::build_rev_list_command(&self.inner.process, args);

        let mut output = String::new();
        if self
            .inner
            .process
            .borrow_mut()
            .execute_args(&command, &mut output, Some(path.clone()))
            != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Failed to execute {}\n\n{}",
                    implode(" ", &command),
                    self.inner.process.borrow().get_error_output(),
                ),
                code: 0,
            }
            .into());
        }

        Ok(GitUtil::parse_rev_list_output(&output, &self.inner.process))
    }

    fn has_metadata_repository(&self, path: &str) -> bool {
        let path = self.normalize_path(path);

        is_dir(&format!("{}/.git", path))
    }
}

#[async_trait::async_trait(?Send)]
impl crate::downloader::DownloaderInterface for GitDownloader {
    fn get_installation_source(&self) -> String {
        <Self as VcsDownloader>::get_installation_source(self)
    }

    fn as_dvcs_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::DvcsDownloaderInterface> {
        Some(self)
    }

    fn as_change_report_interface(
        &mut self,
    ) -> Option<&mut dyn crate::downloader::ChangeReportInterface> {
        Some(self)
    }

    fn as_vcs_capable_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::VcsCapableDownloaderInterface> {
        Some(self)
    }

    async fn download(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::download(self, package, path, prev_package).await
    }

    async fn prepare(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::install(self, package, path).await
    }

    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::update(self, initial, target, path).await
    }

    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::remove(self, package, path).await
    }

    async fn cleanup(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
