//! ref: composer/src/Composer/Downloader/GitDownloader.php

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::{
    ChangeReportInterface, DvcsDownloaderInterface, VcsCapableDownloaderInterface, VcsDownloader,
    VcsDownloaderBase,
};
use crate::io::{IOInterface, IOInterfaceImmutable, io_interface};
use crate::package::PackageInterfaceHandle;
use crate::util::{Filesystem, Git as GitUtil, Platform, ProcessExecutor, Url};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    CmpOp, PhpMixed, RuntimeException, array_map, basename, bconcat, dirname, impl_php_class,
    implode, in_array_strict, is_dir, legacy_lossy, path_from_bytes, path_to_bytes, php_regex,
    preg_is_match, preg_match, preg_match_all, preg_quote, preg_replace, preg_split, realpath,
    rtrim2, strlen, strpos, substr, trim, version_compare,
};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct GitDownloader {
    inner: VcsDownloaderBase,
    has_stashed_changes: std::cell::RefCell<IndexMap<BString, bool>>,
    has_discarded_changes: std::cell::RefCell<IndexMap<BString, bool>>,
    git_util: std::cell::RefCell<GitUtil>,
    cached_packages: std::cell::RefCell<IndexMap<i64, IndexMap<BString, bool>>>,
}

impl_php_class!(GitDownloader, r"Composer\Downloader\GitDownloader");

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
            has_stashed_changes: std::cell::RefCell::new(IndexMap::new()),
            has_discarded_changes: std::cell::RefCell::new(IndexMap::new()),
            git_util: std::cell::RefCell::new(git_util),
            cached_packages: std::cell::RefCell::new(IndexMap::new()),
        }
    }

    pub fn get_unpushed_changes(
        &self,
        _package: PackageInterfaceHandle,
        path: &Path,
    ) -> anyhow::Result<Option<BString>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);
        if !self.has_metadata_repository(&path) {
            return Ok(None);
        }

        let command: Vec<BString> = vec![
            "git".into(),
            "show-ref".into(),
            "--head".into(),
            "-d".into(),
        ];
        let mut output = BString::default();
        if self
            .inner
            .process
            .borrow_mut()
            .execute(&command, &mut output, Some(&path))?
            != 0
        {
            return Err(RuntimeException::new(format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command).display_lossy(),
                self.inner
                    .process
                    .borrow()
                    .get_error_output()
                    .display_lossy(),
            ))
            .into());
        }

        let mut refs = trim(&output);
        let Some(head_match) = preg_match(php_regex!(r"{^([a-f0-9]+) HEAD$}mi"), &refs) else {
            // could not match the HEAD for some reason
            return Ok(None);
        };
        let head_ref = BString::from(head_match.get(1).unwrap_or_default());

        let candidate_branches: Vec<BString> = preg_match_all(
            bconcat!("{^", preg_quote(&head_ref, None), " refs/heads/(.+)$}mi"),
            &refs,
        )
        .map(|branch_match| {
            BString::from(
                branch_match
                    .get(1)
                    .expect("group 1 participates whenever the pattern matches"),
            )
        })
        .collect();
        if candidate_branches.is_empty() {
            // not on a branch, we are either on a not-modified tag or some sort of detached head, so skip this
            return Ok(None);
        }

        // use the first match as branch name for now
        let mut branch = candidate_branches[0].clone();
        let mut unpushed_changes: Option<BString> = None;
        let mut branch_not_found_error = false;

        // do two passes, as if we find anything we want to fetch and then re-try
        for i in 0..=1 {
            let mut remote_branches: Vec<BString> = vec![];

            // try to find matching branch names in remote repos
            for candidate in &candidate_branches {
                let matches: Vec<BString> = preg_match_all(
                    bconcat!(
                        "{^[a-f0-9]+ refs/remotes/((?:[^/]+)/",
                        preg_quote(candidate, None),
                        ")$}mi"
                    ),
                    &refs,
                )
                .map(|m| {
                    BString::from(
                        m.get(1)
                            .expect("group 1 participates whenever the pattern matches"),
                    )
                })
                .collect();
                if !matches.is_empty() {
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
                unpushed_changes = Some(bconcat!(
                    "Branch ",
                    branch,
                    " could not be found on any remote and appears to be unpushed"
                ));
                branch_not_found_error = true;
            } else {
                // if first iteration found no remote branch but it has now found some, reset $unpushedChanges
                // so we get the real diff output no matter its length
                if branch_not_found_error {
                    unpushed_changes = None;
                }
                for remote_branch in &remote_branches {
                    let command: Vec<BString> = vec![
                        "git".into(),
                        "diff".into(),
                        "--name-status".into(),
                        bconcat!(remote_branch, "...", branch),
                        "--".into(),
                    ];
                    let mut output = BString::default();
                    if self.inner.process.borrow_mut().execute(
                        &command,
                        &mut output,
                        Some(&path),
                    )? != 0
                    {
                        return Err(RuntimeException::new(format!(
                            "Failed to execute {}\n\n{}",
                            implode(" ", &command).display_lossy(),
                            self.inner
                                .process
                                .borrow()
                                .get_error_output()
                                .display_lossy(),
                        ))
                        .into());
                    }

                    let output = trim(&output);
                    // keep the shortest diff from all remote branches we compare against
                    if unpushed_changes.is_none()
                        || strlen(&output)
                            < strlen(
                                unpushed_changes
                                    .as_deref()
                                    .map(Vec::as_slice)
                                    .unwrap_or_default(),
                            )
                    {
                        unpushed_changes = Some(output);
                    }
                }
            }

            // first pass and we found unpushed changes, fetch from all remotes to make sure we have up to date
            // remotes and then try again as outdated remotes can sometimes cause false-positives
            if unpushed_changes.is_some() && i == 0 {
                let mut output = BString::default();
                self.inner.process.borrow_mut().execute(
                    &["git", "fetch", "--all"],
                    &mut output,
                    Some(&path),
                )?;

                // update list of refs after fetching
                let command: Vec<BString> = vec![
                    "git".into(),
                    "show-ref".into(),
                    "--head".into(),
                    "-d".into(),
                ];
                let mut output = BString::default();
                if self
                    .inner
                    .process
                    .borrow_mut()
                    .execute(&command, &mut output, Some(&path))?
                    != 0
                {
                    return Err(RuntimeException::new(format!(
                        "Failed to execute {}\n\n{}",
                        implode(" ", &command).display_lossy(),
                        self.inner
                            .process
                            .borrow()
                            .get_error_output()
                            .display_lossy(),
                    ))
                    .into());
                }
                refs = trim(&output);
            }

            // abort after first pass if we didn't find anything
            if unpushed_changes.is_none() {
                break;
            }
        }

        Ok(unpushed_changes)
    }

    /// Updates the given path to the given commit ref
    fn update_to_commit(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        reference: &BStr,
        pretty_version: &BStr,
    ) -> anyhow::Result<Option<BString>> {
        let path_key = path_to_bytes(path).as_bstr();
        let force: Vec<BString> = if self
            .has_discarded_changes
            .borrow()
            .get(path_key)
            .copied()
            .unwrap_or(false)
            || self
                .has_stashed_changes
                .borrow()
                .get(path_key)
                .copied()
                .unwrap_or(false)
        {
            vec!["-f".into()]
        } else {
            vec![]
        };

        // This uses the "--" sequence to separate branch from file parameters.
        //
        // Otherwise git tries the branch name as well as file name.
        // If the non-existent branch is actually the name of a file, the file
        // is checked out.

        let mut branch = preg_replace(
            php_regex!(r"{(?:^dev-|(?:\.x)?-dev$)}i"),
            "",
            pretty_version,
        );

        // Closure equivalent: $execute = function(array $command) use (&$output, $path) { ... };
        // Inlined below at each call site.

        let mut branches: Option<BString> = None;
        {
            let mut output = BString::default();
            if self.inner.process.borrow_mut().execute(
                &["git", "branch", "-r"],
                &mut output,
                Some(path),
            )? == 0
            {
                branches = Some(output);
            }
        }

        // check whether non-commitish are branches or tags, and fetch branches with the remote name
        let git_ref = reference.to_owned();
        if !preg_is_match(php_regex!(r"{^[a-f0-9]{40}$}"), reference)
            && branches.is_some()
            && preg_is_match(
                bconcat!("{^\\s+composer/", preg_quote(reference, None), "$}m"),
                branches.as_deref().map(Vec::as_slice).unwrap_or_default(),
            )
        {
            let mut command1: Vec<BString> = vec!["git".into(), "checkout".into()];
            command1.extend(force.clone());
            command1.extend(vec![
                "-B".into(),
                branch.clone(),
                bconcat!("composer/", reference),
                "--".into(),
            ]);
            let command2: Vec<BString> = vec![
                "git".into(),
                "reset".into(),
                "--hard".into(),
                bconcat!("composer/", reference),
                "--".into(),
            ];

            let mut output = BString::default();
            let ok1 =
                self.inner
                    .process
                    .borrow_mut()
                    .execute(&command1, &mut output, Some(path))?
                    == 0;
            let ok2 = if ok1 {
                let mut output = BString::default();
                self.inner
                    .process
                    .borrow_mut()
                    .execute(&command2, &mut output, Some(path))?
                    == 0
            } else {
                false
            };
            if ok1 && ok2 {
                return Ok(None);
            }
        }

        // try to checkout branch by name and then reset it so it's on the proper branch name
        if preg_is_match(php_regex!(r"{^[a-f0-9]{40}$}"), reference) {
            // add 'v' in front of the branch if it was stripped when generating the pretty name
            if branches.is_some()
                && !preg_is_match(
                    bconcat!("{^\\s+composer/", preg_quote(&branch, None), "$}m"),
                    branches.as_deref().map(Vec::as_slice).unwrap_or_default(),
                )
                && preg_is_match(
                    bconcat!("{^\\s+composer/v", preg_quote(&branch, None), "$}m"),
                    branches.as_deref().map(Vec::as_slice).unwrap_or_default(),
                )
            {
                branch = bconcat!("v", branch);
            }

            let command: Vec<BString> =
                vec!["git".into(), "checkout".into(), branch.clone(), "--".into()];
            let mut fallback_command: Vec<BString> = vec!["git".into(), "checkout".into()];
            fallback_command.extend(force.clone());
            fallback_command.extend(vec![
                "-B".into(),
                branch.clone(),
                bconcat!("composer/", branch),
                "--".into(),
            ]);
            let reset_command: Vec<BString> = vec![
                "git".into(),
                "reset".into(),
                "--hard".into(),
                reference.to_owned(),
                "--".into(),
            ];

            let mut output = BString::default();
            let ok_command =
                self.inner
                    .process
                    .borrow_mut()
                    .execute(&command, &mut output, Some(path))?
                    == 0;
            let ok_fallback = if !ok_command {
                let mut output = BString::default();
                self.inner.process.borrow_mut().execute(
                    &fallback_command,
                    &mut output,
                    Some(path),
                )? == 0
            } else {
                false
            };
            let ok_reset = if ok_command || ok_fallback {
                let mut output = BString::default();
                self.inner
                    .process
                    .borrow_mut()
                    .execute(&reset_command, &mut output, Some(path))?
                    == 0
            } else {
                false
            };
            if (ok_command || ok_fallback) && ok_reset {
                return Ok(None);
            }
        }

        let mut command1: Vec<BString> = vec!["git".into(), "checkout".into()];
        command1.extend(force);
        command1.extend(vec![git_ref.clone(), "--".into()]);
        let command2: Vec<BString> = vec![
            "git".into(),
            "reset".into(),
            "--hard".into(),
            git_ref,
            "--".into(),
        ];
        {
            let mut output = BString::default();
            let ok1 =
                self.inner
                    .process
                    .borrow_mut()
                    .execute(&command1, &mut output, Some(path))?
                    == 0;
            let ok2 = if ok1 {
                let mut output = BString::default();
                self.inner
                    .process
                    .borrow_mut()
                    .execute(&command2, &mut output, Some(path))?
                    == 0
            } else {
                false
            };
            if ok1 && ok2 {
                return Ok(None);
            }
        }

        let mut exception_extra = BString::default();

        // reference was not found (prints "fatal: reference is not a tree: $ref")
        if strpos(self.inner.process.borrow().get_error_output(), reference).is_some() {
            self.inner.io.write_error3(
                &format!(
                    "    <warning>{} is gone (history was rewritten?)</warning>",
                    reference.display_lossy()
                ),
                true,
                io_interface::NORMAL,
            );
            exception_extra = bconcat!(
                "\nIt looks like the commit hash is not available in the repository, maybe ",
                if package.is_dev() {
                    "the commit was removed from the branch"
                } else {
                    "the tag was recreated"
                },
                "? Run \"shirabe update ",
                package.get_pretty_name(),
                "\" to resolve this.",
            );
        }

        let command = bconcat!(implode(" ", &command1), " && ", implode(" ", &command2));

        Err(RuntimeException::new(
            Url::sanitize(bconcat!(
                "Failed to execute ",
                command,
                "\n\n",
                self.inner.process.borrow().get_error_output(),
                exception_extra,
            ))
            .display_lossy()
            .to_string(),
        )
        .into())
    }

    fn update_origin_url(&self, path: &Path, url: &BStr) -> anyhow::Result<()> {
        let mut output = BString::default();
        self.inner.process.borrow_mut().execute(
            &[
                BString::from("git"),
                "remote".into(),
                "set-url".into(),
                "origin".into(),
                "--".into(),
                url.to_owned(),
            ],
            &mut output,
            Some(path),
        )?;
        self.set_push_url(path, url)
    }

    fn set_push_url(&self, path: &Path, url: &BStr) -> anyhow::Result<()> {
        // set push url for github projects
        if let Some(match_) = preg_match(
            format!(
                "{{^(?:https?|git)://{}/([^/]+)/([^/]+?)(?:\\.git)?$}}",
                GitUtil::get_github_domains_regex(&self.inner.config.borrow())
            ),
            url,
        ) {
            let protocols = self.inner.config.borrow_mut().get("github-protocols");
            let m1 = match_.get(1).unwrap_or_default();
            let m2 = match_.get(2).unwrap_or_default();
            let m3 = match_.get(3).unwrap_or_default();
            let mut push_url = bconcat!("git@", m1, ":", m2, "/", m3, ".git");
            if !in_array_strict("ssh".to_string(), protocols.values()) {
                push_url = bconcat!("https://", m1, "/", m2, "/", m3, ".git");
            }
            let cmd: Vec<BString> = vec![
                "git".into(),
                "remote".into(),
                "set-url".into(),
                "--push".into(),
                "origin".into(),
                "--".into(),
                push_url,
            ];
            let mut ignored_output = BString::default();
            self.inner
                .process
                .borrow_mut()
                .execute(&cmd, &mut ignored_output, Some(path))?;
        }

        Ok(())
    }

    async fn discard_changes(&self, path: &Path) -> anyhow::Result<Option<PhpMixed>> {
        let path = self.normalize_path(path);
        let mut output = BString::default();
        if self.inner.process.borrow_mut().execute(
            &["git", "clean", "-df"],
            &mut output,
            Some(&path),
        )? != 0
        {
            return Err(RuntimeException::new(format!(
                "Could not reset changes\n\n:{}",
                output.display_lossy()
            ))
            .into());
        }
        let mut output = BString::default();
        if self.inner.process.borrow_mut().execute(
            &["git", "reset", "--hard"],
            &mut output,
            Some(&path),
        )? != 0
        {
            return Err(RuntimeException::new(format!(
                "Could not reset changes\n\n:{}",
                output.display_lossy()
            ))
            .into());
        }

        self.has_discarded_changes
            .borrow_mut()
            .insert(BString::from(path_to_bytes(&path)), true);

        Ok(None)
    }

    async fn stash_changes(&self, path: &Path) -> anyhow::Result<Option<PhpMixed>> {
        let path = self.normalize_path(path);
        let mut output = BString::default();
        if self.inner.process.borrow_mut().execute(
            &["git", "stash", "--include-untracked"],
            &mut output,
            Some(&path),
        )? != 0
        {
            return Err(RuntimeException::new(format!(
                "Could not stash changes\n\n:{}",
                output.display_lossy()
            ))
            .into());
        }

        self.has_stashed_changes
            .borrow_mut()
            .insert(BString::from(path_to_bytes(&path)), true);

        Ok(None)
    }

    fn view_diff(&self, path: &Path) -> anyhow::Result<()> {
        let path = self.normalize_path(path);
        let mut output = BString::default();
        if self.inner.process.borrow_mut().execute(
            &["git", "diff", "HEAD"],
            &mut output,
            Some(&path),
        )? != 0
        {
            return Err(RuntimeException::new(format!(
                "Could not view diff\n\n:{}",
                output.display_lossy()
            ))
            .into());
        }

        self.inner.io.write_error3(
            &output.display_lossy().to_string(),
            true,
            io_interface::NORMAL,
        );

        Ok(())
    }

    fn normalize_path(&self, path: &Path) -> PathBuf {
        let mut path = BString::from(path_to_bytes(path));
        if Platform::is_windows() && strlen(&path) > 0 {
            let mut base_path = path.clone();
            let mut removed: Vec<BString> = vec![];

            while !is_dir(path_from_bytes(&base_path)) && base_path != "\\" {
                let mut new_removed = vec![basename(&base_path)];
                new_removed.extend(removed);
                removed = new_removed;
                base_path = dirname(&base_path);
            }

            if base_path == "\\" {
                return PathBuf::from(path_from_bytes(&path));
            }

            path = rtrim2(
                bconcat!(
                    realpath(path_from_bytes(&base_path)).unwrap_or_default(),
                    "/",
                    implode("/", &removed),
                ),
                "/",
            );
        }

        PathBuf::from(path_from_bytes(&path))
    }

    fn get_short_hash(&self, reference: &BStr) -> BString {
        if !self.inner.io.is_verbose() && preg_is_match(php_regex!(r"{^[0-9a-f]{40}$}"), reference)
        {
            return substr(reference, 0, Some(10));
        }

        reference.to_owned()
    }

    // TODO(bytes): the Git utility holds commands, URLs and paths as Strings.
    fn run_git_commands(
        &self,
        commands: Vec<Vec<BString>>,
        url: &str,
        cwd: &Path,
        initial_clone: bool,
    ) -> anyhow::Result<()> {
        let commands: Vec<Vec<String>> = commands
            .iter()
            .map(|command| {
                command
                    .iter()
                    .map(|arg| legacy_lossy(arg).into_owned())
                    .collect()
            })
            .collect();
        self.git_util.borrow_mut().run_commands(
            commands,
            url,
            Some(&legacy_lossy(path_to_bytes(cwd))),
            initial_clone,
            (),
        )
    }

    /// The default `VcsDownloader::clean_changes()` behavior: fail if the working copy has
    /// local changes.
    async fn fail_on_local_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
    ) -> anyhow::Result<()> {
        if self.get_local_changes(package, path).await?.is_some() {
            return Err(RuntimeException::new(format!(
                "Source directory {} has uncommitted changes.",
                path.display()
            ))
            .into());
        }

        Ok(())
    }
}

