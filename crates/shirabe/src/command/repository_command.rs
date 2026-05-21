//! ref: composer/src/Composer/Command/RepositoryCommand.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_php_shim::{
    InvalidArgumentException, PHP_URL_HOST, PhpMixed, RuntimeException, parse_url, strtolower,
};

use crate::command::BaseConfigCommand;
use crate::command::{BaseCommand, BaseCommandData, HasBaseCommandData};
use crate::config::Config;
use crate::config::ConfigSourceInterface;
use crate::config::JsonConfigSource;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::factory::Factory;
use crate::io::IOInterface;
use crate::json::JsonFile;

#[derive(Debug)]
pub struct RepositoryCommand {
    base_command_data: BaseCommandData,

    config: Option<std::rc::Rc<std::cell::RefCell<Config>>>,
    config_file: Option<JsonFile>,
    config_source: Option<JsonConfigSource>,
}

impl RepositoryCommand {
    pub fn configure(&mut self) {
        // TODO(cli-completion): suggest_repo_names() / suggest_type_for_add()
        self
            .set_name("repository")
            .set_aliases(&["repo".to_string()])
            .set_description("Manages repositories")
            .set_definition(&[
                InputOption::new("global", Some(PhpMixed::String("g".to_string())), Some(InputOption::VALUE_NONE), "Apply command to the global config file", None).unwrap().into(),
                InputOption::new("file", Some(PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "If you want to choose a different composer.json or config.json", None).unwrap().into(),
                InputOption::new("append", None, Some(InputOption::VALUE_NONE), "When adding a repository, append it (lower priority) instead of prepending it", None).unwrap().into(),
                InputOption::new("before", None, Some(InputOption::VALUE_REQUIRED), "When adding a repository, insert it before the given repository name", None).unwrap().into(),
                InputOption::new("after", None, Some(InputOption::VALUE_REQUIRED), "When adding a repository, insert it after the given repository name", None).unwrap().into(),
                InputArgument::new("action", Some(InputArgument::OPTIONAL), "Action to perform: list, add, remove, set-url, get-url, enable, disable", Some(PhpMixed::String("list".to_string()))).unwrap().into(),
                InputArgument::new("name", Some(InputArgument::OPTIONAL), "Repository name (or special name packagist.org for enable/disable)", None).unwrap().into(),
                InputArgument::new("arg1", Some(InputArgument::OPTIONAL), "Type for add, or new URL for set-url, or JSON config for add", None).unwrap().into(),
                InputArgument::new("arg2", Some(InputArgument::OPTIONAL), "URL for add (if not using JSON)", None).unwrap().into(),
            ])
            .set_help(
                "This command lets you manage repositories in your composer.json.\n\n\
                Examples:\n  \
                composer repo list\n  \
                composer repo add foo vcs https://github.com/acme/foo\n  \
                composer repo add bar composer https://repo.packagist.com/bar\n  \
                composer repo add zips '{\"type\":\"artifact\",\"url\":\"/path/to/dir/with/zips\"}'\n  \
                composer repo add baz vcs https://example.org --before foo\n  \
                composer repo add qux vcs https://example.org --after bar\n  \
                composer repo remove foo\n  \
                composer repo set-url foo https://git.example.org/acme/foo\n  \
                composer repo get-url foo\n  \
                composer repo disable packagist.org\n  \
                composer repo enable packagist.org\n\n\
                Use --global/-g to alter the global config.json instead.\n\
                Use --file to alter a specific file."
            );
    }

    pub fn execute(
        &mut self,
        input: &dyn InputInterface,
        _output: &dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        let action = strtolower(
            &input
                .get_argument("action")
                .as_string()
                .unwrap_or("")
                .to_string(),
        );
        let name = input
            .get_argument("name")
            .as_string()
            .map(|s| s.to_string());
        let arg1 = input
            .get_argument("arg1")
            .as_string()
            .map(|s| s.to_string());
        let arg2 = input
            .get_argument("arg2")
            .as_string()
            .map(|s| s.to_string());

        let config_data = self.config_file.as_mut().unwrap().read()?;
        let config_file_path = self.config_file.as_ref().unwrap().get_path().to_string();
        let config_data_map: IndexMap<String, PhpMixed> = match config_data {
            PhpMixed::Array(m) => m.into_iter().map(|(k, v)| (k, *v)).collect(),
            _ => IndexMap::new(),
        };
        self.config
            .as_mut()
            .unwrap()
            .borrow_mut()
            .merge(&config_data_map, &config_file_path);
        let repos = self.config.as_ref().unwrap().borrow().get_repositories();

        match action.as_str() {
            "list" | "ls" | "show" => {
                self.list_repositories(repos);
                Ok(0)
            }
            "add" => {
                if name.is_none() {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: "You must pass a repository name. Example: composer repo add foo vcs https://example.org".to_string(),
                        code: 0,
                    }));
                }
                if arg1.is_none() {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: "You must pass the type and a url, or a JSON string.".to_string(),
                        code: 0,
                    }));
                }
                let arg1_str = arg1.as_deref().unwrap();
                let repo_config: PhpMixed = if Preg::is_match(r"^\s*\{", arg1_str).unwrap_or(false)
                {
                    JsonFile::parse_json(Some(arg1_str), None)?
                } else {
                    if arg2.is_none() {
                        return Err(anyhow::anyhow!(RuntimeException {
                            message: "You must pass the type and a url. Example: composer repo add foo vcs https://example.org".to_string(),
                            code: 0,
                        }));
                    }
                    let mut m = IndexMap::new();
                    m.insert(
                        "type".to_string(),
                        Box::new(PhpMixed::String(arg1_str.to_string())),
                    );
                    m.insert("url".to_string(), Box::new(PhpMixed::String(arg2.unwrap())));
                    PhpMixed::Array(m)
                };

                let before = input
                    .get_option("before")
                    .as_string()
                    .map(|s| s.to_string());
                let after = input.get_option("after").as_string().map(|s| s.to_string());
                if before.is_some() && after.is_some() {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: "You can not combine --before and --after".to_string(),
                        code: 0,
                    }));
                }

                if before.is_some() || after.is_some() {
                    if matches!(repo_config, PhpMixed::Bool(false)) {
                        return Err(anyhow::anyhow!(RuntimeException {
                            message: "Cannot use --before/--after with boolean repository values"
                                .to_string(),
                            code: 0,
                        }));
                    }
                    let reference_name = before.as_deref().or(after.as_deref()).unwrap();
                    let offset: i64 = if after.is_some() { 1 } else { 0 };
                    self.config_source.as_mut().unwrap().insert_repository(
                        name.as_deref().unwrap(),
                        repo_config.clone(),
                        reference_name,
                        offset,
                    )?;
                    return Ok(0);
                }

                let append = input.get_option("append").as_bool().unwrap_or(false);
                self.config_source.as_mut().unwrap().add_repository(
                    name.as_deref().unwrap(),
                    repo_config.clone(),
                    append,
                )?;
                Ok(0)
            }
            "remove" | "rm" | "delete" => {
                if name.is_none() {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: "You must pass the repository name to remove.".to_string(),
                        code: 0,
                    }));
                }
                let name_str = name.as_deref().unwrap();
                self.config_source
                    .as_mut()
                    .unwrap()
                    .remove_repository(name_str)?;
                if ["packagist", "packagist.org"].contains(&name_str) {
                    self.config_source.as_mut().unwrap().add_repository(
                        "packagist.org",
                        PhpMixed::Null,
                        false,
                    )?;
                }
                Ok(0)
            }
            "set-url" | "seturl" => {
                if name.is_none() || arg1.is_none() {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: "Usage: composer repo set-url <name> <new-url>".to_string(),
                        code: 0,
                    }));
                }
                self.config_source
                    .as_mut()
                    .unwrap()
                    .set_repository_url(name.as_deref().unwrap(), arg1.as_deref().unwrap());
                Ok(0)
            }
            "get-url" | "geturl" => {
                if name.is_none() {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: "Usage: composer repo get-url <name>".to_string(),
                        code: 0,
                    }));
                }
                let name_str = name.as_deref().unwrap();
                if let Some(repo) = repos.get(name_str) {
                    if let PhpMixed::Array(ref repo_map) = *repo {
                        let url = repo_map.get("url").and_then(|v| v.as_string());
                        if let Some(url) = url {
                            self.get_io().write(url);
                            return Ok(0);
                        }
                        return Err(anyhow::anyhow!(InvalidArgumentException {
                            message: format!("The {} repository does not have a URL", name_str),
                            code: 0,
                        }));
                    }
                }
                for (_key, val) in &repos {
                    if let PhpMixed::Array(ref repo_map) = *val {
                        if let Some(n) = repo_map.get("name").and_then(|v| v.as_string()) {
                            if n == name_str {
                                let url = repo_map.get("url").and_then(|v| v.as_string());
                                if let Some(url) = url {
                                    self.get_io().write(url);
                                    return Ok(0);
                                }
                                return Err(anyhow::anyhow!(InvalidArgumentException {
                                    message: format!(
                                        "The {} repository does not have a URL",
                                        name_str
                                    ),
                                    code: 0,
                                }));
                            }
                        }
                    }
                }
                Err(anyhow::anyhow!(InvalidArgumentException {
                    message: format!("There is no {} repository defined", name_str),
                    code: 0,
                }))
            }
            "disable" => {
                if name.is_none() {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: "Usage: composer repo disable packagist.org".to_string(),
                        code: 0,
                    }));
                }
                let name_str = name.as_deref().unwrap();
                if ["packagist", "packagist.org"].contains(&name_str) {
                    let append = input.get_option("append").as_bool().unwrap_or(false);
                    self.config_source.as_mut().unwrap().add_repository(
                        "packagist.org",
                        PhpMixed::Bool(false),
                        append,
                    );
                    return Ok(0);
                }
                Err(anyhow::anyhow!(RuntimeException {
                    message: "Only packagist.org can be enabled/disabled using this command. Use add/remove for other repositories.".to_string(),
                    code: 0,
                }))
            }
            "enable" => {
                if name.is_none() {
                    return Err(anyhow::anyhow!(RuntimeException {
                        message: "Usage: composer repo enable packagist.org".to_string(),
                        code: 0,
                    }));
                }
                let name_str = name.as_deref().unwrap();
                if ["packagist", "packagist.org"].contains(&name_str) {
                    self.config_source
                        .as_mut()
                        .unwrap()
                        .remove_repository("packagist.org");
                    return Ok(0);
                }
                Err(anyhow::anyhow!(RuntimeException {
                    message: "Only packagist.org can be enabled/disabled using this command."
                        .to_string(),
                    code: 0,
                }))
            }
            _ => Err(anyhow::anyhow!(InvalidArgumentException {
                message: format!(
                    "Unknown action \"{}\". Use list, add, remove, set-url, get-url, enable, disable",
                    action
                ),
                code: 0,
            })),
        }
    }

    fn list_repositories(&mut self, mut repos: IndexMap<String, PhpMixed>) {
        let io = self.get_io();

        let mut packagist_present = false;
        for (_key, repo) in &repos {
            if let PhpMixed::Array(ref repo_map) = *repo {
                let has_type_and_url =
                    repo_map.contains_key("type") && repo_map.contains_key("url");
                let is_composer_type =
                    repo_map.get("type").and_then(|v| v.as_string()) == Some("composer");
                let url_host_ends_with_packagist = repo_map
                    .get("url")
                    .and_then(|v| v.as_string())
                    .map(|url| {
                        parse_url(url, PHP_URL_HOST)
                            .as_string()
                            .unwrap_or("")
                            .ends_with("packagist.org")
                    })
                    .unwrap_or(false);
                if has_type_and_url && is_composer_type && url_host_ends_with_packagist {
                    packagist_present = true;
                    break;
                }
            }
        }
        if !packagist_present {
            let mut packagist_entry = IndexMap::new();
            packagist_entry.insert("packagist.org".to_string(), Box::new(PhpMixed::Bool(false)));
            repos.insert(repos.len().to_string(), PhpMixed::Array(packagist_entry));
        }

        if repos.is_empty() {
            io.write("No repositories configured");
            return;
        }

        for (key, repo) in &repos {
            if matches!(*repo, PhpMixed::Bool(false)) {
                io.write(&format!("[{}] <info>disabled</info>", key));
                continue;
            }

            if let PhpMixed::Array(ref repo_map) = *repo {
                if repo_map.len() == 1 {
                    if let Some(first_val) = repo_map.values().next() {
                        if matches!(**first_val, PhpMixed::Bool(false)) {
                            let first_key = repo_map.keys().next().unwrap();
                            io.write(&format!("[{}] <info>disabled</info>", first_key));
                            continue;
                        }
                    }
                }

                let name = repo_map
                    .get("name")
                    .and_then(|v| v.as_string())
                    .unwrap_or(key.as_str());
                let r#type = repo_map
                    .get("type")
                    .and_then(|v| v.as_string())
                    .unwrap_or("unknown");
                let url = repo_map
                    .get("url")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| JsonFile::encode(repo, 448));
                io.write(&format!("[{}] <info>{}</info> {}", name, r#type, url));
            }
        }
    }

    // TODO(cli-completion): fn suggest_type_for_add()
    // TODO(cli-completion): fn suggest_repo_names(&self)
}

impl BaseConfigCommand for RepositoryCommand {
    fn config(&self) -> Option<&std::rc::Rc<std::cell::RefCell<Config>>> {
        self.config.as_ref()
    }

    fn config_mut(&mut self) -> &mut Option<std::rc::Rc<std::cell::RefCell<Config>>> {
        &mut self.config
    }

    fn config_file(&self) -> Option<&JsonFile> {
        self.config_file.as_ref()
    }

    fn config_file_mut(&mut self) -> Option<&mut JsonFile> {
        self.config_file.as_mut()
    }

    fn config_source(&self) -> Option<&JsonConfigSource> {
        self.config_source.as_ref()
    }

    fn config_source_mut(&mut self) -> Option<&mut JsonConfigSource> {
        self.config_source.as_mut()
    }

    fn set_config_file(&mut self, file: Option<JsonFile>) {
        self.config_file = file;
    }

    fn set_config_source(&mut self, source: Option<JsonConfigSource>) {
        self.config_source = source;
    }
}

impl HasBaseCommandData for RepositoryCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    fn base_command_data_mut(&mut self) -> &mut BaseCommandData {
        &mut self.base_command_data
    }
}
