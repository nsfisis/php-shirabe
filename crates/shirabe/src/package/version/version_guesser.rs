//! ref: composer/src/Composer/Package/Version/VersionGuesser.php

use crate::config::Config;
use crate::io::{IOInterface, NullIO};
use crate::package::version::VersionParser;
use crate::repository::vcs::HgDriver;
use crate::util::{
    Git as GitUtil, HttpDownloader, Platform, ProcessExecutor, Svn as SvnUtil, sync_executor,
};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, RuntimeException, array_map, bconcat, empty, implode, is_string, json_encode,
    path_to_bytes, php_regex, preg_is_match, preg_match, preg_quote, preg_replace, str_replace,
    strlen, strnatcasecmp, strpos, substr, trim, usort,
};
use std::path::Path;

/// Seam over the parts of [`VersionGuesser`] that consumers depend on, so they can be exercised
/// with a test double. PHP has no such interface; this exists only to allow mocking the
/// concrete `VersionGuesser`.
pub trait VersionGuesserInterface: std::fmt::Debug {
    fn guess_version(
        &mut self,
        package_config: &IndexMap<BString, PhpMixed>,
        path: &Path,
    ) -> anyhow::Result<Option<VersionData>>;

    fn get_root_version_from_env(&self) -> anyhow::Result<BString>;
}

/// Try to guess the current version number based on different VCS configuration.
#[derive(Debug)]
pub struct VersionGuesser {
    config: std::rc::Rc<std::cell::RefCell<Config>>,

    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,

    version_parser: VersionParser,

    io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,

    /// For testing only: when true, `guess_version` always returns `None`, mirroring
    /// `Composer\Test\Mock\VersionGuesserMock`. `false` in production.
    mock: bool,
}

#[derive(Debug, Clone)]
pub struct VersionData {
    pub version: Option<BString>,
    pub commit: Option<BString>,
    pub pretty_version: Option<BString>,
    pub feature_version: Option<BString>,
    pub feature_pretty_version: Option<BString>,
}

impl VersionGuesser {
    pub fn new(
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        version_parser: VersionParser,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    ) -> Self {
        Self {
            config,
            process,
            version_parser,
            io,
            mock: false,
        }
    }

    /// For testing only: builds a guesser whose `guess_version` always returns `None`, mirroring
    /// `Composer\Test\Mock\VersionGuesserMock`.
    pub fn __new_mock(
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        version_parser: VersionParser,
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
    ) -> Self {
        Self {
            mock: true,
            ..Self::new(config, process, version_parser, io)
        }
    }

    pub fn guess_version(
        &mut self,
        package_config: &IndexMap<BString, PhpMixed>,
        path: &Path,
    ) -> anyhow::Result<Option<VersionData>> {
        // For testing only (ref VersionGuesserMock::guessVersion returns null).
        if self.mock {
            return Ok(None);
        }

        // bypass version guessing in bash completions as it takes time to create
        // new processes and the root version is usually not that important
        if Platform::is_input_completion_process() {
            return Ok(None);
        }

        let version_data = self.guess_git_version(package_config, path)?;
        if version_data.version.is_some() {
            return Ok(Some(self.postprocess(version_data)));
        }

        let version_data = self.guess_hg_version(package_config, path)?;
        if let Some(vd) = version_data
            && vd.version.is_some()
        {
            return Ok(Some(self.postprocess(vd)));
        }

        let version_data = self.guess_fossil_version(path)?;
        if version_data.version.is_some() {
            return Ok(Some(self.postprocess(version_data)));
        }

        let version_data = self.guess_svn_version(package_config, path)?;
        if let Some(vd) = version_data
            && vd.version.is_some()
        {
            return Ok(Some(self.postprocess(vd)));
        }

        Ok(None)
    }

