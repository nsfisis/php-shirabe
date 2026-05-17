//! ref: composer/src/Composer/Downloader/GitDownloader.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, array_map, basename, dirname, implode, in_array, is_dir,
    preg_quote, realpath, rtrim, sprintf, strlen, strpos, substr, trim, version_compare,
};

use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::dvcs_downloader_interface::DvcsDownloaderInterface;
use crate::downloader::vcs_downloader::VcsDownloaderBase;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::util::filesystem::Filesystem;
use crate::util::git::Git as GitUtil;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;
use crate::util::url::Url;

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
        io: Box<dyn IOInterface>,
        config: Config,
        process: Option<ProcessExecutor>,
        fs: Option<Filesystem>,
    ) -> Self {
        let inner = VcsDownloaderBase::new(io, config, process, fs);
        let git_util = GitUtil::new(&*inner.io, &inner.config, &inner.process, &inner.filesystem);
        Self {
            inner,
            has_stashed_changes: IndexMap::new(),
            has_discarded_changes: IndexMap::new(),
            git_util,
            cached_packages: IndexMap::new(),
        }
    }

    pub(crate) fn do_download(
        &mut self,
        package: &dyn PackageInterface,
        _path: &str,
        url: &str,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        // Do not create an extra local cache when repository is already local
        if Filesystem::is_local_path(url) {
            return Ok(promise::resolve(None));
        }

        GitUtil::clean_env(&self.inner.process);

        let cache_path = format!(
            "{}/{}/",
            self.inner
                .config
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or(""),
            Preg::replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(url.to_string())),
        );
        let git_version = GitUtil::get_version(&self.inner.process);

        // --dissociate option is only available since git 2.3.0-rc0
        if git_version.is_some()
            && version_compare(git_version.as_deref().unwrap_or(""), "2.3.0-rc0", ">=")
            && Cache::is_usable(&cache_path)
        {
            self.inner.io.write_error(
                PhpMixed::String(format!(
                    "  - Syncing <info>{}</info> (<comment>{}</comment>) into cache",
                    package.get_name(),
                    package.get_full_pretty_version(),
                )),
                true,
                IOInterface::NORMAL,
            );
            self.inner.io.write_error(
                PhpMixed::String(sprintf(
                    "    Cloning to cache at %s",
                    &[PhpMixed::String(cache_path.clone())],
                )),
                true,
                IOInterface::DEBUG,
            );
            let r#ref = package.get_source_reference();
            if self.git_util.fetch_ref_or_sync_mirror(
                url,
                &cache_path,
                r#ref.unwrap_or(""),
                Some(package.get_pretty_version()),
            ) && is_dir(&cache_path)
            {
                self.cached_packages
                    .entry(package.get_id())
                    .or_insert_with(IndexMap::new)
                    .insert(r#ref.unwrap_or("").to_string(), true);
            }
        } else if git_version.is_none() {
            return Err(RuntimeException {
                message: "git was not found in your PATH, skipping source download".to_string(),
                code: 0,
            }
            .into());
        }

        Ok(promise::resolve(None))
    }

    pub(crate) fn do_install(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        url: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);
        let cache_path = format!(
            "{}/{}/",
            self.inner
                .config
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or(""),
            Preg::replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(url.to_string())),
        );
        let r#ref = package.get_source_reference().unwrap_or("").to_string();

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

        self.inner
            .io
            .write_error(PhpMixed::String(msg), true, IOInterface::NORMAL);

        self.git_util.run_commands(commands, url, &path, true);

        let source_url = package.get_source_url();
        if url != source_url.unwrap_or("") && source_url.is_some() {
            self.update_origin_url(&path, source_url.unwrap());
        } else {
            self.set_push_url(&path, url);
        }

        if let Some(new_ref) =
            self.update_to_commit(package, &path, &r#ref, package.get_pretty_version())?
        {
            if package.get_dist_reference() == package.get_source_reference() {
                // TODO(phase-b): set_dist_reference requires &mut PackageInterface
                // package.set_dist_reference(Some(new_ref.clone()));
            }
            // package.set_source_reference(Some(new_ref));
            let _ = new_ref;
        }

        Ok(promise::resolve(None))
    }

    pub(crate) fn do_update(
        &mut self,
        _initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
        url: &str,
    ) -> Result<Box<dyn PromiseInterface>> {
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
                .get("cache-vcs-dir")
                .as_string()
                .unwrap_or(""),
            Preg::replace(r"{[^a-z0-9.]}i", "-", Url::sanitize(url.to_string())),
        );
        let r#ref = target.get_source_reference().unwrap_or("").to_string();

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

        self.inner
            .io
            .write_error(PhpMixed::String(msg), true, IOInterface::NORMAL);

        let mut output = String::new();
        if self.inner.process.execute(
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

            self.git_util.run_commands(commands, url, &path, false);
        }

        let command = vec![
            "git".to_string(),
            "remote".to_string(),
            "set-url".to_string(),
            "composer".to_string(),
            "--".to_string(),
            "%sanitizedUrl%".to_string(),
        ];
        self.git_util.run_commands(vec![command], url, &path, false);

        if let Some(new_ref) =
            self.update_to_commit(target, &path, &r#ref, target.get_pretty_version())?
        {
            if target.get_dist_reference() == target.get_source_reference() {
                // TODO(phase-b): set_dist_reference requires &mut PackageInterface
                // target.set_dist_reference(Some(new_ref.clone()));
            }
            // target.set_source_reference(Some(new_ref));
            let _ = new_ref;
        }

        let mut update_origin_url = false;
        let mut output = String::new();
        if self.inner.process.execute(
            &vec!["git".to_string(), "remote".to_string(), "-v".to_string()],
            &mut output,
            Some(path.clone()),
        ) == 0
        {
            let origin_match = Preg::is_match_strict_groups(r"{^origin\s+(?P<url>\S+)}m", &output);
            let composer_match =
                Preg::is_match_strict_groups(r"{^composer\s+(?P<url>\S+)}m", &output);
            if let (Some(origin_match), Some(composer_match)) = (origin_match, composer_match) {
                let origin_url = origin_match.get("url").cloned().unwrap_or_default();
                let composer_url = composer_match.get("url").cloned().unwrap_or_default();
                if origin_url == composer_url
                    && Some(composer_url.as_str()) != target.get_source_url()
                {
                    update_origin_url = true;
                }
            }
        }
        if update_origin_url && target.get_source_url().is_some() {
            self.update_origin_url(&path, target.get_source_url().unwrap());
        }

        Ok(promise::resolve(None))
    }

    pub fn get_local_changes(&self, _package: &dyn PackageInterface, path: &str) -> Option<String> {
        GitUtil::clean_env(&self.inner.process);
        if !self.has_metadata_repository(path) {
            return None;
        }

        let command = vec![
            "git".to_string(),
            "status".to_string(),
            "--porcelain".to_string(),
            "--untracked-files=no".to_string(),
        ];
        let mut output = String::new();
        if self
            .inner
            .process
            .execute(&command, &mut output, Some(path.to_string()))
            != 0
        {
            // TODO(phase-b): cannot throw from &self / non-Result fn; bubble error via Result later
            panic!(
                "{}",
                format!(
                    "Failed to execute {}\n\n{}",
                    implode(" ", &command),
                    self.inner.process.get_error_output(),
                )
            );
        }

        let output = trim(&output, None);

        if strlen(&output) > 0 {
            Some(output)
        } else {
            None
        }
    }

    pub fn get_unpushed_changes(
        &self,
        _package: &dyn PackageInterface,
        path: &str,
    ) -> Option<String> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);
        if !self.has_metadata_repository(&path) {
            return None;
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
            .execute(&command, &mut output, Some(path.clone()))
            != 0
        {
            // TODO(phase-b): bubble error via Result later
            panic!(
                "{}",
                format!(
                    "Failed to execute {}\n\n{}",
                    implode(" ", &command),
                    self.inner.process.get_error_output(),
                )
            );
        }

        let mut refs = trim(&output, None);
        let head_ref = match Preg::is_match_strict_groups(r"{^([a-f0-9]+) HEAD$}mi", &refs) {
            Some(m) => m.get(1).cloned().unwrap_or_default(),
            // could not match the HEAD for some reason
            None => return None,
        };

        let candidate_branches: Vec<String> = match Preg::is_match_all_strict_groups(
            &format!("{{^{} refs/heads/(.+)$}}mi", preg_quote(&head_ref, None)),
            &refs,
        ) {
            Some(m) => m.get(1).cloned().unwrap_or_default(),
            // not on a branch, we are either on a not-modified tag or some sort of detached head, so skip this
            None => return None,
        };

        // use the first match as branch name for now
        let mut branch = candidate_branches[0].clone();
        let mut unpushed_changes: Option<String> = None;
        let mut branch_not_found_error = false;

        // do two passes, as if we find anything we want to fetch and then re-try
        for i in 0..=1 {
            let mut remote_branches: Vec<String> = vec![];

            // try to find matching branch names in remote repos
            for candidate in &candidate_branches {
                if let Some(m) = Preg::is_match_all_strict_groups(
                    &format!(
                        "{{^[a-f0-9]+ refs/remotes/((?:[^/]+)/{})$}}mi",
                        preg_quote(candidate, None)
                    ),
                    &refs,
                ) {
                    let matches: Vec<String> = m.get(1).cloned().unwrap_or_default();
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
                    if self
                        .inner
                        .process
                        .execute(&command, &mut output, Some(path.clone()))
                        != 0
                    {
                        // TODO(phase-b): bubble error via Result later
                        panic!(
                            "{}",
                            format!(
                                "Failed to execute {}\n\n{}",
                                implode(" ", &command),
                                self.inner.process.get_error_output(),
                            )
                        );
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
                self.inner.process.execute(
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
                if self
                    .inner
                    .process
                    .execute(&command, &mut output, Some(path.clone()))
                    != 0
                {
                    // TODO(phase-b): bubble error via Result later
                    panic!(
                        "{}",
                        format!(
                            "Failed to execute {}\n\n{}",
                            implode(" ", &command),
                            self.inner.process.get_error_output(),
                        )
                    );
                }
                refs = trim(&output, None);
            }

            // abort after first pass if we didn't find anything
            if unpushed_changes.is_none() {
                break;
            }
        }

        unpushed_changes
    }

    pub(crate) fn clean_changes(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        update: bool,
    ) -> Result<Box<dyn PromiseInterface>> {
        GitUtil::clean_env(&self.inner.process);
        let path = self.normalize_path(path);

        let unpushed = self.get_unpushed_changes(package, &path);
        if let Some(unpushed) = unpushed.as_deref() {
            if self.inner.io.is_interactive()
                || self.inner.config.get("discard-changes").as_bool() != Some(true)
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

        let changes = match self.get_local_changes(package, &path) {
            Some(c) => c,
            None => return Ok(promise::resolve(None)),
        };

        if !self.inner.io.is_interactive() {
            let discard_changes = self.inner.config.get("discard-changes");
            if discard_changes.as_bool() == Some(true) {
                return self.discard_changes(&path);
            }
            if discard_changes.as_string() == Some("stash") {
                if !update {
                    return self.inner.clean_changes(package, &path, update);
                }

                return self.stash_changes(&path);
            }

            return self.inner.clean_changes(package, &path, update);
        }

        let changes: Vec<String> = array_map(
            |elem: &String| format!("    {}", elem),
            &Preg::split(r"{\s*\r?\n\s*}", &changes),
        );
        self.inner.io.write_error(
            PhpMixed::String(format!(
                "    <error>{} has modified files:</error>",
                package.get_pretty_name()
            )),
            true,
            IOInterface::NORMAL,
        );
        let slice_end = 10_usize.min(changes.len());
        self.inner.io.write_error(
            PhpMixed::List(
                changes[..slice_end]
                    .iter()
                    .map(|s| Box::new(PhpMixed::String(s.clone())))
                    .collect(),
            ),
            true,
            IOInterface::NORMAL,
        );
        if (changes.len() as i64) > 10 {
            self.inner.io.write_error(
                PhpMixed::String(format!(
                    "    <info>{} more files modified, choose \"v\" to view the full list</info>",
                    changes.len() as i64 - 10
                )),
                true,
                IOInterface::NORMAL,
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
                    self.discard_changes(&path)?;
                    break 'outer;
                }
                Some("s") => {
                    if !update {
                        // goto help;
                        do_help = true;
                    } else {
                        self.stash_changes(&path)?;
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
                    self.inner.io.write_error(
                        PhpMixed::List(
                            changes
                                .iter()
                                .map(|s| Box::new(PhpMixed::String(s.clone())))
                                .collect(),
                        ),
                        true,
                        IOInterface::NORMAL,
                    );
                }
                Some("d") => {
                    self.view_diff(&path);
                }
                _ => {
                    // case '?': default:
                    do_help = true;
                }
            }

            if do_help {
                // help:
                self.inner.io.write_error(
                    PhpMixed::List(vec![
                        Box::new(PhpMixed::String(format!(
                            "    y - discard changes and apply the {}",
                            if update { "update" } else { "uninstall" }
                        ))),
                        Box::new(PhpMixed::String(format!(
                            "    n - abort the {} and let you manually clean things up",
                            if update { "update" } else { "uninstall" }
                        ))),
                        Box::new(PhpMixed::String("    v - view modified files".to_string())),
                        Box::new(PhpMixed::String(
                            "    d - view local modifications (diff)".to_string(),
                        )),
                    ]),
                    true,
                    IOInterface::NORMAL,
                );
                if update {
                    self.inner.io.write_error(
                        PhpMixed::String(
                            "    s - stash changes and try to reapply them after the update"
                                .to_string(),
                        ),
                        true,
                        IOInterface::NORMAL,
                    );
                }
                self.inner.io.write_error(
                    PhpMixed::String("    ? - print help".to_string()),
                    true,
                    IOInterface::NORMAL,
                );
            }
        }

        Ok(promise::resolve(None))
    }

    pub(crate) fn reapply_changes(&mut self, path: &str) -> Result<()> {
        let path = self.normalize_path(path);
        if self
            .has_stashed_changes
            .get(&path)
            .copied()
            .unwrap_or(false)
        {
            self.has_stashed_changes.shift_remove(&path);
            self.inner.io.write_error(
                PhpMixed::String("    <info>Re-applying stashed changes</info>".to_string()),
                true,
                IOInterface::NORMAL,
            );
            let mut output = String::new();
            if self.inner.process.execute(
                &vec!["git".to_string(), "stash".to_string(), "pop".to_string()],
                &mut output,
                Some(path.clone()),
            ) != 0
            {
                return Err(RuntimeException {
                    message: format!(
                        "Failed to apply stashed changes:\n\n{}",
                        self.inner.process.get_error_output()
                    ),
                    code: 0,
                }
                .into());
            }
        }

        self.has_discarded_changes.shift_remove(&path);
        Ok(())
    }

    /// Updates the given path to the given commit ref
    ///
    /// @throws \RuntimeException
    /// @return null|string       if a string is returned, it is the commit reference that was checked out if the original could not be found
    pub(crate) fn update_to_commit(
        &mut self,
        package: &dyn PackageInterface,
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

        let mut branch = Preg::replace(
            r"{(?:^dev-|(?:\.x)?-dev$)}i",
            "",
            pretty_version.to_string(),
        );

        // Closure equivalent: $execute = function(array $command) use (&$output, $path) { ... };
        // Inlined below at each call site.

        let mut branches: Option<String> = None;
        {
            let mut output = String::new();
            if self.inner.process.execute(
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
            let ok1 = self
                .inner
                .process
                .execute(&command1, &mut output, Some(path.to_string()))
                == 0;
            let ok2 = if ok1 {
                let mut output = String::new();
                self.inner
                    .process
                    .execute(&command2, &mut output, Some(path.to_string()))
                    == 0
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
            let ok_command =
                self.inner
                    .process
                    .execute(&command, &mut output, Some(path.to_string()))
                    == 0;
            let ok_fallback = if !ok_command {
                let mut output = String::new();
                self.inner
                    .process
                    .execute(&fallback_command, &mut output, Some(path.to_string()))
                    == 0
            } else {
                false
            };
            let ok_reset = if ok_command || ok_fallback {
                let mut output = String::new();
                self.inner
                    .process
                    .execute(&reset_command, &mut output, Some(path.to_string()))
                    == 0
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
            let ok1 = self
                .inner
                .process
                .execute(&command1, &mut output, Some(path.to_string()))
                == 0;
            let ok2 = if ok1 {
                let mut output = String::new();
                self.inner
                    .process
                    .execute(&command2, &mut output, Some(path.to_string()))
                    == 0
            } else {
                false
            };
            if ok1 && ok2 {
                return Ok(None);
            }
        }

        let mut exception_extra = String::new();

        // reference was not found (prints "fatal: reference is not a tree: $ref")
        if strpos(self.inner.process.get_error_output(), reference).is_some() {
            self.inner.io.write_error(
                PhpMixed::String(format!(
                    "    <warning>{} is gone (history was rewritten?)</warning>",
                    reference
                )),
                true,
                IOInterface::NORMAL,
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
                self.inner.process.get_error_output(),
                exception_extra,
            )),
            code: 0,
        }
        .into())
    }

    pub(crate) fn update_origin_url(&mut self, path: &str, url: &str) {
        let mut output = String::new();
        self.inner.process.execute(
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
        if let Some(match_) = Preg::is_match_strict_groups(
            &format!(
                "{{^(?:https?|git)://{}/([^/]+)/([^/]+?)(?:\\.git)?$}}",
                GitUtil::get_github_domains_regex(&self.inner.config)
            ),
            url,
        ) {
            let protocols = self.inner.config.get("github-protocols");
            let m1 = match_.get(1).cloned().unwrap_or_default();
            let m2 = match_.get(2).cloned().unwrap_or_default();
            let m3 = match_.get(3).cloned().unwrap_or_default();
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
            self.inner
                .process
                .execute(&cmd, &mut ignored_output, Some(path.to_string()));
        }
    }

    pub(crate) fn get_commit_logs(
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
            .execute(&command, &mut output, Some(path.clone()))
            != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Failed to execute {}\n\n{}",
                    implode(" ", &command),
                    self.inner.process.get_error_output(),
                ),
                code: 0,
            }
            .into());
        }

        Ok(GitUtil::parse_rev_list_output(&output, &self.inner.process))
    }

    /// @phpstan-return PromiseInterface<void|null>
    /// @throws \RuntimeException
    pub(crate) fn discard_changes(&mut self, path: &str) -> Result<Box<dyn PromiseInterface>> {
        let path = self.normalize_path(path);
        let mut output = String::new();
        if self.inner.process.execute(
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
        if self.inner.process.execute(
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

        Ok(promise::resolve(None))
    }

    /// @phpstan-return PromiseInterface<void|null>
    /// @throws \RuntimeException
    pub(crate) fn stash_changes(&mut self, path: &str) -> Result<Box<dyn PromiseInterface>> {
        let path = self.normalize_path(path);
        let mut output = String::new();
        if self.inner.process.execute(
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

        Ok(promise::resolve(None))
    }

    /// @throws \RuntimeException
    pub(crate) fn view_diff(&mut self, path: &str) {
        let path = self.normalize_path(path);
        let mut output = String::new();
        if self.inner.process.execute(
            &vec!["git".to_string(), "diff".to_string(), "HEAD".to_string()],
            &mut output,
            Some(path.clone()),
        ) != 0
        {
            // TODO(phase-b): cannot throw from non-Result fn; bubble error via Result later
            panic!("{}", format!("Could not view diff\n\n:{}", output));
        }

        self.inner
            .io
            .write_error(PhpMixed::String(output), true, IOInterface::NORMAL);
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

    pub(crate) fn has_metadata_repository(&self, path: &str) -> bool {
        let path = self.normalize_path(path);

        is_dir(&format!("{}/.git", path))
    }

    pub(crate) fn get_short_hash(&self, reference: &str) -> String {
        if !self.inner.io.is_verbose()
            && Preg::is_match(r"{^[0-9a-f]{40}$}", reference).unwrap_or(false)
        {
            return substr(reference, 0, Some(10));
        }

        reference.to_string()
    }
}

impl DvcsDownloaderInterface for GitDownloader {
    fn get_unpushed_changes(&self, package: &dyn PackageInterface, path: String) -> Option<String> {
        GitDownloader::get_unpushed_changes(self, package, &path)
    }
}
