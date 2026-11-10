//! ref: composer/src/Composer/Downloader/SvnDownloader.php sha1:6fddad1ae8a9f25c250ea83feb467a7fb40b1b06

use crate::config::{Config, DiscardChanges};
use crate::downloader::{
    ChangeReportInterface, DownloaderInterface, VcsCapableDownloaderInterface, VcsDownloader,
    VcsDownloaderBase,
};
use crate::io::{IOInterface, IOInterfaceImmutable, IOInterfaceWrite};
use crate::package::PackageInterfaceHandle;
use crate::repository::VcsRepository;
use crate::util::{Filesystem, ProcessExecutor, Svn as SvnUtil};
use bstr::{B, BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_shim::{
    CmpOp, PhpMixed, RuntimeException, bformat, error_message, impl_php_class, implode, is_dir,
    path_from_bytes, path_to_bytes, php_regex, preg_is_match, preg_match, preg_replace, preg_split,
    version_compare,
};
use shirabe_symfony_console::input::InputValue;

#[derive(Debug)]
pub struct SvnDownloader {
    inner: VcsDownloaderBase,
    cache_credentials: std::cell::Cell<bool>,
}

impl_php_class!(SvnDownloader, r"Composer\Downloader\SvnDownloader");

impl SvnDownloader {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        fs: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    ) -> Self {
        Self {
            inner: VcsDownloaderBase::new(io, config, Some(process), Some(fs)),
            cache_credentials: std::cell::Cell::new(true),
        }
    }

    fn execute(
        &self,
        package: PackageInterfaceHandle,
        base_url: &BStr,
        command: Vec<BString>,
        url: &BStr,
        cwd: Option<&std::path::Path>,
        path: Option<&std::path::Path>,
    ) -> anyhow::Result<BString> {
        let mut util = SvnUtil::new(
            base_url.to_owned(),
            self.inner.io.clone(),
            self.inner.config.clone(),
            Some(self.inner.process.clone()),
        );
        util.set_cache_credentials(self.cache_credentials.get());
        util.execute(command, url, cwd, path, self.inner.io.is_verbose())
            .map_err(|e| {
                RuntimeException::new(bformat!(
                    "{} could not be downloaded, {}",
                    package.get_pretty_name(),
                    error_message(&e)
                ))
                .into()
            })
    }

    async fn discard_changes(&self, path: &std::path::Path) -> anyhow::Result<Option<PhpMixed>> {
        let mut output = BString::default();
        if self.inner.process.borrow_mut().execute(
            &["svn", "revert", "-R", "."],
            &mut output,
            Some(path),
        )? != 0
        {
            return Err(RuntimeException::new(bformat!(
                "Could not reset changes\n\n:{}",
                self.inner.process.borrow().get_error_output()
            ))
            .into());
        }

        Ok(None)
    }

    /// The default `VcsDownloader::clean_changes()` behavior: fail if the working copy has
    /// local changes.
    async fn fail_on_local_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<()> {
        if self.get_local_changes(package, path).await?.is_some() {
            return Err(RuntimeException::new(bformat!(
                "Source directory {path} has uncommitted changes."
            ))
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

    fn has_cleaned_changes(&self) -> &std::cell::RefCell<IndexMap<bstr::BString, bool>> {
        &self.inner.has_cleaned_changes
    }

    async fn do_download(
        &self,
        _package: PackageInterfaceHandle,
        _path: &std::path::Path,
        url: &bstr::BStr,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        SvnUtil::clean_env()?;
        let mut util = SvnUtil::new(
            url.to_owned(),
            self.inner.io.clone(),
            self.inner.config.clone(),
            Some(self.inner.process.clone()),
        );
        if util.binary_version()?.is_none() {
            return Err(RuntimeException::new(BString::from(
                "svn was not found in your PATH, skipping source download",
            ))
            .into());
        }

        Ok(None)
    }

    async fn do_install(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        url: &bstr::BStr,
    ) -> anyhow::Result<Option<PhpMixed>> {
        SvnUtil::clean_env()?;
        let r#ref = package.get_source_reference();

        {
            let repo = package.get_repository();
            if let Some(repo) = repo {
                let repo_ref = repo.borrow();
                if let Some(vcs_repo) = repo_ref.as_any().downcast_ref::<VcsRepository>() {
                    let repo_config = vcs_repo.get_repo_config();
                    if let Some(val) = repo_config.get(B("svn-cache-credentials")) {
                        self.cache_credentials.set(val.to_bool());
                    }
                }
            }
        }

        self.inner.io.write_error(bformat!(
            " Checking out {}",
            package.get_source_reference().unwrap_or_default()
        ));
        self.execute(
            package,
            url,
            vec!["svn".into(), "co".into()],
            bformat!("{url}/{}", r#ref.unwrap_or_default()).as_bstr(),
            None,
            Some(path),
        )?;

        Ok(None)
    }

    async fn do_update(
        &self,
        _initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &std::path::Path,
        url: &bstr::BStr,
    ) -> anyhow::Result<Option<PhpMixed>> {
        SvnUtil::clean_env()?;
        let r#ref = target.get_source_reference();

        if !self.has_metadata_repository(path) {
            return Err(RuntimeException::new(bformat!("The .svn directory is missing from {path}, see https://getcomposer.org/commit-deps for more information"))
            .into());
        }

        let mut util = SvnUtil::new(
            url.to_owned(),
            self.inner.io.clone(),
            self.inner.config.clone(),
            Some(self.inner.process.clone()),
        );
        let mut flags: Vec<BString> = vec![];
        if version_compare(
            util.binary_version()?.unwrap_or_default(),
            "1.7.0",
            CmpOp::Ge,
        ) {
            flags.push("--ignore-ancestry".into());
        }

        self.inner.io.write_error(bformat!(
            " Checking out {}",
            r#ref.clone().unwrap_or_default()
        ));
        let mut command: Vec<BString> = vec!["svn".into(), "switch".into()];
        command.extend(flags);
        self.execute(
            target,
            url,
            command,
            bformat!("{url}/{}", r#ref.unwrap_or_default()).as_bstr(),
            Some(path),
            None,
        )?;

        Ok(None)
    }

    async fn clean_changes(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        update: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        let changes = self.get_local_changes(package.clone(), path).await?;
        if changes.is_none() {
            return Ok(None);
        }

        if !self.inner.io.is_interactive() {
            if self.inner.config.borrow().get_discard_changes()? == DiscardChanges::Bool(true) {
                return self.discard_changes(path).await;
            }

            self.fail_on_local_changes(package, path).await?;
            return Ok(None);
        }

        let changes_str = changes.unwrap();
        let changes: Vec<BString> = preg_split(php_regex!(r"{\s*\r?\n\s*}"), &changes_str)
            .into_iter()
            .map(|elem| bformat!("    {elem}"))
            .collect();
        let count_changes = changes.len() as i64;
        self.inner.io.write_error(bformat!(
            "    <error>{} has modified file{}:</error>",
            package.get_pretty_name(),
            if count_changes == 1 { "" } else { "s" }
        ));
        let slice_end = 10_usize.min(changes.len());
        for line in &changes[..slice_end] {
            self.inner.io.write_error(line);
        }
        if count_changes > 10 {
            let remaining_changes = count_changes - 10;
            self.inner.io.write_error(bformat!("    <info>{remaining_changes} more file{} modified, choose \"v\" to view the full list</info>", if remaining_changes == 1 { "" } else { "s" }));
        }

        loop {
            match self
                .inner
                .io
                .ask(
                    "    <info>Discard changes [y,n,v,?]?</info> ",
                    InputValue::from("?"),
                )?
                .as_string()
                .map(|s| &**s)
            {
                Some(b"y") => {
                    self.discard_changes(path).await?;
                    break;
                }
                Some(b"n") => {
                    return Err(RuntimeException::new(BString::from("Update aborted")).into());
                }
                Some(b"v") => {
                    for line in &changes {
                        self.inner.io.write_error(line);
                    }
                }
                _ => {
                    let help_lines = vec![
                        bformat!(
                            "    y - discard changes and apply the {}",
                            if update { "update" } else { "uninstall" }
                        ),
                        bformat!(
                            "    n - abort the {} and let you manually clean things up",
                            if update { "update" } else { "uninstall" }
                        ),
                        BString::from("    v - view modified files"),
                        BString::from("    ? - print help"),
                    ];
                    for line in &help_lines {
                        self.inner.io.write_error(line);
                    }
                }
            }
        }

        Ok(None)
    }

    fn get_commit_logs(
        &self,
        from_reference: &bstr::BStr,
        to_reference: &bstr::BStr,
        path: &std::path::Path,
    ) -> anyhow::Result<bstr::BString> {
        if preg_is_match(php_regex!(r"{@(\d+)$}"), from_reference)
            && preg_is_match(php_regex!(r"{@(\d+)$}"), to_reference)
        {
            // retrieve the svn base url from the checkout folder
            let command: Vec<BString> = vec![
                "svn".into(),
                "info".into(),
                "--non-interactive".into(),
                "--xml".into(),
                "--".into(),
                path_to_bytes(path).into(),
            ];
            let mut output = BString::default();
            if self
                .inner
                .process
                .borrow_mut()
                .execute(&command, &mut output, Some(path))?
                != 0
            {
                return Err(RuntimeException::new(bformat!(
                    "Failed to execute {}\n\n{}",
                    implode(" ", &command),
                    self.inner.process.borrow().get_error_output()
                ))
                .into());
            }

            let url_pattern = "#<url>(.*)</url>#";
            let base_url = if let Some(matches) = preg_match(url_pattern, &output) {
                matches.get(1).unwrap_or_default().to_owned()
            } else {
                return Err(RuntimeException::new(bformat!(
                    "Unable to determine svn url for path {path}"
                ))
                .into());
            };

            // strip paths from references and only keep the actual revision
            let from_revision = preg_replace(php_regex!(r"{.*@(\d+)$}"), "$1", from_reference);
            let to_revision = preg_replace(php_regex!(r"{.*@(\d+)$}"), "$1", to_reference);

            let command: Vec<BString> = vec![
                "svn".into(),
                "log".into(),
                "-r".into(),
                bformat!("{from_revision}:{to_revision}"),
                "--incremental".into(),
            ];

            let mut util = SvnUtil::new(
                base_url,
                self.inner.io.clone(),
                self.inner.config.clone(),
                Some(self.inner.process.clone()),
            );
            util.set_cache_credentials(self.cache_credentials.get());
            util.execute_local(command.clone(), path, None, self.inner.io.is_verbose())
                .map_err(|e| {
                    RuntimeException::new(bformat!(
                        "Failed to execute {}\n\n{}",
                        implode(" ", &command),
                        error_message(&e)
                    ))
                    .into()
                })
        } else {
            Ok(bformat!(
                "Could not retrieve changes between {from_reference} and {to_reference} due to missing revision information"
            ))
        }
    }

    fn has_metadata_repository(&self, path: &std::path::Path) -> bool {
        is_dir(path_from_bytes(&bformat!("{path}/.svn")))
    }
}

#[async_trait::async_trait(?Send)]
impl ChangeReportInterface for SvnDownloader {
    async fn get_local_changes(
        &self,
        _package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<bstr::BString>> {
        if !self.has_metadata_repository(path) {
            return Ok(None);
        }

        let mut output = BString::default();
        self.inner.process.borrow_mut().execute(
            &["svn", "status", "--ignore-externals"],
            &mut output,
            Some(path),
        )?;

        Ok(if preg_is_match(php_regex!("{^ *[^X ] +}m"), &output) {
            Some(output)
        } else {
            None
        })
    }
}

impl VcsCapableDownloaderInterface for SvnDownloader {
    fn get_vcs_reference(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<bstr::BString>> {
        self.inner.get_vcs_reference(package, path)
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for SvnDownloader {
    fn get_installation_source(&self) -> BString {
        <Self as VcsDownloader>::get_installation_source(self)
    }

    fn as_change_report_interface(&self) -> Option<&dyn ChangeReportInterface> {
        Some(self)
    }

    fn as_vcs_capable_downloader_interface(&self) -> Option<&dyn VcsCapableDownloaderInterface> {
        Some(self)
    }

    async fn download_with_output(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::download(self, package, path, prev_package).await
    }

    async fn prepare(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install_with_output(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::install(self, package, path).await
    }

    async fn update(
        &self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &std::path::Path,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::update(self, initial, target, path).await
    }

    async fn remove_with_output(
        &self,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        _output: bool,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::remove(self, package, path).await
    }

    async fn cleanup(
        &self,
        r#type: &BStr,
        package: PackageInterfaceHandle,
        path: &std::path::Path,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> anyhow::Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
