//! ref: composer/src/Composer/Repository/RepositoryFactory.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{get_debug_type, json_encode, InvalidArgumentException, PhpMixed, UnexpectedValueException};

use crate::config::Config;
use crate::event_dispatcher::event_dispatcher::EventDispatcher;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::repository::filesystem_repository::FilesystemRepository;
use crate::repository::repository_interface::RepositoryInterface;
use crate::repository::repository_manager::RepositoryManager;
use crate::util::http_downloader::HttpDownloader;
use crate::util::process_executor::ProcessExecutor;

pub struct RepositoryFactory;

impl RepositoryFactory {
    pub fn config_from_string(io: &dyn IOInterface, config: &Config, repository: &str, allow_filesystem: bool) -> anyhow::Result<IndexMap<String, PhpMixed>> {
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
            let json = JsonFile::new(repository.to_string(), Some(Factory::create_http_downloader(io, config)?));
            let data = json.read()?;
            let has_packages = data.get("packages").map_or(false, |v| !v.is_null());
            let has_includes = data.get("includes").map_or(false, |v| !v.is_null());
            let has_provider_includes = data.get("provider-includes").map_or(false, |v| !v.is_null());
            if has_packages || has_includes || has_provider_includes {
                let real_path = std::fs::canonicalize(repository).ok()
                    .and_then(|p| p.to_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| repository.to_string())
                    .replace('\\', "/");
                let mut repo_config = IndexMap::new();
                repo_config.insert("type".to_string(), PhpMixed::String("composer".to_string()));
                repo_config.insert("url".to_string(), PhpMixed::String(format!("file://{}", real_path)));
                return Ok(repo_config);
            } else if allow_filesystem {
                let mut repo_config = IndexMap::new();
                repo_config.insert("type".to_string(), PhpMixed::String("filesystem".to_string()));
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
            let repo_config = JsonFile::parse_json(repository, None)?.unwrap_or_default();
            return Ok(repo_config);
        }

        Err(InvalidArgumentException {
            message: format!("Invalid repository url ({}) given. Has to be a .json file, an http url or a JSON object.", repository),
            code: 0,
        }.into())
    }

    pub fn from_string(io: &dyn IOInterface, config: &Config, repository: &str, allow_filesystem: bool, rm: Option<&mut RepositoryManager>) -> anyhow::Result<Box<dyn RepositoryInterface>> {
        let repo_config = Self::config_from_string(io, config, repository, allow_filesystem)?;
        Self::create_repo(io, config, repo_config, rm)
    }

    pub fn create_repo(io: &dyn IOInterface, config: &Config, repo_config: IndexMap<String, PhpMixed>, rm: Option<&mut RepositoryManager>) -> anyhow::Result<Box<dyn RepositoryInterface>> {
        let mut owned_rm;
        let rm = if let Some(rm) = rm {
            rm
        } else {
            owned_rm = Self::manager(io, config, None, None, None)?;
            &mut owned_rm
        };
        let mut repos = Self::create_repos(rm, vec![PhpMixed::Array(
            repo_config.into_iter().map(|(k, v)| (k, Box::new(v))).collect()
        )])?;
        Ok(repos.remove(0))
    }

    pub fn default_repos(io: Option<&dyn IOInterface>, config: Option<Config>, rm: Option<&mut RepositoryManager>) -> anyhow::Result<Vec<Box<dyn RepositoryInterface>>> {
        let config = match config {
            Some(c) => c,
            None => Factory::create_config(None, None)?,
        };
        if let Some(io) = io {
            io.load_configuration(&config);
        }

        let mut owned_rm;
        let rm = if let Some(rm) = rm {
            rm
        } else {
            let io = io.ok_or_else(|| InvalidArgumentException {
                message: "This function requires either an IOInterface or a RepositoryManager".to_string(),
                code: 0,
            })?;
            owned_rm = Self::manager(io, &config, Some(Factory::create_http_downloader(io, &config)?), None, None)?;
            &mut owned_rm
        };

        let repo_configs = config.get_repositories();
        Self::create_repos(rm, repo_configs)
    }

