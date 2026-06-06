//! ref: composer/src/Composer/Downloader/SvnDownloader.php

use crate::io::io_interface;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{PhpMixed, RuntimeException, is_dir, version_compare};

use crate::config::Config;
use crate::downloader::ChangeReportInterface;
use crate::downloader::DownloaderInterface;
use crate::downloader::VcsCapableDownloaderInterface;
use crate::downloader::VcsDownloader;
use crate::downloader::VcsDownloaderBase;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::repository::VcsRepository;
use crate::util::Filesystem;
use crate::util::ProcessExecutor;
use crate::util::Svn as SvnUtil;

#[derive(Debug)]
pub struct SvnDownloader {
    inner: VcsDownloaderBase,
    pub(crate) cache_credentials: bool,
}

impl SvnDownloader {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        fs: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    ) -> Self {
        Self {
            inner: VcsDownloaderBase::new(io, config, Some(process), Some(fs)),
            cache_credentials: true,
        }
    }

    pub(crate) fn execute(
        &self,
        package: PackageInterfaceHandle,
        base_url: &str,
        command: Vec<String>,
        url: &str,
        cwd: Option<&str>,
        path: Option<&str>,
    ) -> anyhow::Result<String> {
        let mut util = SvnUtil::new(
            base_url.to_string(),
            self.inner.io.clone(),
            self.inner.config.clone(),
            Some(self.inner.process.clone()),
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

    pub(crate) async fn discard_changes(&self, path: &str) -> anyhow::Result<Option<PhpMixed>> {
        let mut output = String::new();
        if self.inner.process.borrow_mut().execute_args(
            &["svn", "revert", "-R", "."].map(|s| s.to_string()).to_vec(),
            &mut output,
            Some(path.to_string()),
        ) != 0
        {
            return Err(RuntimeException {
                message: format!(
                    "Could not reset changes\n\n:{}",
                    self.inner.process.borrow().get_error_output()
                ),
                code: 0,
            }
            .into());
        }

        Ok(None)
    }

    /// The default `VcsDownloader::clean_changes()` behavior: fail if the working copy has
    /// local changes.
    fn fail_on_local_changes(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<()> {
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

impl VcsDownloader for SvnDownloader {
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
        _package: PackageInterfaceHandle,
        _path: &str,
        url: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        SvnUtil::clean_env();
        let mut util = SvnUtil::new(
            url.to_string(),
            self.inner.io.clone(),
            self.inner.config.clone(),
            Some(self.inner.process.clone()),
        );
        if util.binary_version().is_none() {
            return Err(RuntimeException {
                message: "svn was not found in your PATH, skipping source download".to_string(),
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
    ) -> anyhow::Result<Option<PhpMixed>> {
        SvnUtil::clean_env();
        let r#ref = package.get_source_reference();

        {
            let repo = package.get_repository();
            if let Some(repo) = repo {
                let repo_ref = repo.borrow();
                if let Some(vcs_repo) = repo_ref.as_any().downcast_ref::<VcsRepository>() {
                    let repo_config = vcs_repo.get_repo_config();
                    if repo_config.contains_key("svn-cache-credentials") {
                        if let Some(val) = repo_config
                            .get("svn-cache-credentials")
                            .and_then(|v| v.as_bool())
                        {
                            self.cache_credentials = val;
                        }
                    }
                }
            }
        }

        self.inner.io.write_error3(
            &format!(
                " Checking out {}",
                package.get_source_reference().unwrap_or_default()
            ),
            true,
            io_interface::NORMAL,
        );
        self.execute(
            package,
            url,
            vec!["svn".to_string(), "co".to_string()],
            &format!("{}/{}", url, r#ref.unwrap_or_default()),
            None,
            Some(path),
        )?;

        Ok(None)
    }

    async fn do_update(
        &mut self,
        _initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> anyhow::Result<Option<PhpMixed>> {
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

        let mut util = SvnUtil::new(
            url.to_string(),
            self.inner.io.clone(),
            self.inner.config.clone(),
            Some(self.inner.process.clone()),
        );
        let mut flags: Vec<String> = vec![];
        if version_compare(&util.binary_version().unwrap_or_default(), "1.7.0", ">=") {
            flags.push("--ignore-ancestry".to_string());
        }

        self.inner.io.write_error3(
            &format!(" Checking out {}", r#ref.clone().unwrap_or_default()),
            true,
            io_interface::NORMAL,
        );
        let mut command = vec!["svn".to_string(), "switch".to_string()];
        command.extend(flags);
        self.execute(
            target,
            url,
            command,
            &format!("{}/{}", url, r#ref.unwrap_or_default()),
            Some(path),
            None,
        )?;

        Ok(None)
    }

    async fn clean_changes(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        update: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let changes = self.get_local_changes(package.clone(), path)?;
        if changes.is_none() {
            return Ok(None);
        }

        if !self.inner.io.is_interactive() {
            if self
                .inner
                .config
                .borrow_mut()
                .get("discard-changes")
                .as_bool()
                == Some(true)
            {
                return self.discard_changes(path).await;
            }

            self.fail_on_local_changes(package, path)?;
            return Ok(None);
        }

        let changes_str = changes.unwrap();
        let changes: Vec<String> = Preg::split(r"{\s*\r?\n\s*}", &changes_str)
            .unwrap_or_default()
            .into_iter()
            .map(|elem| format!("    {}", elem))
            .collect();
        let count_changes = changes.len() as i64;
        self.inner.io.write_error3(
            &format!(
                "    <error>{} has modified file{}:</error>",
                package.get_pretty_name(),
                if count_changes == 1 { "" } else { "s" }
            ),
            true,
            io_interface::NORMAL,
        );
        let slice_end = 10_usize.min(changes.len());
        for line in &changes[..slice_end] {
            self.inner.io.write_error3(line, true, io_interface::NORMAL);
        }
        if count_changes > 10 {
            let remaining_changes = count_changes - 10;
            self.inner.io.write_error3(
                &format!(
                    "    <info>{} more file{} modified, choose \"v\" to view the full list</info>",
                    remaining_changes,
                    if remaining_changes == 1 { "" } else { "s" }
                ),
                true,
                io_interface::NORMAL,
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
                    self.discard_changes(path).await?;
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
                    for line in &changes {
                        self.inner.io.write_error3(line, true, io_interface::NORMAL);
                    }
                }
                _ => {
                    let help_lines = vec![
                        format!(
                            "    y - discard changes and apply the {}",
                            if update { "update" } else { "uninstall" }
                        ),
                        format!(
                            "    n - abort the {} and let you manually clean things up",
                            if update { "update" } else { "uninstall" }
                        ),
                        "    v - view modified files".to_string(),
                        "    ? - print help".to_string(),
                    ];
                    for line in &help_lines {
                        self.inner.io.write_error3(line, true, io_interface::NORMAL);
                    }
                }
            }
        }

        Ok(None)
    }

    fn get_commit_logs(
        &mut self,
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
            if self.inner.process.borrow_mut().execute_args(
                &command,
                &mut output,
                Some(path.to_string()),
            ) != 0
            {
                return Err(RuntimeException {
                    message: format!(
                        "Failed to execute {}\n\n{}",
                        command.join(" "),
                        self.inner.process.borrow().get_error_output()
                    ),
                    code: 0,
                }
                .into());
            }

            let url_pattern = "#<url>(.*)</url>#";
            let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
            let base_url = if Preg::match_strict_groups3(url_pattern, &output, Some(&mut matches))
                .unwrap_or(false)
            {
                matches
                    .get(&CaptureKey::ByIndex(1))
                    .cloned()
                    .unwrap_or_default()
            } else {
                return Err(RuntimeException {
                    message: format!("Unable to determine svn url for path {}", path),
                    code: 0,
                }
                .into());
            };

            // strip paths from references and only keep the actual revision
            let from_revision =
                Preg::replace(r"{.*@(\d+)$}", "$1", &from_reference).unwrap_or_default();
            let to_revision =
                Preg::replace(r"{.*@(\d+)$}", "$1", &to_reference).unwrap_or_default();

            let command = vec![
                "svn".to_string(),
                "log".to_string(),
                "-r".to_string(),
                format!("{}:{}", from_revision, to_revision),
                "--incremental".to_string(),
            ];

            let mut util = SvnUtil::new(
                base_url,
                self.inner.io.clone(),
                self.inner.config.clone(),
                Some(self.inner.process.clone()),
            );
            util.set_cache_credentials(self.cache_credentials);
            util.execute_local(command.clone(), path, None, self.inner.io.is_verbose())
                .map_err(|e| {
                    RuntimeException {
                        message: format!("Failed to execute {}\n\n{}", command.join(" "), e),
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

    fn has_metadata_repository(&self, path: &str) -> bool {
        is_dir(&format!("{}/.svn", path))
    }
}

impl ChangeReportInterface for SvnDownloader {
    fn get_local_changes(
        &mut self,
        _package: PackageInterfaceHandle,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        if !self.has_metadata_repository(path) {
            return Ok(None);
        }

        let mut output = String::new();
        self.inner.process.borrow_mut().execute_args(
            &["svn", "status", "--ignore-externals"]
                .map(|s| s.to_string())
                .to_vec(),
            &mut output,
            Some(path.to_string()),
        );

        Ok(
            if Preg::is_match("{^ *[^X ] +}m", &output).unwrap_or(false) {
                Some(output)
            } else {
                None
            },
        )
    }
}

impl VcsCapableDownloaderInterface for SvnDownloader {
    fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: String) -> Option<String> {
        self.inner.get_vcs_reference(package, &path)
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for SvnDownloader {
    fn get_installation_source(&self) -> String {
        <Self as VcsDownloader>::get_installation_source(self)
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
