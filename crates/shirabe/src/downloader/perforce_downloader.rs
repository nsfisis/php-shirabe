//! ref: composer/src/Composer/Downloader/PerforceDownloader.php

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
use crate::util::Perforce;
use crate::util::ProcessExecutor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct PerforceDownloader {
    inner: VcsDownloaderBase,
    pub(crate) perforce: Option<Perforce>,
}

impl PerforceDownloader {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        fs: std::rc::Rc<std::cell::RefCell<Filesystem>>,
    ) -> Self {
        Self {
            inner: VcsDownloaderBase::new(io, config, Some(process), Some(fs)),
            perforce: None,
        }
    }

    fn get_label_from_source_reference(&self, source_ref: String) -> Option<String> {
        let pos = source_ref.find('@');
        if let Some(pos) = pos {
            return Some(source_ref[pos + 1..].to_string());
        }

        None
    }

    pub fn init_perforce(&mut self, package: PackageInterfaceHandle, path: String, url: String) {
        if let Some(perforce) = self.perforce.as_mut() {
            perforce.initialize_path(&path);
            return;
        }

        let repository = package.get_repository();
        let repo_config: Option<IndexMap<String, PhpMixed>> = if let Some(repo) = repository {
            let repo_ref = repo.borrow();
            repo_ref
                .as_any()
                .downcast_ref::<VcsRepository>()
                .map(|vcs_repo| self.get_repo_config(vcs_repo))
        } else {
            None
        };
        self.perforce = Some(Perforce::create(
            repo_config.unwrap_or_default(),
            url,
            path,
            self.inner.process.clone(),
            self.inner.io.clone(),
        ));
    }

    fn get_repo_config(&self, repository: &VcsRepository) -> IndexMap<String, PhpMixed> {
        repository.get_repo_config().clone()
    }

    pub fn set_perforce(&mut self, perforce: Perforce) {
        self.perforce = Some(perforce);
    }
}

impl VcsDownloader for PerforceDownloader {
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
        _url: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        Ok(None)
    }

    async fn do_install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> Result<Option<PhpMixed>> {
        let source_ref = package.get_source_reference().map(|s| s.to_string());
        let label = self.get_label_from_source_reference(source_ref.clone().unwrap_or_default());

        self.inner.io.write_error(&format!(
            "Cloning {}",
            source_ref.clone().unwrap_or_default()
        ));
        self.init_perforce(package, path.to_string(), url.to_string());
        self.perforce
            .as_mut()
            .unwrap()
            .set_stream(&source_ref.clone().unwrap_or_default());
        self.perforce.as_mut().unwrap().p4_login();
        self.perforce.as_mut().unwrap().write_p4_client_spec();
        self.perforce.as_mut().unwrap().connect_client();
        self.perforce
            .as_mut()
            .unwrap()
            .sync_code_base(label.as_deref());
        self.perforce.as_mut().unwrap().cleanup_client_spec();

        Ok(None)
    }

    async fn do_update(
        &mut self,
        _initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
        url: &str,
    ) -> Result<Option<PhpMixed>> {
        self.do_install(target, path, url).await
    }

    fn get_commit_logs(
        &mut self,
        from_reference: &str,
        to_reference: &str,
        _path: &str,
    ) -> Result<String> {
        Ok(self
            .perforce
            .as_mut()
            .unwrap()
            .get_commit_logs(from_reference, to_reference)
            .unwrap_or_default())
    }

    fn has_metadata_repository(&self, _path: &str) -> bool {
        true
    }
}

impl ChangeReportInterface for PerforceDownloader {
    fn get_local_changes(
        &mut self,
        _package: PackageInterfaceHandle,
        _path: &str,
    ) -> Result<Option<String>> {
        self.inner
            .io
            .write_error("Perforce driver does not check for local changes before overriding");

        Ok(None)
    }
}

impl VcsCapableDownloaderInterface for PerforceDownloader {
    fn get_vcs_reference(&self, package: PackageInterfaceHandle, path: String) -> Option<String> {
        self.inner.get_vcs_reference(package, &path)
    }
}

#[async_trait::async_trait(?Send)]
impl DownloaderInterface for PerforceDownloader {
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

    fn get_installation_source(&self) -> String {
        <Self as VcsDownloader>::get_installation_source(self)
    }

    async fn download(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::download(self, package, path, prev_package).await
    }

    async fn prepare(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::prepare(self, r#type, package, path, prev_package).await
    }

    async fn install(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::install(self, package, path).await
    }

    async fn update(
        &mut self,
        initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: &str,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::update(self, initial, target, path).await
    }

    async fn remove(
        &mut self,
        package: PackageInterfaceHandle,
        path: &str,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::remove(self, package, path).await
    }

    async fn cleanup(
        &mut self,
        r#type: &str,
        package: PackageInterfaceHandle,
        path: &str,
        prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        <Self as VcsDownloader>::cleanup(self, r#type, package, path, prev_package).await
    }
}
