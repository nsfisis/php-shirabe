//! ref: composer/src/Composer/Repository/RepositoryManager.php

use indexmap::IndexMap;
use shirabe_php_shim::{InvalidArgumentException, PhpMixed, json_encode};
use shirabe_semver::constraint::AnyConstraint;

use crate::config::Config;
use crate::event_dispatcher::EventDispatcher;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::package::PackageInterfaceHandle;
use crate::repository::FilterRepository;
use crate::repository::RepositoryInterfaceHandle;
use crate::util::HttpDownloader;
use crate::util::ProcessExecutor;

#[derive(Debug)]
pub struct RepositoryManager {
    local_repository: Option<RepositoryInterfaceHandle>,
    repositories: Vec<RepositoryInterfaceHandle>,
    repository_classes: IndexMap<String, String>,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    config: std::rc::Rc<std::cell::RefCell<Config>>,
    http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
    event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
}

impl RepositoryManager {
    pub fn new(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: std::rc::Rc<std::cell::RefCell<HttpDownloader>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> Self {
        let process = process.unwrap_or_else(|| {
            std::rc::Rc::new(std::cell::RefCell::new(ProcessExecutor::new(Some(
                io.clone(),
            ))))
        });
        Self {
            local_repository: None,
            repositories: vec![],
            repository_classes: IndexMap::new(),
            io,
            config,
            http_downloader,
            event_dispatcher,
            process,
        }
    }

    pub fn find_package(
        &self,
        name: &str,
        constraint: &AnyConstraint,
    ) -> anyhow::Result<Option<PackageInterfaceHandle>> {
        for repository in &self.repositories {
            if let Some(package) = repository.find_package(
                name,
                crate::repository::FindPackageConstraint::Constraint(constraint.clone()),
            )? {
                return Ok(Some(package.into()));
            }
        }
        Ok(None)
    }

    pub fn find_packages(
        &self,
        name: &str,
        constraint: &AnyConstraint,
    ) -> anyhow::Result<Vec<PackageInterfaceHandle>> {
        let mut packages: Vec<PackageInterfaceHandle> = vec![];
        for repository in self.get_repositories() {
            for p in repository.find_packages(
                name,
                Some(crate::repository::FindPackageConstraint::Constraint(
                    constraint.clone(),
                )),
            )? {
                packages.push(p);
            }
        }
        Ok(packages)
    }

    pub fn add_repository(&mut self, repository: RepositoryInterfaceHandle) {
        self.repositories.push(repository);
    }

    pub fn prepend_repository(&mut self, repository: RepositoryInterfaceHandle) {
        self.repositories.insert(0, repository);
    }

    pub fn create_repository(
        &self,
        r#type: &str,
        config: IndexMap<String, PhpMixed>,
        name: Option<&str>,
    ) -> anyhow::Result<RepositoryInterfaceHandle> {
        if !self.repository_classes.contains_key(r#type) {
            return Err(InvalidArgumentException {
                message: format!("Repository type is not registered: {}", r#type),
                code: 0,
            }
            .into());
        }

        if config.get("packagist").and_then(|v| v.as_bool()) == Some(false) {
            let config_json = json_encode(&PhpMixed::Array(
                config
                    .iter()
                    .map(|(k, v)| (k.clone(), Box::new(v.clone())))
                    .collect(),
            ))
            .unwrap_or_default();
            self.io.write_error(&format!("<warning>Repository \"{}\" ({}) has a packagist key which should be in its own repository definition</warning>", name.unwrap_or(""), config_json));
        }

        let class = self.repository_classes[r#type].clone();

        let has_filter = config.contains_key("only")
            || config.contains_key("exclude")
            || config.contains_key("canonical");
        let filter_config = if has_filter {
            Some(config.clone())
        } else {
            None
        };

        let mut cleaned_config = config;
        cleaned_config.remove("only");
        cleaned_config.remove("exclude");
        cleaned_config.remove("canonical");

        // Phase B: implement dynamic class instantiation by class name
        let repository = self.create_repository_by_class(&class, cleaned_config)?;

        if let Some(filter_config) = filter_config {
            return Ok(RepositoryInterfaceHandle::new(FilterRepository::new(
                repository,
                filter_config,
            )?));
        }

        Ok(repository)
    }

    fn create_repository_by_class(
        &self,
        _class: &str,
        _config: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<RepositoryInterfaceHandle> {
        todo!("Phase B: dynamic class instantiation by class name")
    }

    pub fn set_repository_class(&mut self, r#type: &str, class: &str) {
        self.repository_classes
            .insert(r#type.to_string(), class.to_string());
    }

    pub fn get_repositories(&self) -> &Vec<RepositoryInterfaceHandle> {
        &self.repositories
    }

    pub fn set_local_repository(&mut self, repository: RepositoryInterfaceHandle) {
        self.local_repository = Some(repository);
    }

    pub fn get_local_repository(&self) -> RepositoryInterfaceHandle {
        self.local_repository.clone().unwrap()
    }
}
