//! ref: composer/src/Composer/Repository/RepositoryFactory.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, UnexpectedValueException, get_debug_type, json_encode,
    php_to_string,
};

use crate::config::Config;
use crate::event_dispatcher::EventDispatcher;
use crate::factory::Factory;
use crate::io::IOInterface;
use crate::io::IOInterfaceMutable;
use crate::json::JsonFile;
use crate::repository::FilesystemRepository;
use crate::repository::RepositoryInterfaceHandle;
use crate::repository::RepositoryManager;
use crate::util::HttpDownloader;
use crate::util::ProcessExecutor;

pub struct RepositoryFactory;

impl RepositoryFactory {
    pub fn config_from_string(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
        repository: &str,
        allow_filesystem: bool,
    ) -> anyhow::Result<IndexMap<String, PhpMixed>> {
        if repository.starts_with("http") {
            let mut repo_config = IndexMap::new();
            repo_config.insert("type".to_string(), PhpMixed::String("composer".to_string()));
            repo_config.insert("url".to_string(), PhpMixed::String(repository.to_string()));
            return Ok(repo_config);
        }

        let extension = std::path::Path::new(repository)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if extension == "json" {
            let mut json = JsonFile::new(
                repository.to_string(),
                Some(std::rc::Rc::new(std::cell::RefCell::new(
                    Factory::create_http_downloader(io.clone(), config, IndexMap::new())?,
                ))),
                Some(io.clone()),
            )?;
            let data = json.read()?;
            let has_packages = data.get("packages").is_some_and(|v| !v.is_null());
            let has_includes = data.get("includes").is_some_and(|v| !v.is_null());
            let has_provider_includes = data.get("provider-includes").is_some_and(|v| !v.is_null());
            if has_packages || has_includes || has_provider_includes {
                let real_path = std::fs::canonicalize(repository)
                    .ok()
                    .and_then(|p| p.to_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| repository.to_string())
                    .replace('\\', "/");
                let mut repo_config = IndexMap::new();
                repo_config.insert("type".to_string(), PhpMixed::String("composer".to_string()));
                repo_config.insert(
                    "url".to_string(),
                    PhpMixed::String(format!("file://{}", real_path)),
                );
                return Ok(repo_config);
            } else if allow_filesystem {
                let mut repo_config = IndexMap::new();
                repo_config.insert(
                    "type".to_string(),
                    PhpMixed::String("filesystem".to_string()),
                );
                repo_config.insert("json".to_string(), PhpMixed::String(repository.to_string()));
                return Ok(repo_config);
            } else {
                return Err(InvalidArgumentException {
                    message: format!("Invalid repository URL ({}) given. This file does not contain a valid composer repository.", repository),
                    code: 0,
                }.into());
            }
        }

        if repository.starts_with('{') {
            let parsed = JsonFile::parse_json(Some(repository), None)?;
            let repo_config: IndexMap<String, PhpMixed> =
                parsed.as_array().map(|m| m.clone()).unwrap_or_default();
            return Ok(repo_config);
        }

        Err(InvalidArgumentException {
            message: format!("Invalid repository url ({}) given. Has to be a .json file, an http url or a JSON object.", repository),
            code: 0,
        }.into())
    }

    pub fn from_string(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
        repository: &str,
        allow_filesystem: bool,
        rm: Option<&mut RepositoryManager>,
    ) -> anyhow::Result<RepositoryInterfaceHandle> {
        let repo_config =
            Self::config_from_string(io.clone(), config, repository, allow_filesystem)?;
        Self::create_repo(io, config, repo_config, rm)
    }

    pub fn create_repo(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
        repo_config: IndexMap<String, PhpMixed>,
        rm: Option<&mut RepositoryManager>,
    ) -> anyhow::Result<RepositoryInterfaceHandle> {
        let mut owned_rm;
        let rm = if let Some(rm) = rm {
            rm
        } else {
            owned_rm = Self::manager(io, config, None, None, None)?;
            &mut owned_rm
        };
        let repos =
            Self::create_repos(rm, vec![PhpMixed::Array(repo_config.into_iter().collect())])?;
        // PHP: return current($repos);
        let (_, first) = repos
            .into_iter()
            .next()
            .ok_or_else(|| UnexpectedValueException {
                message: "create_repos returned no repository".to_string(),
                code: 0,
            })?;
        Ok(first)
    }

    pub fn default_repos(
        io: Option<std::rc::Rc<std::cell::RefCell<dyn IOInterface>>>,
        config: Option<std::rc::Rc<std::cell::RefCell<Config>>>,
        rm: Option<&mut RepositoryManager>,
    ) -> anyhow::Result<IndexMap<String, RepositoryInterfaceHandle>> {
        let config = match config {
            Some(c) => c,
            None => std::rc::Rc::new(std::cell::RefCell::new(Factory::create_config(None, None)?)),
        };
        if let Some(io) = &io {
            io.borrow_mut()
                .load_configuration(&mut config.borrow_mut())?;
        }

        let mut owned_rm;
        let rm = if let Some(rm) = rm {
            rm
        } else {
            let io = io.ok_or_else(|| InvalidArgumentException {
                message: "This function requires either an IOInterface or a RepositoryManager"
                    .to_string(),
                code: 0,
            })?;
            owned_rm = Self::manager(
                io.clone(),
                &config,
                Some(std::rc::Rc::new(std::cell::RefCell::new(
                    Factory::create_http_downloader(io, &config, IndexMap::new())?,
                ))),
                None,
                None,
            )?;
            &mut owned_rm
        };

        let repo_configs = config.borrow().get_repositories();
        // PHP: array_values($repoConfigs) — keep ordering, discard keys
        Self::create_repos(rm, repo_configs.into_iter().map(|(_, v)| v).collect())
    }

