//! ref: composer/src/Composer/Command/RepositoryCommand.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::console::completion::completion_input::CompletionInput;
use shirabe_external_packages::symfony::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{
    InvalidArgumentException, PHP_URL_HOST, PhpMixed, RuntimeException, parse_url, strtolower,
};

use crate::command::base_command::BaseCommand;
use crate::command::base_config_command::BaseConfigCommand;
use crate::composer::Composer;
use crate::config::Config;
use crate::config::json_config_source::JsonConfigSource;
use crate::console::input::input_argument::InputArgument;
use crate::console::input::input_option::InputOption;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;

#[derive(Debug)]
pub struct RepositoryCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,

    config: Option<Config>,
    config_file: Option<JsonFile>,
    config_source: Option<JsonConfigSource>,
}

impl RepositoryCommand {
    pub fn configure(&mut self) {
        let suggest_repo_names_before = self.suggest_repo_names();
        let suggest_repo_names_after = self.suggest_repo_names();
        let suggest_repo_names_name = self.suggest_repo_names();
        let suggest_type_for_add = Self::suggest_type_for_add();
        self.inner.inner
            .set_name("repository")
            .set_aliases(vec!["repo".to_string()])
            .set_description("Manages repositories")
            .set_definition(vec![
                InputOption::new("global", Some(PhpMixed::String("g".to_string())), Some(InputOption::VALUE_NONE), "Apply command to the global config file", None, vec![]),
                InputOption::new("file", Some(PhpMixed::String("f".to_string())), Some(InputOption::VALUE_REQUIRED), "If you want to choose a different composer.json or config.json", None, vec![]),
                InputOption::new("append", None, Some(InputOption::VALUE_NONE), "When adding a repository, append it (lower priority) instead of prepending it", None, vec![]),
                InputOption::new("before", None, Some(InputOption::VALUE_REQUIRED), "When adding a repository, insert it before the given repository name", None, suggest_repo_names_before),
                InputOption::new("after", None, Some(InputOption::VALUE_REQUIRED), "When adding a repository, insert it after the given repository name", None, suggest_repo_names_after),
                InputArgument::new("action", Some(InputArgument::OPTIONAL), "Action to perform: list, add, remove, set-url, get-url, enable, disable", Some(PhpMixed::String("list".to_string())), vec!["list", "add", "remove", "set-url", "get-url", "enable", "disable"]),
                InputArgument::new("name", Some(InputArgument::OPTIONAL), "Repository name (or special name packagist.org for enable/disable)", None, suggest_repo_names_name),
                InputArgument::new("arg1", Some(InputArgument::OPTIONAL), "Type for add, or new URL for set-url, or JSON config for add", None, suggest_type_for_add),
                InputArgument::new("arg2", Some(InputArgument::OPTIONAL), "URL for add (if not using JSON)", None, vec![]),
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

        let config_data = self.inner.config_file.as_ref().unwrap().read()?;
        let config_file_path = self
            .inner
            .config_file
            .as_ref()
            .unwrap()
            .get_path()
            .to_string();
        self.inner
            .config
            .as_mut()
            .unwrap()
            .merge(config_data, &config_file_path);
        let repos = self.inner.config.as_ref().unwrap().get_repositories();

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
                    self.inner
                        .config_source
                        .as_mut()
                        .unwrap()
                        .insert_repository(
                            name.as_deref().unwrap(),
                            repo_config,
                            reference_name,
                            offset,
                        );
                    return Ok(0);
                }

                let append = input.get_option("append").as_bool().unwrap_or(false);
                self.inner.config_source.as_mut().unwrap().add_repository(
                    name.as_deref().unwrap(),
                    repo_config,
                    append,
                );
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
                self.inner
                    .config_source
                    .as_mut()
                    .unwrap()
                    .remove_repository(name_str);
                if ["packagist", "packagist.org"].contains(&name_str) {
                    self.inner.config_source.as_mut().unwrap().add_repository(
                        "packagist.org",
                        PhpMixed::Bool(false),
                        false,
                    );
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
                self.inner
                    .config_source
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
                            self.inner.inner.get_io().write(url);
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
                                    self.inner.inner.get_io().write(url);
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
                    self.inner.config_source.as_mut().unwrap().add_repository(
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
                    self.inner
                        .config_source
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

    fn list_repositories(&self, mut repos: IndexMap<String, PhpMixed>) {
        let io = self.inner.inner.get_io();

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
                    .unwrap_or_else(|| JsonFile::encode(repo));
                io.write(&format!("[{}] <info>{}</info> {}", name, r#type, url));
            }
        }
    }

    fn suggest_type_for_add() -> Box<dyn Fn(&CompletionInput) -> Vec<String>> {
        Box::new(|input: &CompletionInput| {
            if input.get_argument("action").as_string() == Some("add") {
                vec![
                    "composer".to_string(),
                    "vcs".to_string(),
                    "artifact".to_string(),
                    "path".to_string(),
                ]
            } else {
                vec![]
            }
        })
    }

    fn suggest_repo_names(&self) -> Box<dyn Fn(&CompletionInput) -> Vec<String> + '_> {
        Box::new(move |input: &CompletionInput| {
            let action = input
                .get_argument("action")
                .as_string()
                .unwrap_or("")
                .to_string();
            if ["enable", "disable"].contains(&action.as_str()) {
                return vec!["packagist.org".to_string()];
            }
            if !["remove", "set-url", "get-url"].contains(&action.as_str()) {
                return vec![];
            }
            let config = Factory::create_config(None, None).unwrap();
            let config_file_path = self.inner.get_composer_config_file(input, &config);
            let config_file = JsonFile::new(config_file_path, None, None);
            let data = config_file.read().unwrap_or_default();
            let mut repos = vec![];
            if let Some(repositories) = data.get("repositories").and_then(|v| v.as_list()) {
                for repo in repositories {
                    if let PhpMixed::Array(ref repo_map) = **repo {
                        if let Some(name) = repo_map.get("name").and_then(|v| v.as_string()) {
                            repos.push(name.to_string());
                        }
                    }
                }
            }
            repos.sort();
            repos
        })
    }
}

impl BaseCommand for RepositoryCommand {
    fn inner(&self) -> &Command {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Command {
        &mut self.inner
    }

    fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    fn composer_mut(&mut self) -> &mut Option<Composer> {
        &mut self.composer
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.io.as_deref()
    }

    fn io_mut(&mut self) -> &mut Option<Box<dyn IOInterface>> {
        &mut self.io
    }
}

impl BaseConfigCommand for RepositoryCommand {
    fn config(&self) -> Option<&Config> {
        self.config.as_ref()
    }

    fn config_mut(&mut self) -> Option<&mut Config> {
        self.config.as_mut()
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
}
