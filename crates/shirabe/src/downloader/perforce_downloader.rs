//! ref: composer/src/Composer/Downloader/PerforceDownloader.php

use crate::config::Config;
use crate::downloader::DownloaderInterface;
use crate::downloader::VcsDownloaderBase;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterface;
use crate::package::PackageInterfaceHandle;
use crate::repository::VcsRepository;
use crate::util::Filesystem;
use crate::util::Perforce;
use crate::util::ProcessExecutor;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;
use std::any::Any;

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

    pub(crate) async fn do_download(
        &self,
        _package: PackageInterfaceHandle,
        _path: String,
        _url: String,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        Ok(None)
    }

    pub async fn do_install(
        &mut self,
        package: PackageInterfaceHandle,
        path: String,
        url: String,
    ) -> Result<Option<PhpMixed>> {
        let source_ref = package.get_source_reference().map(|s| s.to_string());
        let label = self.get_label_from_source_reference(source_ref.clone().unwrap_or_default());

        self.inner.io.write_error(&format!(
            "Cloning {}",
            source_ref.clone().unwrap_or_default()
        ));
        self.init_perforce(package, path.clone(), url);
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

    fn get_label_from_source_reference(&self, source_ref: String) -> Option<String> {
        let pos = source_ref.find('@');
        if let Some(pos) = pos {
            return Some(source_ref[pos + 1..].to_string());
        }

        None
    }

    pub fn init_perforce(&mut self, package: PackageInterfaceHandle, path: String, url: String) {
        if self.perforce.is_some() {
            self.perforce.as_mut().unwrap().initialize_path(&path);
            return;
        }

        let package_rc = package.as_rc().borrow();
        let repository = package_rc.as_package_interface().get_repository();
        let repo_config: Option<IndexMap<String, PhpMixed>> = if let Some(repo) = repository {
            if let Some(vcs_repo) = repo.as_any().downcast_ref::<VcsRepository>() {
                Some(self.get_repo_config(vcs_repo))
            } else {
                None
            }
        } else {
            None
        };
        drop(package_rc);
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

    pub(crate) async fn do_update(
        &mut self,
        _initial: PackageInterfaceHandle,
        target: PackageInterfaceHandle,
        path: String,
        url: String,
    ) -> Result<Option<PhpMixed>> {
        self.do_install(target, path, url).await
    }

    pub fn get_local_changes(
        &self,
        _package: PackageInterfaceHandle,
        _path: String,
    ) -> Option<String> {
        self.inner
            .io
            .write_error("Perforce driver does not check for local changes before overriding");

        None
    }

    pub(crate) fn get_commit_logs(
        &mut self,
        from_reference: String,
        to_reference: String,
        _path: String,
    ) -> Result<String> {
        Ok(self
            .perforce
            .as_mut()
            .unwrap()
            .get_commit_logs(&from_reference, &to_reference)
            .unwrap_or_default())
    }

    pub fn set_perforce(&mut self, perforce: Perforce) {
        self.perforce = Some(perforce);
    }

    pub(crate) fn has_metadata_repository(&self, _path: &str) -> bool {
        true
    }
}

// TODO(phase-b): wire up VcsDownloader trait properly. PerforceDownloader extends VcsDownloader
// which implements DownloaderInterface in PHP. Delegating each trait method to todo!() until the
// inner VcsDownloaderBase exposes the matching impl surface.
#[async_trait::async_trait(?Send)]
impl DownloaderInterface for PerforceDownloader {
    fn get_installation_source(&self) -> String {
        todo!()
    }

    async fn download(
        &self,
        _package: PackageInterfaceHandle,
        _path: &str,
        _prev_package: Option<PackageInterfaceHandle>,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn prepare(
        &self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _path: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn install(
        &self,
        _package: PackageInterfaceHandle,
        _path: &str,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn update(
        &self,
        _initial: PackageInterfaceHandle,
        _target: PackageInterfaceHandle,
        _path: &str,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn remove(
        &self,
        _package: PackageInterfaceHandle,
        _path: &str,
        _output: bool,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }

    async fn cleanup(
        &self,
        _type: &str,
        _package: PackageInterfaceHandle,
        _path: &str,
        _prev_package: Option<PackageInterfaceHandle>,
    ) -> Result<Option<PhpMixed>> {
        todo!()
    }
}