impl DvcsDownloaderInterface for GitDownloader {
    fn get_unpushed_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
    ) -> anyhow::Result<Option<bstr::BString>> {
        GitDownloader::get_unpushed_changes(self, package, path)
    }
}

#[async_trait::async_trait(?Send)]
impl ChangeReportInterface for GitDownloader {
    async fn get_local_changes(
        &self,
        _package: PackageInterfaceHandle,
        path: &Path,
    ) -> anyhow::Result<Option<bstr::BString>> {
        GitUtil::clean_env(&self.inner.process);
        if !self.has_metadata_repository(path) {
            return Ok(None);
        }

        let command: Vec<BString> = vec![
            "git".into(),
            "status".into(),
            "--porcelain".into(),
            "--untracked-files=no".into(),
        ];
        let mut output = BString::default();
        if self
            .inner
            .process
            .borrow_mut()
            .execute(&command, &mut output, Some(path))?
            != 0
        {
            return Err(RuntimeException::new(format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command).display_lossy(),
                self.inner
                    .process
                    .borrow()
                    .get_error_output()
                    .display_lossy(),
            ))
            .into());
        }

        let output = trim(&output);

        Ok(if strlen(&output) > 0 {
            Some(output)
        } else {
            None
        })
    }
}

impl VcsCapableDownloaderInterface for GitDownloader {
    fn get_vcs_reference(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
    ) -> Option<bstr::BString> {
        self.inner.get_vcs_reference(package, path)
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

    fn has_cleaned_changes(&self) -> &std::cell::RefCell<IndexMap<bstr::BString, bool>> {
        &self.inner.has_cleaned_changes
    }

    async fn do_download(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        url: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        // Do not create an extra local cache when repository is already local
        if Filesystem::is_local_path(url) {
            return Ok(None);
        }

        GitUtil::clean_env(&self.inner.process);

        let cache_path = PathBuf::from(path_from_bytes(&bconcat!(
            self.inner
                .config
                .borrow_mut()
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or_default(),
            "/",
            preg_replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(url)),
            "/",
        )));
        let git_version = GitUtil::get_version(&self.inner.process)?;

        // --dissociate option is only available since git 2.3.0-rc0
        if git_version.is_some()
            && version_compare(
                git_version.as_deref().unwrap_or_default(),
                "2.3.0-rc0",
                CmpOp::Ge,
            )
            && Cache::is_usable(&cache_path)
        {
            self.inner.io.write_error3(
                &format!(
                    "  - Syncing <info>{}</info> (<comment>{}</comment>) into cache",
                    package.get_name().display_lossy(),
                    package
                        .get_full_pretty_version(true, crate::package::DisplayMode::SourceRefIfDev)
                        .display_lossy(),
                ),
                true,
                io_interface::NORMAL,
            );
            self.inner.io.write_error3(
                &format!("    Cloning to cache at {}", cache_path.display()),
                true,
                io_interface::DEBUG,
            );
            let r#ref = package.get_source_reference().unwrap_or_default();
            // TODO(bytes): the Git utility holds URLs, paths and refs as Strings.
            if self.git_util.borrow_mut().fetch_ref_or_sync_mirror(
                url,
                &legacy_lossy(path_to_bytes(&cache_path)),
                &legacy_lossy(&r#ref),
                Some(&legacy_lossy(&package.get_pretty_version())),
            )? && is_dir(&cache_path)
            {
                self.cached_packages
                    .borrow_mut()
                    .entry(package.get_id())
                    .or_default()
                    .insert(r#ref, true);
            }
        } else if git_version.is_none() {
            return Err(RuntimeException::new(
                "git was not found in your PATH, skipping source download".to_string(),
            )
            .into());
        }

        Ok(None)
    }

    async fn do_install(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        url: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);
        let cache_path = bconcat!(
            self.inner
                .config
                .borrow_mut()
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or_default(),
            "/",
            preg_replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(url)),
            "/",
        );
        let r#ref = package.get_source_reference().unwrap_or_default();

        let msg;
        let commands: Vec<Vec<BString>>;
        let has_cached = self
            .cached_packages
            .borrow()
            .get(&package.get_id())
            .and_then(|m| m.get(r#ref.as_bstr()))
            .copied()
            .unwrap_or(false);
        if has_cached {
            msg = format!(
                "Cloning {} from cache",
                self.get_short_hash(r#ref.as_bstr()).display_lossy()
            );

            let mut clone_flags: Vec<BString> = vec![
                "--dissociate".into(),
                "--reference".into(),
                cache_path.clone(),
            ];
            let transport_options = package.get_transport_options();
            if let Some(git_opts) = transport_options.get(B("git")).and_then(|v| v.as_array())
                && git_opts
                    .get(B("single_use_clone"))
                    .is_some_and(|v| v.to_bool())
            {
                clone_flags = vec![];
            }

            commands = vec![
                {
                    let mut base: Vec<BString> = vec![
                        "git".into(),
                        "clone".into(),
                        "--no-checkout".into(),
                        cache_path,
                        path_to_bytes(&path).into(),
                    ];
                    base.extend(clone_flags);
                    base
                },
                vec![
                    "git".into(),
                    "remote".into(),
                    "set-url".into(),
                    "origin".into(),
                    "--".into(),
                    "%sanitizedUrl%".into(),
                ],
                vec![
                    "git".into(),
                    "remote".into(),
                    "add".into(),
                    "composer".into(),
                    "--".into(),
                    "%sanitizedUrl%".into(),
                ],
            ];
        } else {
            msg = format!(
                "Cloning {}",
                self.get_short_hash(r#ref.as_bstr()).display_lossy()
            );
            commands = vec![
                vec![
                    "git".into(),
                    "clone".into(),
                    "--no-checkout".into(),
                    "--".into(),
                    "%url%".into(),
                    path_to_bytes(&path).into(),
                ],
                vec![
                    "git".into(),
                    "remote".into(),
                    "add".into(),
                    "composer".into(),
                    "--".into(),
                    "%url%".into(),
                ],
                vec!["git".into(), "fetch".into(), "composer".into()],
                vec![
                    "git".into(),
                    "remote".into(),
                    "set-url".into(),
                    "origin".into(),
                    "--".into(),
                    "%sanitizedUrl%".into(),
                ],
                vec![
                    "git".into(),
                    "remote".into(),
                    "set-url".into(),
                    "composer".into(),
                    "--".into(),
                    "%sanitizedUrl%".into(),
                ],
            ];
            if Platform::get_env("COMPOSER_DISABLE_NETWORK").is_some() {
                return Err(RuntimeException::new(format!(
                    "The required git reference for {} is not in cache and network is disabled, aborting",
                    package.get_name().display_lossy(),
                ))
                .into());
            }
        }

        self.inner.io.write_error3(&msg, true, io_interface::NORMAL);

        self.run_git_commands(commands, url, &path, true)?;

        let source_url = package.get_source_url();
        if source_url.as_ref().is_none_or(|s| s != url) && source_url.is_some() {
            self.update_origin_url(&path, source_url.as_deref().unwrap().as_bstr())?;
        } else {
            self.set_push_url(&path, url.as_bytes().as_bstr())?;
        }

        if let Some(new_ref) = self.update_to_commit(
            package.clone(),
            &path,
            r#ref.as_bstr(),
            package.get_pretty_version().as_bstr(),
        )? {
            if package.get_dist_reference() == package.get_source_reference() {
                package.set_dist_reference(Some(new_ref.clone()));
            }
            package.set_source_reference(Some(new_ref));
        }

        Ok(None)
    }

    async fn do_update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &Path,
        url: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);
        if !self.has_metadata_repository(&path) {
            return Err(RuntimeException::new(format!(
                "The .git directory is missing from {}, see https://getcomposer.org/commit-deps for more information",
                path.display()
            ))
            .into());
        }

        let cache_path = bconcat!(
            self.inner
                .config
                .borrow_mut()
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or_default(),
            "/",
            preg_replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(url)),
            "/",
        );
        let r#ref = target.get_source_reference().unwrap_or_default();

        let msg;
        let remote_url: BString;
        let has_cached = self
            .cached_packages
            .borrow()
            .get(&target.get_id())
            .and_then(|m| m.get(r#ref.as_bstr()))
            .copied()
            .unwrap_or(false);
        if has_cached {
            msg = format!(
                "Checking out {} from cache",
                self.get_short_hash(r#ref.as_bstr()).display_lossy()
            );
            remote_url = cache_path;
        } else {
            msg = format!(
                "Checking out {}",
                self.get_short_hash(r#ref.as_bstr()).display_lossy()
            );
            remote_url = "%url%".into();
            if Platform::get_env("COMPOSER_DISABLE_NETWORK").is_some() {
                return Err(RuntimeException::new(format!(
                    "The required git reference for {} is not in cache and network is disabled, aborting",
                    target.get_name().display_lossy(),
                ))
                .into());
            }
        }

        self.inner.io.write_error3(&msg, true, io_interface::NORMAL);

        let mut output = BString::default();
        if self.inner.process.borrow_mut().execute(
            &[
                BString::from("git"),
                "rev-parse".into(),
                "--quiet".into(),
                "--verify".into(),
                bconcat!(r#ref, "^{commit}"),
            ],
            &mut output,
            Some(&path),
        )? != 0
        {
            let commands: Vec<Vec<BString>> = vec![
                vec![
                    "git".into(),
                    "remote".into(),
                    "set-url".into(),
                    "composer".into(),
                    "--".into(),
                    remote_url,
                ],
                vec!["git".into(), "fetch".into(), "composer".into()],
                vec![
                    "git".into(),
                    "fetch".into(),
                    "--tags".into(),
                    "composer".into(),
                ],
            ];

            self.run_git_commands(commands, url, &path, false)?;
        }

        let command: Vec<BString> = vec![
            "git".into(),
            "remote".into(),
            "set-url".into(),
            "composer".into(),
            "--".into(),
            "%sanitizedUrl%".into(),
        ];
        self.run_git_commands(vec![command], url, &path, false)?;

        if let Some(new_ref) = self.update_to_commit(
            target.clone(),
            &path,
            r#ref.as_bstr(),
            target.get_pretty_version().as_bstr(),
        )? {
            if target.get_dist_reference() == target.get_source_reference() {
                target.set_dist_reference(Some(new_ref.clone()));
            }
            target.set_source_reference(Some(new_ref));
        }

        let mut update_origin_url = false;
        let mut output = BString::default();
        if self.inner.process.borrow_mut().execute(
            &["git", "remote", "-v"],
            &mut output,
            Some(&path),
        )? == 0
            && let Some(origin_match) =
                preg_match(php_regex!(r"{^origin\s+(?P<url>\S+)}m"), &output)
            && let Some(composer_match) =
                preg_match(php_regex!(r"{^composer\s+(?P<url>\S+)}m"), &output)
        {
            let origin_url = origin_match.name("url").unwrap_or_default();
            let composer_url = composer_match.name("url").unwrap_or_default();
            if origin_url == composer_url
                && target
                    .get_source_url()
                    .is_none_or(|s| s.as_bstr() != composer_url)
            {
                update_origin_url = true;
            }
        }
        if update_origin_url && target.get_source_url().is_some() {
            self.update_origin_url(&path, target.get_source_url().unwrap().as_bstr())?;
        }

        Ok(None)
    }

    async fn clean_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        update: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);

        let unpushed = self.get_unpushed_changes(package.clone(), &path)?;
        if let Some(unpushed) = unpushed.as_ref()
            && (self.inner.io.is_interactive()
                || self
                    .inner
                    .config
                    .borrow_mut()
                    .get("discard-changes")
                    .as_bool()
                    != Some(true))
        {
            return Err(RuntimeException::new(format!(
                "Source directory {} has unpushed changes on the current branch: \n{}",
                path.display(),
                unpushed.display_lossy()
            ))
            .into());
        }

        let changes = match self.get_local_changes(package.clone(), &path).await? {
            Some(c) => c,
            None => return Ok(None),
        };

        if !self.inner.io.is_interactive() {
            let discard_changes = self.inner.config.borrow_mut().get("discard-changes");
            if discard_changes.as_bool() == Some(true) {
                return self.discard_changes(&path).await;
            }
            if discard_changes.as_string() == Some("stash".into()) {
                if !update {
                    self.fail_on_local_changes(package.clone(), &path).await?;
                    return Ok(None);
                }

                return self.stash_changes(&path).await;
            }

            self.fail_on_local_changes(package, &path).await?;
            return Ok(None);
        }

        let changes: Vec<BString> = array_map(
            |elem: &BString| bconcat!("    ", elem),
            &preg_split(php_regex!(r"{\s*\r?\n\s*}"), &changes),
        );
        self.inner.io.write_error3(
            &format!(
                "    <error>{} has modified files:</error>",
                package.get_pretty_name().display_lossy()
            ),
            true,
            io_interface::NORMAL,
        );
        let slice_end = 10_usize.min(changes.len());
        self.inner.io.write_error3(
            &implode("\n", &changes[..slice_end])
                .display_lossy()
                .to_string(),
            true,
            io_interface::NORMAL,
        );
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
                    PhpMixed::string("?".to_string()),
                )?
                .as_string()
                .map(|s| legacy_lossy(s).into_owned());
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
                    return Err(RuntimeException::new("Update aborted".to_string()).into());
                }
                Some("v") => {
                    self.inner.io.write_error3(
                        &implode("\n", &changes).display_lossy().to_string(),
                        true,
                        io_interface::NORMAL,
                    );
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

    fn reapply_changes(&self, path: &Path) -> anyhow::Result<()> {
        let path = self.normalize_path(path);
        let path_key = path_to_bytes(&path).as_bstr();
        if self
            .has_stashed_changes
            .borrow()
            .get(path_key)
            .copied()
            .unwrap_or(false)
        {
            self.has_stashed_changes.borrow_mut().shift_remove(path_key);
            self.inner.io.write_error3(
                "    <info>Re-applying stashed changes</info>",
                true,
                io_interface::NORMAL,
            );
            let mut output = BString::default();
            if self.inner.process.borrow_mut().execute(
                &["git", "stash", "pop"],
                &mut output,
                Some(&path),
            )? != 0
            {
                return Err(RuntimeException::new(format!(
                    "Failed to apply stashed changes:\n\n{}",
                    self.inner
                        .process
                        .borrow()
                        .get_error_output()
                        .display_lossy()
                ))
                .into());
            }
        }

        self.has_discarded_changes
            .borrow_mut()
            .shift_remove(path_key);
        Ok(())
    }

    fn get_commit_logs(
        &self,
        from_reference: &bstr::BStr,
        to_reference: &bstr::BStr,
        path: &Path,
    ) -> anyhow::Result<bstr::BString> {
        let path = self.normalize_path(path);
        let mut args: Vec<BString> = vec![
            "--format=%h - %an: %s".into(),
            bconcat!(from_reference, "..", to_reference),
        ];
        args.extend(GitUtil::get_no_show_signature_flags(&self.inner.process)?);
        let command = GitUtil::build_rev_list_command(&self.inner.process, args)?;

        let mut output = BString::default();
        if self
            .inner
            .process
            .borrow_mut()
            .execute(&command, &mut output, Some(&path))?
            != 0
        {
            return Err(RuntimeException::new(format!(
                "Failed to execute {}\n\n{}",
                implode(" ", &command).display_lossy(),
                self.inner
                    .process
                    .borrow()
                    .get_error_output()
                    .display_lossy(),
            ))
            .into());
        }

        GitUtil::parse_rev_list_output(&output, &self.inner.process)
    }

    fn has_metadata_repository(&self, path: &Path) -> bool {
        let path = self.normalize_path(path);

        is_dir(path_from_bytes(&bconcat!(path, "/.git")))
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

    fn as_change_report_interface(&self) -> Option<&dyn crate::downloader::ChangeReportInterface> {
        Some(self)
    }

    fn as_vcs_capable_downloader_interface(
        &self,
    ) -> Option<&dyn crate::downloader::VcsCapableDownloaderInterface> {
        Some(self)
    }

    async fn download(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        prev_package: Option<PackageInterfaceHandle>,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::download(self, package, path, prev_package).await
    }

    async fn prepare(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::install(self, package, path).await
    }

    async fn update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::update(self, initial, target, path).await
    }

    async fn remove(
        &self,
        package: PackageInterfaceHandle,
        path: &Path,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::remove(self, package, path).await
    }

    async fn cleanup(
        &self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
