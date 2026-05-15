//! ref: composer/src/Composer/Repository/RepositoryManager.php

use indexmap::IndexMap;
use shirabe_php_shim::{json_encode, InvalidArgumentException, PhpMixed};
use shirabe_semver::constraint::constraint_interface::ConstraintInterface;

use crate::config::Config;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::io::io_interface::IOInterface;
use crate::package::package_interface::PackageInterface;
use crate::repository::filter_repository::FilterRepository;
use crate::repository::installed_repository_interface::InstalledRepositoryInterface;
use crate::repository::repository_interface::RepositoryInterface;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;

pub struct RepositoryManager {
    local_repository: Option<Box<dyn InstalledRepositoryInterface>>,
    repositories: Vec<Box<dyn RepositoryInterface>>,
    repository_classes: IndexMap<String, String>,
    io: Box<dyn IOInterface>,
    config: Config,
    http_downloader: HttpDownloader,
    event_dispatcher: Option<EventDispatcher>,
    process: ProcessExecutor,
}

impl RepositoryManager {
    pub fn new(io: &dyn IOInterface, config: &Config, http_downloader: HttpDownloader, event_dispatcher: Option<EventDispatcher>, process: Option<ProcessExecutor>) -> Self {
        let process = process.unwrap_or_else(|| ProcessExecutor::new(io));
        Self {
            local_repository: None,
            repositories: vec![],
            repository_classes: IndexMap::new(),
            io: io.clone_box(),
            config: config.clone(),
            http_downloader,
            event_dispatcher,
            process,
        }
    }

    pub fn find_package(&self, name: &str, constraint: &dyn ConstraintInterface) -> Option<Box<dyn PackageInterface>> {
        for repository in &self.repositories {
            if let Some(package) = repository.find_package(name, constraint) {
                return Some(package);
            }
        }
        None
    }

    pub fn find_packages(&self, name: &str, constraint: &dyn ConstraintInterface) -> Vec<Box<dyn PackageInterface>> {
        let mut packages: Vec<Box<dyn PackageInterface>> = vec![];
        for repository in self.get_repositories() {
            packages.extend(repository.find_packages(name, constraint));
        }
        packages
    }

    pub fn add_repository(&mut self, repository: Box<dyn RepositoryInterface>) {
        self.repositories.push(repository);
    }

    pub fn prepend_repository(&mut self, repository: Box<dyn RepositoryInterface>) {
        self.repositories.insert(0, repository);
    }

    pub fn create_repository(&self, r#type: &str, config: IndexMap<String, PhpMixed>, name: Option<&str>) -> anyhow::Result<Box<dyn RepositoryInterface>> {
        if !self.repository_classes.contains_key(r#type) {
            return Err(InvalidArgumentException {
                message: format!("Repository type is not registered: {}", r#type),
                code: 0,
            }.into());
        }

        if config.get("packagist").and_then(|v| v.as_bool()) == Some(false) {
            let config_json = json_encode(&PhpMixed::Array(config.iter().map(|(k, v)| (k.clone(), Box::new(v.clone()))).collect())).unwrap_or_default();
            self.io.write_error(&format!("<warning>Repository \"{}\" ({}) has a packagist key which should be in its own repository definition</warning>", name.unwrap_or(""), config_json));
        }

        let class = self.repository_classes[r#type].clone();

        let has_filter = config.contains_key("only") || config.contains_key("exclude") || config.contains_key("canonical");
        let filter_config = if has_filter { Some(config.clone()) } else { None };

        let mut cleaned_config = config;
        cleaned_config.remove("only");
        cleaned_config.remove("exclude");
        cleaned_config.remove("canonical");

        // Phase B: implement dynamic class instantiation by class name
        let repository = self.create_repository_by_class(&class, cleaned_config)?;

        if let Some(filter_config) = filter_config {
            return Ok(Box::new(FilterRepository::new(repository, filter_config)));
        }

        Ok(repository)
    }

    fn create_repository_by_class(&self, _class: &str, _config: IndexMap<String, PhpMixed>) -> anyhow::Result<Box<dyn RepositoryInterface>> {
        todo!("Phase B: dynamic class instantiation by class name")
    }

    pub fn set_repository_class(&mut self, r#type: &str, class: &str) {
        self.repository_classes.insert(r#type.to_string(), class.to_string());
    }

    pub fn get_repositories(&self) -> &Vec<Box<dyn RepositoryInterface>> {
        &self.repositories
    }

    pub fn set_local_repository(&mut self, repository: Box<dyn InstalledRepositoryInterface>) {
        self.local_repository = Some(repository);
    }

    pub fn get_local_repository(&self) -> &dyn InstalledRepositoryInterface {
        self.local_repository.as_ref().unwrap().as_ref()
    }
}