    pub fn manager(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
        config: &std::rc::Rc<std::cell::RefCell<Config>>,
        http_downloader: Option<std::rc::Rc<std::cell::RefCell<HttpDownloader>>>,
        event_dispatcher: Option<std::rc::Rc<std::cell::RefCell<EventDispatcher>>>,
        process: Option<std::rc::Rc<std::cell::RefCell<ProcessExecutor>>>,
    ) -> anyhow::Result<RepositoryManager> {
        let http_downloader = match http_downloader {
            Some(h) => h,
            None => std::rc::Rc::new(std::cell::RefCell::new(Factory::create_http_downloader(
                io.clone(),
                config,
                IndexMap::new(),
            )?)),
        };
        let process = match process {
            Some(p) => p,
            None => {
                let mut p = ProcessExecutor::new(Some(io.clone()));
                p.enable_async();
                std::rc::Rc::new(std::cell::RefCell::new(p))
            }
        };

        let mut rm = RepositoryManager::new(
            io,
            config.clone(),
            http_downloader,
            event_dispatcher,
            Some(process),
        );
        rm.set_repository_class("composer", "Composer\\Repository\\ComposerRepository");
        rm.set_repository_class("vcs", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("package", "Composer\\Repository\\PackageRepository");
        rm.set_repository_class("pear", "Composer\\Repository\\PearRepository");
        rm.set_repository_class("git", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("bitbucket", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("git-bitbucket", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("github", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("gitlab", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("svn", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("fossil", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("perforce", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("hg", "Composer\\Repository\\VcsRepository");
        rm.set_repository_class("artifact", "Composer\\Repository\\ArtifactRepository");
        rm.set_repository_class("path", "Composer\\Repository\\PathRepository");

        Ok(rm)
    }

    pub fn default_repos_with_default_manager(
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> anyhow::Result<IndexMap<String, RepositoryInterfaceHandle>> {
        let config = std::rc::Rc::new(std::cell::RefCell::new(Factory::create_config(
            Some(io.clone()),
            None,
        )?));
        let mut manager = Self::manager(io.clone(), &config, None, None, None)?;
        io.borrow_mut()
            .load_configuration(&mut config.borrow_mut())?;
        Self::default_repos(Some(io), Some(config), Some(&mut manager))
    }

    fn create_repos(
        rm: &mut RepositoryManager,
        repo_configs: Vec<PhpMixed>,
    ) -> anyhow::Result<IndexMap<String, RepositoryInterfaceHandle>> {
        let mut repo_map: IndexMap<String, RepositoryInterfaceHandle> = IndexMap::new();

        for (index, repo) in repo_configs.into_iter().enumerate() {
            match &repo {
                PhpMixed::String(_) => {
                    return Err(UnexpectedValueException {
                        message: "\"repositories\" should be an array of repository definitions, only a single repository was given".to_string(),
                        code: 0,
                    }.into());
                }
                PhpMixed::Array(repo_arr) => {
                    if !repo_arr.contains_key("type") {
                        return Err(UnexpectedValueException {
                            message: format!(
                                "Repository \"{}\" ({}) must have a type defined",
                                index,
                                json_encode(&repo).unwrap_or_default()
                            ),
                            code: 0,
                        }
                        .into());
                    }
                    let repo_type = repo_arr
                        .get("type")
                        .and_then(|v| v.as_string())
                        .unwrap_or("")
                        .to_string();
                    let repo_config_map: IndexMap<String, PhpMixed> = repo_arr
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    let name =
                        Self::generate_repository_name_indexed(index, &repo_config_map, &repo_map);

                    if repo_type == "filesystem" {
                        let json_path = repo_arr
                            .get("json")
                            .and_then(|v| v.as_string())
                            .unwrap_or("")
                            .to_string();
                        let created: RepositoryInterfaceHandle =
                            RepositoryInterfaceHandle::new(FilesystemRepository::new(
                                JsonFile::new(json_path, None, None)?,
                                false,
                                None,
                                None,
                            )?);
                        repo_map.insert(name, created);
                    } else {
                        let created = rm.create_repository(
                            &repo_type,
                            repo_config_map,
                            Some(&index.to_string()),
                        )?;
                        repo_map.insert(name, created);
                    }
                }
                _ => {
                    return Err(UnexpectedValueException {
                        message: format!(
                            "Repository \"{}\" ({}) should be an array, {} given",
                            index,
                            json_encode(&repo).unwrap_or_default(),
                            get_debug_type(&repo)
                        ),
                        code: 0,
                    }
                    .into());
                }
            }
        }

        Ok(repo_map)
    }

    pub fn generate_repository_name<T>(
        index: &PhpMixed,
        repo: &IndexMap<String, PhpMixed>,
        existing_repos: &IndexMap<String, T>,
    ) -> String {
        let mut name = if matches!(index, PhpMixed::Int(_)) && repo.contains_key("url") {
            let url = repo.get("url").and_then(|v| v.as_string()).unwrap_or("");
            Preg::replace("{^https?://}i", "", url)
        } else {
            php_to_string(index)
        };
        while existing_repos.contains_key(&name) {
            name.push('2');
        }
        name
    }

    fn generate_repository_name_indexed(
        index: usize,
        repo: &IndexMap<String, PhpMixed>,
        existing_repos: &IndexMap<String, RepositoryInterfaceHandle>,
    ) -> String {
        let mut name = if let Some(url) = repo.get("url").and_then(|v| v.as_string()) {
            Preg::replace("{^https?://}i", "", url)
        } else {
            index.to_string()
        };
        while existing_repos.contains_key(&name) {
            name.push('2');
        }
        name
    }
}