    fn postprocess(&self, mut version_data: VersionData) -> VersionData {
        // PHP: !empty($versionData['feature_version']) && $versionData['feature_version'] === $versionData['version'] && $versionData['feature_pretty_version'] === $versionData['pretty_version']
        let feature_matches = version_data
            .feature_version
            .as_ref()
            .map(|fv| !fv.is_empty())
            .unwrap_or(false)
            && version_data.feature_version == version_data.version
            && version_data.feature_pretty_version == version_data.pretty_version;
        if feature_matches {
            version_data.feature_version = None;
            version_data.feature_pretty_version = None;
        }

        if "-dev"
            == substr(
                version_data.version.as_ref().map_or(B(""), |v| v.as_bstr()),
                -4,
                None,
            )
            && preg_is_match(
                php_regex!(r"{\.9{7}}"),
                version_data.version.as_ref().map_or(B(""), |v| v.as_bstr()),
            )
        {
            version_data.pretty_version = Some(preg_replace(
                php_regex!(r"{(\.9{7})+}"),
                ".x",
                version_data.version.as_ref().map_or(B(""), |v| v.as_bstr()),
            ));
        }

        let feature_non_empty = version_data
            .feature_version
            .as_ref()
            .map(|fv| !fv.is_empty())
            .unwrap_or(false);
        if feature_non_empty
            && "-dev"
                == substr(
                    version_data
                        .feature_version
                        .as_ref()
                        .map_or(B(""), |v| v.as_bstr()),
                    -4,
                    None,
                )
            && preg_is_match(
                php_regex!(r"{\.9{7}}"),
                version_data
                    .feature_version
                    .as_ref()
                    .map_or(B(""), |v| v.as_bstr()),
            )
        {
            version_data.feature_pretty_version = Some(preg_replace(
                php_regex!(r"{(\.9{7})+}"),
                ".x",
                version_data
                    .feature_version
                    .as_ref()
                    .map_or(B(""), |v| v.as_bstr()),
            ));
        }

        version_data
    }

