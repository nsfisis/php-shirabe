//! ref: composer/src/Composer/Package/Version/VersionGuesser.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::process::process::Process;
use shirabe_php_shim::{
    PHP_INT_MAX, PhpMixed, RuntimeException, array_keys, array_map, array_merge, empty,
    function_exists, implode, is_string, json_encode, preg_quote, str_replace, strlen,
    strnatcasecmp, strpos, substr, trim, usort,
};
use shirabe_semver::version_parser::VersionParser as SemverVersionParser;

use crate::config::Config;
use crate::io::io_interface::IOInterface;
use crate::io::null_io::NullIO;
use crate::package::version::version_parser::VersionParser;
use crate::repository::vcs::hg_driver::HgDriver;
use crate::util::git::Git as GitUtil;
use crate::util::http_downloader::HttpDownloader;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;
use crate::util::svn::Svn as SvnUtil;

/// Try to guess the current version number based on different VCS configuration.
///
/// @phpstan-type Version array{version: string, commit: string|null, pretty_version: string|null}|array{version: string, commit: string|null, pretty_version: string|null, feature_version: string|null, feature_pretty_version: string|null}
#[derive(Debug)]
pub struct VersionGuesser {
    /// @var Config
    config: Config,

    /// @var ProcessExecutor
    process: ProcessExecutor,

    /// @var SemverVersionParser
    version_parser: SemverVersionParser,

    /// @var IOInterface|null
    io: Option<Box<dyn IOInterface>>,
}

/// PHP: @phpstan-type Version array{version, commit, pretty_version, feature_version?, feature_pretty_version?}
#[derive(Debug, Clone)]
pub struct VersionData {
    pub version: Option<String>,
    pub commit: Option<String>,
    pub pretty_version: Option<String>,
    pub feature_version: Option<String>,
    pub feature_pretty_version: Option<String>,
}

impl VersionGuesser {
    pub fn new(
        config: Config,
        process: ProcessExecutor,
        version_parser: SemverVersionParser,
        io: Option<Box<dyn IOInterface>>,
    ) -> Self {
        Self {
            config,
            process,
            version_parser,
            io,
        }
    }

