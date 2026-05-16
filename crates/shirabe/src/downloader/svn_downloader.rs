//! ref: composer/src/Composer/Downloader/SvnDownloader.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::react::promise;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::{is_dir, version_compare, PhpMixed, RuntimeException};

use crate::downloader::vcs_downloader::VcsDownloader;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::repository::vcs_repository::VcsRepository;
use crate::util::svn::Svn as SvnUtil;

#[derive(Debug)]
pub struct SvnDownloader {
    inner: VcsDownloader,
    pub(crate) cache_credentials: bool,
}

impl SvnDownloader {
    pub(crate) fn do_download(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        url: &str,
        prev_package: Option<&dyn PackageInterface>,
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        SvnUtil::clean_env();
        let util = SvnUtil::new(url, &*self.inner.io, &self.inner.config, &self.inner.process);
        if util.binary_version().is_none() {
            return Err(RuntimeException {
                message: "svn was not found in your PATH, skipping source download".to_string(),
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
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        SvnUtil::clean_env();
        let r#ref = package.get_source_reference();

        let repo = package.get_repository();
        if let Some(repo) = repo {
            if let Some(vcs_repo) = repo.as_any().downcast_ref::<VcsRepository>() {
                let repo_config = vcs_repo.get_repo_config();
                if repo_config.contains_key("svn-cache-credentials") {
                    if let Some(val) = repo_config.get("svn-cache-credentials").and_then(|v| v.as_bool()) {
                        self.cache_credentials = val;
                    }
                }
            }
        }

        self.inner.io.write_error(
            PhpMixed::String(format!(" Checking out {}", package.get_source_reference())),
            true,
            IOInterface::NORMAL,
        );
        self.execute(
            package,
            url,
            vec!["svn".to_string(), "co".to_string()],
            &format!("{}/{}", url, r#ref),
            None,
            Some(path),
        )?;

        Ok(promise::resolve(None))
    }

    pub(crate) fn do_update(
        &mut self,
        initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: &str,
        url: &str,
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        SvnUtil::clean_env();
        let r#ref = target.get_source_reference();

        if !self.has_metadata_repository(path) {
            return Err(RuntimeException {
                message: format!(
                    "The .svn directory is missing from {}, see https://getcomposer.org/commit-deps for more information",
                    path
                ),
                code: 0,
            }
            .into());
        }

        let util = SvnUtil::new(url, &*self.inner.io, &self.inner.config, &self.inner.process);
        let mut flags: Vec<String> = vec![];
        if version_compare(&util.binary_version().unwrap_or_default(), "1.7.0", ">=") {
            flags.push("--ignore-ancestry".to_string());
        }

        self.inner.io.write_error(
            PhpMixed::String(format!(" Checking out {}", r#ref)),
            true,
            IOInterface::NORMAL,
        );
        let mut command = vec!["svn".to_string(), "switch".to_string()];
        command.extend(flags);
        self.execute(
            target,
            url,
            command,
            &format!("{}/{}", url, r#ref),
            Some(path),
            None,
        )?;

        Ok(promise::resolve(None))
    }

    pub fn get_local_changes(&self, package: &dyn PackageInterface, path: &str) -> Option<String> {
        if !self.has_metadata_repository(path) {
            return None;
        }

        let mut output = String::new();
        self.inner.process.execute(
            &["svn", "status", "--ignore-externals"]
                .map(|s| s.to_string())
                .to_vec(),
            &mut output,
            Some(path.to_string()),
        );

        if Preg::is_match("{^ *[^X ] +}m", &output).unwrap_or(false) {
            Some(output)
        } else {
            None
        }
    }

    pub(crate) fn execute(
        &self,
        package: &dyn PackageInterface,
        base_url: &str,
        command: Vec<String>,
        url: &str,
        cwd: Option<&str>,
        path: Option<&str>,
    ) -> anyhow::Result<String> {
        let mut util = SvnUtil::new(
            base_url,
            &*self.inner.io,
            &self.inner.config,
            &self.inner.process,
        );
        util.set_cache_credentials(self.cache_credentials);
        util.execute(command, url, cwd, path, self.inner.io.is_verbose())
            .map_err(|e| {
                anyhow::anyhow!(
                    "{} could not be downloaded, {}",
                    package.get_pretty_name(),
                    e
                )
            })
    }

    pub(crate) fn clean_changes(
        &mut self,
        package: &dyn PackageInterface,
        path: &str,
        update: bool,
    ) -> anyhow::Result<Box<dyn PromiseInterface>> {
        let changes = self.get_local_changes(package, path);
        if changes.is_none() {
            return Ok(promise::resolve(None));
        }

        if !self.inner.io.is_interactive() {
            if self.inner.config.get("discard-changes").as_bool() == Some(true) {
                return self.discard_changes(path);
            }

            return self.inner.clean_changes(package, path, update);
        }

        let changes_str = changes.unwrap();
        let changes: Vec<String> = Preg::split(r"{\s*\r?\n\s*}", &changes_str)
            .into_iter()
            .map(|elem| format!("    {}", elem))
            .collect();
        let count_changes = changes.len() as i64;
        self.inner.io.write_error(
            PhpMixed::String(format!(
                "    <error>{} has modified file{}:</error>",
                package.get_pretty_name(),
                if count_changes == 1 { "" } else { "s" }
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
        if count_changes > 10 {
            let remaining_changes = count_changes - 10;
            self.inner.io.write_error(
                PhpMixed::String(format!(
                    "    <info>{} more file{} modified, choose \"v\" to view the full list</info>",
                    remaining_changes,
                    if remaining_changes == 1 { "" } else { "s" }
                )),
                true,
                IOInterface::NORMAL,
            );
        }

        loop {
            match self
                .inner
                .io
                .ask(
                    "    <info>Discard changes [y,n,v,?]?</info> ".to_string(),
                    PhpMixed::String("?".to_string()),
                )
                .as_string()
            {
                Some("y") => {
                    self.discard_changes(path)?;
                    break;
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
                _ => {
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
                            Box::new(PhpMixed::String(
                                "    v - view modified files".to_string(),
                            )),
                            Box::new(PhpMixed::String("    ? - print help".to_string())),
                        ]),
                        true,
                        IOInterface::NORMAL,
                    );
                }
            }
        }

        Ok(promise::resolve(None))
    }

    pub(crate) fn get_commit_logs(
        &self,
        from_reference: &str,
        to_reference: &str,
        path: &str,
    ) -> anyhow::Result<String> {
        if Preg::is_match(r"{@(\d+)$}", from_reference).unwrap_or(false)
            && Preg::is_match(r"{@(\d+)$}", to_reference).unwrap_or(false)
        {
            // retrieve the svn base url from the checkout folder
            let command = vec![
                "svn".to_string(),
                "info".to_string(),
                "--non-interactive".to_string(),
                "--xml".to_string(),
                "--".to_string(),
                path.to_string(),
            ];
            let mut output = String::new();
            if self
                .inner
                .process
                .execute(&command, &mut output, Some(path.to_string()))
                != 0
            {
                return Err(RuntimeException {
                    message: format!(
                        "Failed to execute {}\n\n{}",
                        command.join(" "),
                        self.inner.process.get_error_output()
                    ),
                    code: 0,
                }
                .into());
            }

            let url_pattern = "#<url>(.*)</url>#";
            let base_url = if let Some(matches) = Preg::match_strict_groups(url_pattern, &output) {
                matches.get("1").cloned().unwrap_or_default()
            } else {
                return Err(RuntimeException {
                    message: format!("Unable to determine svn url for path {}", path),
                    code: 0,
                }
                .into());
            };

            // strip paths from references and only keep the actual revision
            let from_revision = Preg::replace(r"{.*@(\d+)$}", "$1", from_reference.to_string());
            let to_revision = Preg::replace(r"{.*@(\d+)$}", "$1", to_reference.to_string());

            let command = vec![
                "svn".to_string(),
                "log".to_string(),
                "-r".to_string(),
                format!("{}:{}", from_revision, to_revision),
                "--incremental".to_string(),
            ];

            let mut util = SvnUtil::new(
                &base_url,
                &*self.inner.io,
                &self.inner.config,
                &self.inner.process,
            );
            util.set_cache_credentials(self.cache_credentials);
            util.execute_local(command.clone(), path, None, self.inner.io.is_verbose())
                .map_err(|e| {
                    RuntimeException {
                        message: format!(
                            "Failed to execute {}\n\n{}",
                            command.join(" "),
                            e
                        ),
                        code: 0,
                    }
                    .into()
                })
        } else {
            Ok(format!(
                "Could not retrieve changes between {} and {} due to missing revision information",
                from_reference, to_reference
            ))
        }
    }

    pub(crate) fn discard_changes(&self, path: &str) -> anyhow::Result<Box<dyn PromiseInterface>> {
        let mut output = String::new();
        if self.inner.process.execute(
            &["svn", "revert", "-R", "."]
                .map(|s| s.to_string())
                .to_vec(),
            &mut output,
            Some(path.to_string()),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Could not reset changes\n\n:{}",
                    self.inner.process.get_error_output()
                ),
                code: 0,
            }
            .into());
        }

        Ok(promise::resolve(None))
    }

    pub(crate) fn has_metadata_repository(&self, path: &str) -> bool {
        is_dir(&format!("{}/.svn", path))
    }
}