    fn guess_git_version(
        &mut self,
        package_config: &IndexMap<BString, PhpMixed>,
        path: &Path,
    ) -> anyhow::Result<VersionData> {
        GitUtil::clean_env(&self.process);
        let mut commit: Option<BString> = None;
        let mut version: Option<BString> = None;
        let mut pretty_version: Option<BString> = None;
        let mut feature_version: Option<BString> = None;
        let mut feature_pretty_version: Option<BString> = None;
        let mut is_detached = false;

        // try to fetch current version from git branch
        let mut output = BString::default();
        if 0 == self.process.borrow_mut().execute(
            &["git", "branch", "-a", "--no-color", "--no-abbrev", "-v"],
            &mut output,
            Some(path),
        )? {
            let mut branches: Vec<BString> = vec![];
            let mut is_feature_branch = false;

            // find current branch and collect all branch names
            for branch in self.process.borrow().split_lines(&output) {
                if !branch.is_empty()
                    && let Some(m) = preg_match(
                        php_regex!(
                            r"{^(?:\* ) *(\(no branch\)|\(detached from \S+\)|\(HEAD detached at \S+\)|\S+) *([a-f0-9]+) .*$}"
                        ),
                        &branch,
                    )
                {
                    let g1 = m.get(1).unwrap_or_default();
                    let g2 = BString::from(m.get(2).unwrap_or_default());
                    if g1 == "(no branch)"
                        || strpos(g1, "(detached ") == Some(0)
                        || strpos(g1, "(HEAD detached at") == Some(0)
                    {
                        version = Some(bconcat!("dev-", g2));
                        pretty_version = version.clone();
                        is_feature_branch = true;
                        is_detached = true;
                    } else {
                        version = Some(self.version_parser.normalize_branch(g1)?);
                        pretty_version = Some(bconcat!("dev-", g1));
                        is_feature_branch = self.is_feature_branch(package_config, Some(g1));
                    }

                    commit = Some(g2);
                }

                if !branch.is_empty()
                    && !preg_is_match(php_regex!(r"{^ *.+/HEAD }"), &branch)
                    && let Some(m) = preg_match(
                        php_regex!(
                            r"{^(?:\* )? *((?:remotes/(?:origin|upstream)/)?[^\s/]+) *([a-f0-9]+) .*$}"
                        ),
                        &branch,
                    )
                {
                    branches.push(m.get(1).unwrap_or_default().into());
                }
            }

            if is_feature_branch {
                feature_version = version.clone();
                feature_pretty_version = pretty_version.clone();

                // try to find the best (nearest) version branch to assume this feature's version
                let result = self.guess_feature_version(
                    package_config,
                    version.clone(),
                    branches,
                    vec![
                        "git".into(),
                        "rev-list".into(),
                        "%candidate%..%branch%".into(),
                    ],
                    path,
                )?;
                version = result.version;
                pretty_version = result.pretty_version;
            }
        }
        GitUtil::check_for_repo_ownership_error(
            self.process.borrow().get_error_output().as_bstr(),
            path,
            self.io.clone(),
        );

        if version.is_none() || is_detached {
            let result = self.version_from_git_tags(path)?;
            if let Some(r) = result {
                version = Some(r.0);
                pretty_version = Some(r.1);
                feature_version = None;
                feature_pretty_version = None;
            }
        }

        if commit.is_none() {
            let mut arguments: Vec<BString> =
                vec!["--format=%H".into(), "-n1".into(), "HEAD".into()];
            arguments.extend(GitUtil::get_no_show_signature_flags(&self.process)?);
            let command = GitUtil::build_rev_list_command(&self.process, arguments)?;
            let mut command_output = BString::default();
            if 0 == self
                .process
                .borrow_mut()
                .execute(&command, &mut command_output, Some(path))?
            {
                let parsed = trim(GitUtil::parse_rev_list_output(
                    &command_output,
                    &self.process,
                )?);
                commit = if parsed.is_empty() {
                    None
                } else {
                    Some(parsed)
                };
            }
        }

        Ok(VersionData {
            version,
            commit,
            pretty_version,
            feature_version,
            feature_pretty_version,
        })
    }

    fn version_from_git_tags(&mut self, path: &Path) -> anyhow::Result<Option<(BString, BString)>> {
        // try to fetch current version from git tags
        let mut output = BString::default();
        if 0 == self.process.borrow_mut().execute(
            &["git", "describe", "--exact-match", "--tags"],
            &mut output,
            Some(path),
        )? {
            match self.version_parser.normalize(trim(&output), None) {
                Ok(version) => {
                    return Ok(Some((version, trim(&output))));
                }
                Err(_e) => {}
            }
        }

        Ok(None)
    }