    /// @param array<string, mixed> $packageConfig
    /// @param string               $path Path to guess into
    ///
    /// @phpstan-return Version|null
    pub fn guess_version(
        &mut self,
        package_config: &IndexMap<String, PhpMixed>,
        path: &str,
    ) -> Result<Option<VersionData>> {
        if !function_exists("proc_open") {
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
        if let Some(vd) = version_data {
            if vd.version.is_some() {
                return Ok(Some(self.postprocess(vd)));
            }
        }

        let version_data = self.guess_fossil_version(path)?;
        if version_data.version.is_some() {
            return Ok(Some(self.postprocess(version_data)));
        }

        let version_data = self.guess_svn_version(package_config, path)?;
        if let Some(vd) = version_data {
            if vd.version.is_some() {
                return Ok(Some(self.postprocess(vd)));
            }
        }

        Ok(None)
    }

    /// @phpstan-param Version $versionData
    ///
    /// @phpstan-return Version
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

        if "-dev" == substr(version_data.version.as_deref().unwrap_or(""), -4, None)
            && Preg::is_match(r"{\.9{7}}", version_data.version.as_deref().unwrap_or(""))
        {
            version_data.pretty_version = Some(Preg::replace(
                r"{(\.9{7})+}",
                ".x",
                version_data.version.as_deref().unwrap_or(""),
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
                    version_data.feature_version.as_deref().unwrap_or(""),
                    -4,
                    None,
                )
            && Preg::is_match(
                r"{\.9{7}}",
                version_data.feature_version.as_deref().unwrap_or(""),
            )
        {
            version_data.feature_pretty_version = Some(Preg::replace(
                r"{(\.9{7})+}",
                ".x",
                version_data.feature_version.as_deref().unwrap_or(""),
            ));
        }

        version_data
    }

    /// @param array<string, mixed> $packageConfig
    ///
    /// @return array{version: string|null, commit: string|null, pretty_version: string|null, feature_version?: string|null, feature_pretty_version?: string|null}
    fn guess_git_version(
        &mut self,
        package_config: &IndexMap<String, PhpMixed>,
        path: &str,
    ) -> Result<VersionData> {
        GitUtil::clean_env(&mut self.process);
        let mut commit: Option<String> = None;
        let mut version: Option<String> = None;
        let mut pretty_version: Option<String> = None;
        let mut feature_version: Option<String> = None;
        let mut feature_pretty_version: Option<String> = None;
        let mut is_detached = false;

        // try to fetch current version from git branch
        let mut output = String::new();
        if 0 == self.process.execute(
            &[
                "git".to_string(),
                "branch".to_string(),
                "-a".to_string(),
                "--no-color".to_string(),
                "--no-abbrev".to_string(),
                "-v".to_string(),
            ],
            &mut output,
            Some(path.to_string()),
        ) {
            let mut branches: Vec<String> = vec![];
            let mut is_feature_branch = false;

            // find current branch and collect all branch names
            for branch in self.process.split_lines(&output) {
                if !branch.is_empty() {
                    if let Some(m) = Preg::is_match_strict_groups(
                        r"{^(?:\* ) *(\(no branch\)|\(detached from \S+\)|\(HEAD detached at \S+\)|\S+) *([a-f0-9]+) .*$}",
                        &branch,
                    ) {
                        let g1 = m.get(1).cloned().unwrap_or_default();
                        let g2 = m.get(2).cloned().unwrap_or_default();
                        if g1 == "(no branch)"
                            || strpos(&g1, "(detached ") == Some(0)
                            || strpos(&g1, "(HEAD detached at") == Some(0)
                        {
                            version = Some(format!("dev-{}", g2));
                            pretty_version = version.clone();
                            is_feature_branch = true;
                            is_detached = true;
                        } else {
                            version = Some(self.version_parser.normalize_branch(&g1));
                            pretty_version = Some(format!("dev-{}", g1));
                            is_feature_branch = self.is_feature_branch(package_config, Some(&g1));
                        }

                        commit = Some(g2);
                    }
                }

                if !branch.is_empty()
                    && Preg::is_match_strict_groups(r"{^ *.+/HEAD }", &branch).is_none()
                {
                    if let Some(m) = Preg::is_match_strict_groups(
                        r"{^(?:\* )? *((?:remotes/(?:origin|upstream)/)?[^\s/]+) *([a-f0-9]+) .*$}",
                        &branch,
                    ) {
                        branches.push(m.get(1).cloned().unwrap_or_default());
                    }
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
                        "git".to_string(),
                        "rev-list".to_string(),
                        "%candidate%..%branch%".to_string(),
                    ],
                    path,
                )?;
                version = result.version;
                pretty_version = result.pretty_version;
            }
        }
        GitUtil::check_for_repo_ownership_error(
            &self.process.get_error_output(),
            path,
            self.io.as_deref(),
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
            let command = GitUtil::build_rev_list_command(
                &self.process,
                array_merge(
                    PhpMixed::List(vec![
                        Box::new(PhpMixed::String("--format=%H".to_string())),
                        Box::new(PhpMixed::String("-n1".to_string())),
                        Box::new(PhpMixed::String("HEAD".to_string())),
                    ]),
                    GitUtil::get_no_show_signature_flags(&self.process),
                )
                .as_list()
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            );
            let mut command_output = String::new();
            if 0 == self
                .process
                .execute(&command, &mut command_output, Some(path.to_string()))
            {
                let parsed = trim(
                    &GitUtil::parse_rev_list_output(&command_output, &self.process),
                    None,
                );
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

    /// @return array{version: string, pretty_version: string}|null
    fn version_from_git_tags(&mut self, path: &str) -> Result<Option<(String, String)>> {
        // try to fetch current version from git tags
        let mut output = String::new();
        if 0 == self.process.execute(
            &[
                "git".to_string(),
                "describe".to_string(),
                "--exact-match".to_string(),
                "--tags".to_string(),
            ],
            &mut output,
            Some(path.to_string()),
        ) {
            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            match self.version_parser.normalize(&trim(&output, None), None) {
                Ok(version) => return Ok(Some((version, trim(&output, None)))),
                Err(_e) => {}
            }
        }

        Ok(None)
    }

    /// @param array<string, mixed> $packageConfig
    ///
    /// @return array{version: string|null, commit: ''|null, pretty_version: string|null, feature_version?: string|null, feature_pretty_version?: string|null}|null
    fn guess_hg_version(
        &mut self,
        package_config: &IndexMap<String, PhpMixed>,
        path: &str,
    ) -> Result<Option<VersionData>> {
        // try to fetch current version from hg branch
        let mut output = String::new();
        if 0 == self.process.execute(
            &["hg".to_string(), "branch".to_string()],
            &mut output,
            Some(path.to_string()),
        ) {
            let branch = trim(&output, None);
            let version = self.version_parser.normalize_branch(&branch);
            let is_feature_branch = strpos(&version, "dev-") == Some(0);

            if VersionParser::DEFAULT_BRANCH_ALIAS == version {
                return Ok(Some(VersionData {
                    version: Some(version.clone()),
                    commit: None,
                    pretty_version: Some(format!("dev-{}", branch)),
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
            let io = NullIO::new();
            let mut repo_config: IndexMap<String, PhpMixed> = IndexMap::new();
            repo_config.insert("url".to_string(), PhpMixed::String(path.to_string()));
            let mut driver = HgDriver::new(
                repo_config,
                // TODO(phase-b): NullIO -> Box<dyn IOInterface>
                Box::new(io),
                self.config.clone(),
                // TODO(phase-b): HttpDownloader::new signature
                todo!("HttpDownloader::new(io, config)"),
                // TODO(phase-b): clone ProcessExecutor
                todo!("self.process.clone()"),
            );
            let branches: Vec<String> =
                array_map(|k: &String| k.clone(), &array_keys(driver.get_branches()));

            // try to find the best (nearest) version branch to assume this feature's version
            let mut result = self.guess_feature_version(
                package_config,
                Some(version.clone()),
                branches,
                vec![
                    "hg".to_string(),
                    "log".to_string(),
                    "-r".to_string(),
                    "not ancestors('%candidate%') and ancestors('%branch%')".to_string(),
                    "--template".to_string(),
                    "\"{node}\\n\"".to_string(),
                ],
                path,
            )?;
            // PHP: $result['commit'] = '';
            // TODO(phase-b): VersionData::commit modeled as Option<String>; using Some(String::new())
            let commit = Some(String::new());
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

    /// @param array<string, mixed>     $packageConfig
    /// @param list<string>             $branches
    /// @param list<string>             $scmCmdline
    ///
    /// @return array{version: string|null, pretty_version: string|null}
    fn guess_feature_version(
        &mut self,
        package_config: &IndexMap<String, PhpMixed>,
        version: Option<String>,
        mut branches: Vec<String>,
        scm_cmdline: Vec<String>,
        path: &str,
    ) -> Result<FeatureVersionResult> {
        let mut pretty_version: Option<String> = version.clone();
        let mut version = version;

        // ignore feature branches if they have no branch-alias or self.version is used
        // and find the branch they came from to use as a version instead
        let has_branch_alias = package_config
            .get("extra")
            .and_then(|v| v.as_array())
            .and_then(|m| m.get("branch-alias"))
            .and_then(|v| v.as_array())
            .map(|m| m.contains_key(version.as_deref().unwrap_or("")))
            .unwrap_or(false);
        let has_self_version = strpos(
            &json_encode(&PhpMixed::Array(
                package_config
                    .iter()
                    .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                    .collect(),
            ))
            .unwrap_or_default(),
            "\"self.version\"",
        )
        .is_some();
        if !has_branch_alias || has_self_version {
            let branch = Preg::replace(r"{^dev-}", "", version.as_deref().unwrap_or(""));
            let mut length: i64 = PHP_INT_MAX;

            // return directly, if branch is configured to be non-feature branch
            if !self.is_feature_branch(package_config, Some(&branch)) {
                return Ok(FeatureVersionResult {
                    version,
                    pretty_version,
                });
            }

            // sort local branches first then remote ones
            // and sort numeric branches below named ones, to make sure if the branch has the same distance from main and 1.10 and 1.9 for example, 1.9 is picked
            // and sort using natural sort so that 1.10 will appear before 1.9
            usort(&mut branches, |a: &String, b: &String| -> i64 {
                let a_remote = strpos(a, "remotes/") == Some(0);
                let b_remote = strpos(b, "remotes/") == Some(0);

                if a_remote != b_remote {
                    return if a_remote { 1 } else { -1 };
                }

                strnatcasecmp(b, a)
            });

            let mut promises: Vec<Box<dyn shirabe_external_packages::react::promise::promise_interface::PromiseInterface>> =
                vec![];
            self.process.set_max_jobs(30);
            // TODO(phase-b): try/finally with resetMaxJobs
            let result: Result<()> = (|| -> Result<()> {
                let mut last_index: i64 = -1;
                for (index, candidate) in branches.iter().enumerate() {
                    let candidate_version = Preg::replace(r"{^remotes/\S+/}", "", candidate);

                    // do not compare against itself or other feature branches
                    if candidate == &branch
                        || self.is_feature_branch(package_config, Some(&candidate_version))
                    {
                        continue;
                    }

                    let candidate_clone = candidate.clone();
                    let branch_clone = branch.clone();
                    let cmd_line: Vec<String> = array_map(
                        move |component: &String| -> String {
                            // TODO(phase-b): str_replace with array arguments — emulating
                            let r1 = str_replace("%candidate%", &candidate_clone, component);
                            str_replace("%branch%", &branch_clone, &r1)
                        },
                        &scm_cmdline,
                    );
                    let async_promise = self.process.execute_async(&cmd_line, path);
                    promises.push(async_promise.then(Box::new(
                        move |process: Process| -> Result<()> {
                            if !process.is_successful() {
                                return Ok(());
                            }

                            let output = process.get_output();
                            // overwrite existing if we have a shorter diff, or we have an equal diff and an index that comes later in the array (i.e. older version)
                            // as newer versions typically have more commits, if the feature branch is based on a newer branch it should have a longer diff to the old version
                            // but if it doesn't and they have equal diffs, then it probably is based on the old version
                            // TODO(phase-b): closure captures need shared mutable state (last_index, length, version, pretty_version, promises)
                            todo!(
                                "mutate last_index/length/version/pretty_version and possibly cancel promises"
                            );
                        },
                    )));
                }

                self.process.wait();
                Ok(())
            })();
            self.process.reset_max_jobs();
            result?;
        }

        Ok(FeatureVersionResult {
            version,
            pretty_version,
        })
    }

    /// @param array<string, mixed> $packageConfig
    fn is_feature_branch(
        &self,
        package_config: &IndexMap<String, PhpMixed>,
        branch_name: Option<&str>,
    ) -> bool {
        let mut non_feature_branches = String::new();
        let nf_value = package_config.get("non-feature-branches");
        if !empty(&nf_value.cloned().unwrap_or(PhpMixed::Null)) {
            let names: Vec<String> = nf_value
                .and_then(|v| v.as_list())
                .map(|l| {
                    l.iter()
                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            non_feature_branches = implode("|", &names);
        }

        !Preg::is_match(
            &format!(
                r"{{^({}|master|main|latest|next|current|support|tip|trunk|default|develop|\d+\..+)$}}",
                non_feature_branches,
            ),
            branch_name.unwrap_or(""),
        )
    }

    /// @return array{version: string|null, commit: '', pretty_version: string|null}
    fn guess_fossil_version(&mut self, path: &str) -> Result<VersionData> {
        let mut version: Option<String> = None;
        let mut pretty_version: Option<String> = None;

        // try to fetch current version from fossil
        let mut output = String::new();
        if 0 == self.process.execute(
            &[
                "fossil".to_string(),
                "branch".to_string(),
                "list".to_string(),
            ],
            &mut output,
            Some(path.to_string()),
        ) {
            let branch = trim(&output, None);
            version = Some(self.version_parser.normalize_branch(&branch));
            pretty_version = Some(format!("dev-{}", branch));
        }

        // try to fetch current version from fossil tags
        let mut output = String::new();
        if 0 == self.process.execute(
            &["fossil".to_string(), "tag".to_string(), "list".to_string()],
            &mut output,
            Some(path.to_string()),
        ) {
            // TODO(phase-b): use anyhow::Result<Result<T, E>> to model PHP try/catch
            match self.version_parser.normalize(&trim(&output, None), None) {
                Ok(v) => {
                    version = Some(v);
                    pretty_version = Some(trim(&output, None));
                }
                Err(_e) => {}
            }
        }

        Ok(VersionData {
            version,
            commit: Some(String::new()),
            pretty_version,
            feature_version: None,
            feature_pretty_version: None,
        })
    }

    /// @param array<string, mixed> $packageConfig
    ///
    /// @return array{version: string, commit: '', pretty_version: string}|null
    fn guess_svn_version(
        &mut self,
        package_config: &IndexMap<String, PhpMixed>,
        path: &str,
    ) -> Result<Option<VersionData>> {
        SvnUtil::clean_env();

        // try to fetch current version from svn
        let mut output = String::new();
        if 0 == self.process.execute(
            &["svn".to_string(), "info".to_string(), "--xml".to_string()],
            &mut output,
            Some(path.to_string()),
        ) {
            let trunk_path = package_config
                .get("trunk-path")
                .and_then(|v| v.as_string())
                .map(|s| preg_quote(s, Some('#')))
                .unwrap_or_else(|| "trunk".to_string());
            let branches_path = package_config
                .get("branches-path")
                .and_then(|v| v.as_string())
                .map(|s| preg_quote(s, Some('#')))
                .unwrap_or_else(|| "branches".to_string());
            let tags_path = package_config
                .get("tags-path")
                .and_then(|v| v.as_string())
                .map(|s| preg_quote(s, Some('#')))
                .unwrap_or_else(|| "tags".to_string());

            let url_pattern = format!(
                "#<url>.*/({}|({}|{})/(.*))</url>#",
                trunk_path, branches_path, tags_path,
            );

            if let Some(matches) = Preg::is_match_with_indexed_captures(&url_pattern, &output)? {
                let m1 = matches.get(1).cloned().unwrap_or_default();
                let m2 = matches.get(2).cloned();
                let m3 = matches.get(3).cloned();
                if m2.is_some()
                    && m3.is_some()
                    && (branches_path == *m2.as_ref().unwrap()
                        || tags_path == *m2.as_ref().unwrap())
                {
                    // we are in a branches path
                    let version = self.version_parser.normalize_branch(m3.as_deref().unwrap());
                    let pretty_version = format!("dev-{}", m3.as_ref().unwrap());

                    return Ok(Some(VersionData {
                        version: Some(version),
                        commit: Some(String::new()),
                        pretty_version: Some(pretty_version),
                        feature_version: None,
                        feature_pretty_version: None,
                    }));
                }

                assert!(is_string(&PhpMixed::String(m1.clone())));
                let pretty_version = trim(&m1, None);
                let version = if pretty_version == "trunk" {
                    "dev-trunk".to_string()
                } else {
                    self.version_parser.normalize(&pretty_version, None)?
                };

                return Ok(Some(VersionData {
                    version: Some(version),
                    commit: Some(String::new()),
                    pretty_version: Some(pretty_version),
                    feature_version: None,
                    feature_pretty_version: None,
                }));
            }
        }

        Ok(None)
    }

    pub fn get_root_version_from_env(&self) -> Result<String> {
        let version = Platform::get_env("COMPOSER_ROOT_VERSION");
        let version = match version {
            Some(v) if !v.is_empty() => v,
            _ => {
                return Err(RuntimeException {
                    message: "COMPOSER_ROOT_VERSION not set or empty".to_string(),
                    code: 0,
                }
                .into());
            }
        };
        if let Some(m) = Preg::is_match_strict_groups(r"{^(\d+(?:\.\d+)*)-dev$}i", &version) {
            return Ok(format!("{}.x-dev", m.get(1).cloned().unwrap_or_default()));
        }

        Ok(version)
    }
}

#[derive(Debug)]
pub struct FeatureVersionResult {
    pub version: Option<String>,
    pub pretty_version: Option<String>,
}
