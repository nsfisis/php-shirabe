//! ref: composer/src/Composer/Downloader/PerforceDownloader.php

use crate::downloader::vcs_downloader::VcsDownloaderBase;
use crate::package::package_interface::PackageInterface;
use crate::repository::vcs_repository::VcsRepository;
use crate::util::perforce::Perforce;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::react::promise::promise_interface::PromiseInterface;
use shirabe_php_shim::PhpMixed;
use std::any::Any;

#[derive(Debug)]
pub struct PerforceDownloader {
    inner: VcsDownloaderBase,
    pub(crate) perforce: Option<Perforce>,
}

impl PerforceDownloader {
    pub(crate) fn do_download(
        &self,
        _package: &dyn PackageInterface,
        _path: String,
        _url: String,
        _prev_package: Option<&dyn PackageInterface>,
    ) -> Result<Box<dyn PromiseInterface>> {
        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    pub fn do_install(
        &mut self,
        package: &dyn PackageInterface,
        path: String,
        url: String,
    ) -> Result<Box<dyn PromiseInterface>> {
        let source_ref = package.get_source_reference();
        let label = self.get_label_from_source_reference(source_ref.clone().unwrap_or_default());

        self.inner.io.write_error(&format!(
            "Cloning {}",
            source_ref.clone().unwrap_or_default()
        ));
        self.init_perforce(package, path.clone(), url);
        self.perforce
            .as_mut()
            .unwrap()
            .set_stream(source_ref.clone().unwrap_or_default());
        self.perforce.as_mut().unwrap().p4_login();
        self.perforce.as_mut().unwrap().write_p4_client_spec();
        self.perforce.as_mut().unwrap().connect_client();
        self.perforce
            .as_mut()
            .unwrap()
            .sync_code_base(label.as_deref());
        self.perforce.as_mut().unwrap().cleanup_client_spec();

        Ok(shirabe_external_packages::react::promise::resolve(None))
    }

    fn get_label_from_source_reference(&self, source_ref: String) -> Option<String> {
        let pos = source_ref.find('@');
        if let Some(pos) = pos {
            return Some(source_ref[pos + 1..].to_string());
        }

        None
    }

    pub fn init_perforce(&mut self, package: &dyn PackageInterface, path: String, url: String) {
        if self.perforce.is_some() {
            self.perforce.as_mut().unwrap().initialize_path(path);
            return;
        }

        let repository = package.get_repository();
        let repo_config: Option<IndexMap<String, PhpMixed>> = if let Some(repo) = repository {
            if let Some(vcs_repo) = (repo.as_any() as &dyn Any).downcast_ref::<VcsRepository>() {
                Some(self.get_repo_config(vcs_repo))
            } else {
                None
            }
        } else {
            None
        };
        self.perforce = Some(Perforce::create(
            repo_config,
            url,
            path,
            &self.inner.process,
            &self.inner.io,
        ));
    }

    fn get_repo_config(&self, repository: &VcsRepository) -> IndexMap<String, PhpMixed> {
        repository.get_repo_config()
    }

    pub(crate) fn do_update(
        &mut self,
        _initial: &dyn PackageInterface,
        target: &dyn PackageInterface,
        path: String,
        url: String,
    ) -> Result<Box<dyn PromiseInterface>> {
        self.do_install(target, path, url)
    }

    pub fn get_local_changes(
        &self,
        _package: &dyn PackageInterface,
        _path: String,
    ) -> Option<String> {
        self.inner
            .io
            .write_error("Perforce driver does not check for local changes before overriding");

        None
    }

    pub(crate) fn get_commit_logs(
        &self,
        from_reference: String,
        to_reference: String,
        _path: String,
    ) -> Result<String> {
        Ok(self
            .perforce
            .as_ref()
            .unwrap()
            .get_commit_logs(from_reference, to_reference))
    }

    pub fn set_perforce(&mut self, perforce: Perforce) {
        self.perforce = Some(perforce);
    }

    pub(crate) fn has_metadata_repository(&self, _path: &str) -> bool {
        true
    }
}