    fn guess_hg_version(
        &mut self,
        package_config: &IndexMap<BString, PhpMixed>,
        path: &Path,
    ) -> anyhow::Result<Option<VersionData>> {
        // try to fetch current version from hg branch
        let mut output = BString::default();
        if 0 == self
            .process
            .borrow_mut()
            .execute(&["hg", "branch"], &mut output, Some(path))?
        {
            let branch = trim(&output);
            let version = self.version_parser.normalize_branch(&branch)?;
            let is_feature_branch = strpos(&version, "dev-") == Some(0);

            if VersionParser::DEFAULT_BRANCH_ALIAS == version {
                return Ok(Some(VersionData {
                    version: Some(version),
                    commit: None,
                    pretty_version: Some(bconcat!("dev-", branch)),
                    feature_version: None,
                    feature_pretty_version: None,
                }));
            }

            if !is_feature_branch {
                return Ok(Some(VersionData {
                    version: Some(version.clone()),
                    commit: None,
                    pretty_version: Some(version),
                    feature_version: None,
                    feature_pretty_version: None,
                }));
            }

            // re-use the HgDriver to fetch branches (this properly includes bookmarks)
            let mut repo_config: IndexMap<BString, PhpMixed> = IndexMap::new();
            repo_config.insert("url".into(), PhpMixed::string(path_to_bytes(path)));
            let io: std::rc::Rc<std::cell::RefCell<dyn crate::io::IOInterface>> =
                std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
            let http_io: std::rc::Rc<std::cell::RefCell<dyn crate::io::IOInterface>> =
                std::rc::Rc::new(std::cell::RefCell::new(NullIO::new()));
            let http_downloader = std::rc::Rc::new(std::cell::RefCell::new(HttpDownloader::new(
                http_io,
                self.config.clone(),
                IndexMap::new(),
                false,
            )));
            let mut driver = HgDriver::new(
                repo_config,
                io,
                self.config.clone(),
                http_downloader,
                std::rc::Rc::clone(&self.process),
            );
            let branches: Vec<BString> = driver.get_branches()?.keys().cloned().collect();

            // try to find the best (nearest) version branch to assume this feature's version
            let result = self.guess_feature_version(
                package_config,
                Some(version.clone()),
                branches,
                vec![
                    "hg".into(),
                    "log".into(),
                    "-r".into(),
                    "not ancestors('%candidate%') and ancestors('%branch%')".into(),
                    "--template".into(),
                    "\"{node}\\n\"".into(),
                ],
                path,
            )?;
            let commit = Some(BString::default());
            let feature_version = Some(version.clone());
            let feature_pretty_version = Some(version);

            return Ok(Some(VersionData {
                version: result.version,
                commit,
                pretty_version: result.pretty_version,
                feature_version,
                feature_pretty_version,
            }));
        }

        Ok(None)
    }

    fn guess_feature_version(
        &mut self,
        package_config: &IndexMap<BString, PhpMixed>,
        version: Option<BString>,
        mut branches: Vec<BString>,
        scm_cmdline: Vec<BString>,
        path: &Path,
    ) -> anyhow::Result<FeatureVersionResult> {
        let mut pretty_version: Option<BString> = version.clone();
        let mut version = version;

        // ignore feature branches if they have no branch-alias or self.version is used
        // and find the branch they came from to use as a version instead
        let has_branch_alias = package_config
            .get(B("extra"))
            .and_then(|v| v.as_array())
            .and_then(|m| m.get(B("branch-alias")))
            .and_then(|v| v.as_array())
            .map(|m| m.contains_key(version.as_ref().map_or(B(""), |v| v.as_bstr())))
            .unwrap_or(false);
        let has_self_version = strpos(
            json_encode(&PhpMixed::Array(
                package_config
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ))
            .unwrap_or_default(),
            "\"self.version\"",
        )
        .is_some();
        if !has_branch_alias || has_self_version {
            let branch = preg_replace(
                php_regex!(r"{^dev-}"),
                "",
                version.as_ref().map_or(B(""), |v| v.as_bstr()),
            );
            let mut length = i64::MAX;

            // return directly, if branch is configured to be non-feature branch
            if !self.is_feature_branch(package_config, Some(branch.as_bstr())) {
                return Ok(FeatureVersionResult {
                    version,
                    pretty_version,
                });
            }

            // sort local branches first then remote ones
            // and sort numeric branches below named ones, to make sure if the branch has the same distance from main and 1.10 and 1.9 for example, 1.9 is picked
            // and sort using natural sort so that 1.10 will appear before 1.9
            usort(&mut branches, |a: &BString, b: &BString| -> i64 {
                let a_remote = strpos(a, "remotes/") == Some(0);
                let b_remote = strpos(b, "remotes/") == Some(0);

                if a_remote != b_remote {
                    return if a_remote { 1 } else { -1 };
                }

                strnatcasecmp(b, a)
            });

            self.process.borrow_mut().set_max_jobs(30);
            // PHP runs every candidate diff in parallel and cancels the siblings once a zero-length
            // diff is found; the single-threaded sync bridge block_on's each diff serially and stops
            // at the first zero-length match.
            let result: anyhow::Result<()> = (|| -> anyhow::Result<()> {
                let mut last_index: i64 = -1;
                for (index, candidate) in branches.iter().enumerate() {
                    let index = index as i64;
                    let candidate_version =
                        preg_replace(php_regex!(r"{^remotes/\S+/}"), "", candidate);

                    // do not compare against itself or other feature branches
                    if candidate == &branch
                        || self.is_feature_branch(package_config, Some(candidate_version.as_bstr()))
                    {
                        continue;
                    }

                    let cmd_line: Vec<BString> = array_map(
                        |component: &BString| -> BString {
                            let r1 = str_replace("%candidate%", candidate, component);
                            str_replace("%branch%", &branch, &r1)
                        },
                        &scm_cmdline,
                    );
                    let process_future = self
                        .process
                        .borrow_mut()
                        .execute_async(&cmd_line, Some(path));
                    let mut process = sync_executor::block_on(process_future)?;
                    if !process.is_successful() {
                        continue;
                    }

                    let output = process.get_output()?;
                    // overwrite existing if we have a shorter diff, or we have an equal diff and an index that comes later in the array (i.e. older version)
                    // as newer versions typically have more commits, if the feature branch is based on a newer branch it should have a longer diff to the old version
                    // but if it doesn't and they have equal diffs, then it probably is based on the old version
                    if strlen(&output) < length || (strlen(&output) == length && last_index < index)
                    {
                        last_index = index;
                        length = strlen(&output);
                        version = Some(self.version_parser.normalize_branch(&candidate_version)?);
                        pretty_version = Some(bconcat!("dev-", candidate_version));
                        if length == 0 {
                            break;
                        }
                    }
                }

                Ok(())
            })();
            self.process.borrow_mut().reset_max_jobs();
            result?;
        }

        Ok(FeatureVersionResult {
            version,
            pretty_version,
        })
    }