    pub fn manager(io: &dyn IOInterface, config: &Config, http_downloader: Option<HttpDownloader>, event_dispatcher: Option<EventDispatcher>, process: Option<ProcessExecutor>) -> anyhow::Result<RepositoryManager> {
        let http_downloader = match http_downloader {
            Some(h) => h,
            None => Factory::create_http_downloader(io, config)?,
        };
        let process = match process {
            Some(p) => p,
            None => {
                let mut p = ProcessExecutor::new(io);
                p.enable_async();
                p
            }
        };

        let mut rm = RepositoryManager::new(io, config, http_downloader, event_dispatcher, process);
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

    pub fn default_repos_with_default_manager(io: &dyn IOInterface) -> anyhow::Result<Vec<Box<dyn RepositoryInterface>>> {
        let config = Factory::create_config(Some(io), None)?;
        let mut manager = Self::manager(io, &config, None, None, None)?;
        io.load_configuration(&config);
        Self::default_repos(Some(io), Some(config), Some(&mut manager))
    }

    fn create_repos(rm: &mut RepositoryManager, repo_configs: Vec<PhpMixed>) -> anyhow::Result<Vec<Box<dyn RepositoryInterface>>> {
        let mut repo_map: IndexMap<String, Box<dyn RepositoryInterface>> = IndexMap::new();

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
                            message: format!("Repository \"{}\" ({}) must have a type defined", index, json_encode(&repo).unwrap_or_default()),
                            code: 0,
                        }.into());
                    }
                    let repo_type = repo_arr.get("type").and_then(|v| v.as_string()).unwrap_or("").to_string();
                    let repo_config_map: IndexMap<String, PhpMixed> = repo_arr.iter().map(|(k, v)| (k.clone(), *v.clone())).collect();
                    let name = Self::generate_repository_name_indexed(index, &repo_config_map, &repo_map);

                    if repo_type == "filesystem" {
                        let json_path = repo_arr.get("json").and_then(|v| v.as_string()).unwrap_or("").to_string();
                        repo_map.insert(name, Box::new(FilesystemRepository::new(json_path)?));
                    } else {
                        let created = rm.create_repository(&repo_type, repo_config_map, &index.to_string())?;
                        repo_map.insert(name, created);
                    }
                }
                _ => {
                    return Err(UnexpectedValueException {
                        message: format!("Repository \"{}\" ({}) should be an array, {} given", index, json_encode(&repo).unwrap_or_default(), get_debug_type(&repo)),
                        code: 0,
                    }.into());
                }
            }
        }

        Ok(repo_map.into_values().collect())
    }

    pub fn generate_repository_name(index: &PhpMixed, repo: &IndexMap<String, PhpMixed>, existing_repos: &IndexMap<String, Box<dyn RepositoryInterface>>) -> String {
        let mut name = match index {
            PhpMixed::Int(_) => {
                if let Some(url) = repo.get("url").and_then(|v| v.as_string()) {
                    Preg::replace("{^https?://}i", "", url, -1).unwrap_or_else(|_| url.to_string())
                } else {
                    index.as_string().unwrap_or("").to_string()
                }
            }
            _ => index.as_string().unwrap_or("").to_string(),
        };
        while existing_repos.contains_key(&name) {
            name.push('2');
        }
        name
    }

    fn generate_repository_name_indexed(index: usize, repo: &IndexMap<String, PhpMixed>, existing_repos: &IndexMap<String, Box<dyn RepositoryInterface>>) -> String {
        let mut name = if let Some(url) = repo.get("url").and_then(|v| v.as_string()) {
            Preg::replace("{^https?://}i", "", url, -1).unwrap_or_else(|_| url.to_string())
        } else {
            index.to_string()
        };
        while existing_repos.contains_key(&name) {
            name.push('2');
        }
        name
    }
}