    fn is_feature_branch(
        &self,
        package_config: &IndexMap<BString, PhpMixed>,
        branch_name: Option<&BStr>,
    ) -> bool {
        let mut non_feature_branches = BString::default();
        let nf_value = package_config.get(B("non-feature-branches"));
        if !empty(&nf_value.cloned().unwrap_or(PhpMixed::Null)) {
            let names: Vec<BString> = nf_value
                .and_then(|v| v.as_list())
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(BString::from))
                        .collect()
                })
                .unwrap_or_default();
            non_feature_branches = implode("|", &names);
        }

        !preg_is_match(
            bconcat!(
                "{^(",
                non_feature_branches,
                r"|master|main|latest|next|current|support|tip|trunk|default|develop|\d+\..+)$}",
            ),
            branch_name.unwrap_or_default(),
        )
    }

    fn guess_fossil_version(&mut self, path: &Path) -> anyhow::Result<VersionData> {
        let mut version: Option<BString> = None;
        let mut pretty_version: Option<BString> = None;

        // try to fetch current version from fossil
        let mut output = BString::default();
        if 0 == self.process.borrow_mut().execute(
            &["fossil", "branch", "list"],
            &mut output,
            Some(path),
        )? {
            let branch = trim(&output);
            version = Some(self.version_parser.normalize_branch(&branch)?);
            pretty_version = Some(bconcat!("dev-", branch));
        }

        // try to fetch current version from fossil tags
        let mut output = BString::default();
        if 0 == self.process.borrow_mut().execute(
            &["fossil", "tag", "list"],
            &mut output,
            Some(path),
        )? {
            match self.version_parser.normalize(trim(&output), None) {
                Ok(v) => {
                    version = Some(v);
                    pretty_version = Some(trim(&output));
                }
                Err(_e) => {}
            }
        }

        Ok(VersionData {
            version,
            commit: Some(BString::default()),
            pretty_version,
            feature_version: None,
            feature_pretty_version: None,
        })
    }

    fn guess_svn_version(
        &mut self,
        package_config: &IndexMap<BString, PhpMixed>,
        path: &Path,
    ) -> anyhow::Result<Option<VersionData>> {
        SvnUtil::clean_env();

        // try to fetch current version from svn
        let mut output = BString::default();
        if 0 == self.process.borrow_mut().execute(
            &["svn", "info", "--xml"],
            &mut output,
            Some(path),
        )? {
            let trunk_path = package_config
                .get(B("trunk-path"))
                .and_then(|v| v.as_string())
                .map(|s| preg_quote(s, Some('#')))
                .unwrap_or_else(|| "trunk".to_string());
            let branches_path = package_config
                .get(B("branches-path"))
                .and_then(|v| v.as_string())
                .map(|s| preg_quote(s, Some('#')))
                .unwrap_or_else(|| "branches".to_string());
            let tags_path = package_config
                .get(B("tags-path"))
                .and_then(|v| v.as_string())
                .map(|s| preg_quote(s, Some('#')))
                .unwrap_or_else(|| "tags".to_string());

            let url_pattern = format!(
                "#<url>.*/({}|({}|{})/(.*))</url>#",
                trunk_path, branches_path, tags_path,
            );

            if let Some(matches) = preg_match(&url_pattern, &output) {
                let m1 = matches.get(1).unwrap_or_default();
                let m2 = matches.get(2);
                let m3 = matches.get(3);
                if let Some(m2) = m2
                    && let Some(m3) = m3
                    && (branches_path.as_bytes() == m2.as_bytes()
                        || tags_path.as_bytes() == m2.as_bytes())
                {
                    // we are in a branches path
                    let version = self.version_parser.normalize_branch(m3)?;
                    let pretty_version = bconcat!("dev-", m3);

                    return Ok(Some(VersionData {
                        version: Some(version),
                        commit: Some(BString::default()),
                        pretty_version: Some(pretty_version),
                        feature_version: None,
                        feature_pretty_version: None,
                    }));
                }

                assert!(is_string(&PhpMixed::string(m1)));
                let pretty_version = trim(m1);
                let version = if pretty_version == "trunk" {
                    BString::from("dev-trunk")
                } else {
                    self.version_parser.normalize(&pretty_version, None)?
                };

                return Ok(Some(VersionData {
                    version: Some(version),
                    commit: Some(BString::default()),
                    pretty_version: Some(pretty_version),
                    feature_version: None,
                    feature_pretty_version: None,
                }));
            }
        }

        Ok(None)
    }

    pub fn get_root_version_from_env(&self) -> anyhow::Result<BString> {
        let version = Platform::get_env("COMPOSER_ROOT_VERSION");
        let version = match version {
            Some(v) if !v.is_empty() => BString::from(v),
            _ => {
                return Err(RuntimeException::new(
                    "COMPOSER_ROOT_VERSION not set or empty".to_string(),
                )
                .into());
            }
        };
        if let Some(m) = preg_match(php_regex!(r"{^(\d+(?:\.\d+)*)-dev$}i"), &version) {
            return Ok(bconcat!(m.get(1).unwrap_or_default(), ".x-dev"));
        }

        Ok(version)
    }
}

impl VersionGuesserInterface for VersionGuesser {
    fn guess_version(
        &mut self,
        package_config: &IndexMap<BString, PhpMixed>,
        path: &Path,
    ) -> anyhow::Result<Option<VersionData>> {
        VersionGuesser::guess_version(self, package_config, path)
    }

    fn get_root_version_from_env(&self) -> anyhow::Result<BString> {
        VersionGuesser::get_root_version_from_env(self)
    }
}

#[derive(Debug)]
pub struct FeatureVersionResult {
    pub version: Option<BString>,
    pub pretty_version: Option<BString>,
}
