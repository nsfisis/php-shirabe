//! ref: composer/src/Composer/Command/RepositoryCommand.php

use crate::command::BaseConfigCommand;
use crate::command::{BaseCommand, BaseCommandData};
use crate::config::Config;
use crate::config::ConfigSourceInterface;
use crate::config::JsonConfigSource;
use crate::console::input::InputArgument;
use crate::console::input::InputOption;
use crate::io::IOInterfaceImmutable;
use crate::json::JsonFile;
use indexmap::IndexMap;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, impl_php_class, parse_url, php_regex,
    preg_is_match, strtolower,
};
use shirabe_symfony_console::command::Command;
use shirabe_symfony_console::input::InputInterface;
use shirabe_symfony_console::input::InputValue;
use shirabe_symfony_console::output::OutputInterface;

#[derive(Debug)]
pub struct RepositoryCommand {
    base_command_data: BaseCommandData,

    config: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<Config>>>>,
    config_file: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<JsonFile>>>>,
    config_source: std::cell::RefCell<Option<JsonConfigSource>>,
}

impl_php_class!(RepositoryCommand, r"Composer\Command\RepositoryCommand");

impl Default for RepositoryCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl RepositoryCommand {
    pub fn new() -> Self {
        let command = RepositoryCommand {
            base_command_data: BaseCommandData::new(None),
            config: std::cell::RefCell::new(None),
            config_file: std::cell::RefCell::new(None),
            config_source: std::cell::RefCell::new(None),
        };
        command
            .configure()
            .expect("RepositoryCommand::configure uses static, valid metadata");
        command
    }

    fn list_repositories(&self, mut repos: IndexMap<String, PhpMixed>) -> anyhow::Result<()> {
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
                        parse_url(url)
                            .and_then(|parsed| parsed.host)
                            .unwrap_or_default()
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
            packagist_entry.insert("packagist.org".to_string(), PhpMixed::Bool(false));
            repos.insert(repos.len().to_string(), PhpMixed::Array(packagist_entry));
        }

        if repos.is_empty() {
            io.write("No repositories configured");
            return Ok(());
        }

        for (key, repo) in &repos {
            if matches!(*repo, PhpMixed::Bool(false)) {
                io.write(&format!("[{}] <info>disabled</info>", key));
                continue;
            }

            if let PhpMixed::Array(ref repo_map) = *repo {
                if repo_map.len() == 1
                    && let Some(first_val) = repo_map.values().next()
                    && matches!(*first_val, PhpMixed::Bool(false))
                {
                    let first_key = repo_map.keys().next().unwrap();
                    io.write(&format!("[{}] <info>disabled</info>", first_key));
                    continue;
                }

                let name = repo_map
                    .get("name")
                    .and_then(|v| v.as_string())
                    .unwrap_or(key.as_str());
                let r#type = repo_map
                    .get("type")
                    .and_then(|v| v.as_string())
                    .unwrap_or("unknown");
                let url = match repo_map
                    .get("url")
                    .and_then(|v| v.as_string())
                    .map(|s| s.to_string())
                {
                    Some(url) => url,
                    None => JsonFile::encode(repo)?,
                };
                io.write(&format!("[{}] <info>{}</info> {}", name, r#type, url));
            }
        }

        Ok(())
    }

    /// PHP: private function suggestTypeForAdd(): \Closure (a static closure — `this` unused)
    fn suggest_type_for_add(&self) -> crate::console::input::SuggestedValues {
        crate::console::input::SuggestedValues::Closure(Box::new(|_this, input, _suggestions| {
            if input.get_argument("action")?.to_php_string() == "add" {
                return Ok(vec![
                    "composer".to_string(),
                    "vcs".to_string(),
                    "artifact".to_string(),
                    "path".to_string(),
                ]);
            }

            Ok(vec![])
        }))
    }

    fn suggest_repo_names(&self) -> crate::console::input::SuggestedValues {
        crate::console::input::SuggestedValues::Closure(Box::new(|this, input, _suggestions| {
            let action = input.get_argument("action")?.to_php_string();
            if ["enable", "disable"].contains(&action.as_str()) {
                return Ok(vec!["packagist.org".to_string()]);
            }

            if !["remove", "set-url", "get-url"].contains(&action.as_str()) {
                return Ok(vec![]);
            }

            let this = this
                .as_any()
                .downcast_ref::<RepositoryCommand>()
                .expect("suggestRepoNames is bound to RepositoryCommand");
            // PHP passes the CompletionInput itself; the accessors only read from it, so a
            // clone behind a fresh handle is equivalent.
            let input_handle: std::rc::Rc<
                std::cell::RefCell<dyn shirabe_symfony_console::input::InputInterface>,
            > = std::rc::Rc::new(std::cell::RefCell::new(input.clone()));
            let config = crate::factory::Factory::create_config(None, None)?;
            let config_file = JsonFile::new(
                this.get_composer_config_file(input_handle, &config)?,
                None,
                None,
            )?;

            let data = config_file.read()?;
            let mut repos: Vec<String> = vec![];

            if let Some(repositories) = data.as_array().and_then(|d| d.get("repositories")) {
                if let Some(list) = repositories.as_list() {
                    for repo in list {
                        if let Some(name) = repo.as_array().and_then(|r| r.get("name")) {
                            repos.push(name.to_string());
                        }
                    }
                } else if let Some(map) = repositories.as_array() {
                    for repo in map.values() {
                        if let Some(name) = repo.as_array().and_then(|r| r.get("name")) {
                            repos.push(name.to_string());
                        }
                    }
                }
            }

            repos.sort();

            Ok(repos)
        }))
    }
}

impl Command for RepositoryCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("repository")?;
        self.set_aliases(vec!["repo".to_string()])?;
        self.set_description("Manages repositories");
        self.set_definition(&[
            InputOption::new(
                "global",
                Some("g"),
                Some(InputOption::VALUE_NONE),
                "Apply command to the global config file",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "file",
                Some("f"),
                Some(InputOption::VALUE_REQUIRED),
                "If you want to choose a different composer.json or config.json",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new(
                "append",
                None,
                Some(InputOption::VALUE_NONE),
                "When adding a repository, append it (lower priority) instead of prepending it",
                None,
            )
            .unwrap()
            .into(),
            InputOption::new6(
                "before",
                None,
                Some(InputOption::VALUE_REQUIRED),
                "When adding a repository, insert it before the given repository name",
                None,
                self.suggest_repo_names(),
            )
            .unwrap()
            .into(),
            InputOption::new6(
                "after",
                None,
                Some(InputOption::VALUE_REQUIRED),
                "When adding a repository, insert it after the given repository name",
                None,
                self.suggest_repo_names(),
            )
            .unwrap()
            .into(),
            InputArgument::new5(
                "action",
                Some(InputArgument::OPTIONAL),
                "Action to perform: list, add, remove, set-url, get-url, enable, disable",
                Some(InputValue::String("list".to_string())),
                crate::console::input::SuggestedValues::List(vec![
                    "list".to_string(),
                    "add".to_string(),
                    "remove".to_string(),
                    "set-url".to_string(),
                    "get-url".to_string(),
                    "enable".to_string(),
                    "disable".to_string(),
                ]),
            )
            .unwrap()
            .into(),
            InputArgument::new5(
                "name",
                Some(InputArgument::OPTIONAL),
                "Repository name (or special name packagist.org for enable/disable)",
                None,
                self.suggest_repo_names(),
            )
            .unwrap()
            .into(),
            InputArgument::new5(
                "arg1",
                Some(InputArgument::OPTIONAL),
                "Type for add, or new URL for set-url, or JSON config for add",
                None,
                self.suggest_type_for_add(),
            )
            .unwrap()
            .into(),
            InputArgument::new(
                "arg2",
                Some(InputArgument::OPTIONAL),
                "URL for add (if not using JSON)",
                None,
            )
            .unwrap()
            .into(),
        ]);
        self.set_help(
            "This command lets you manage repositories in your composer.json.\n\n\
            Examples:\n  \
            shirabe repo list\n  \
            shirabe repo add foo vcs https://github.com/acme/foo\n  \
            shirabe repo add bar composer https://repo.packagist.com/bar\n  \
            shirabe repo add zips '{\"type\":\"artifact\",\"url\":\"/path/to/dir/with/zips\"}'\n  \
            shirabe repo add baz vcs https://example.org --before foo\n  \
            shirabe repo add qux vcs https://example.org --after bar\n  \
            shirabe repo remove foo\n  \
            shirabe repo set-url foo https://git.example.org/acme/foo\n  \
            shirabe repo get-url foo\n  \
            shirabe repo disable packagist.org\n  \
            shirabe repo enable packagist.org\n\n\
            Use --global/-g to alter the global config.json instead.\n\
            Use --file to alter a specific file.",
        );
        Ok(())
    }

    fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        _output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        let action = strtolower(
            input
                .borrow()
                .get_argument("action")?
                .as_string()
                .unwrap_or(""),
        );
        let name = input
            .borrow()
            .get_argument("name")?
            .as_string()
            .map(|s| s.to_string());
        let arg1 = input
            .borrow()
            .get_argument("arg1")?
            .as_string()
            .map(|s| s.to_string());
        let arg2 = input
            .borrow()
            .get_argument("arg2")?
            .as_string()
            .map(|s| s.to_string());

        let config_file = self.config_file.borrow().as_ref().unwrap().clone();
        let config_data = config_file.borrow_mut().read()?;
        let config_file_path = config_file.borrow().get_path().to_string();
        let config_data_map: IndexMap<String, PhpMixed> = match config_data {
            PhpMixed::Array(m) => m.into_iter().collect(),
            _ => IndexMap::new(),
        };
        let config = self.config.borrow().as_ref().unwrap().clone();
        config
            .borrow_mut()
            .merge(&config_data_map, &config_file_path);
        let repos = config.borrow().get_repositories();

        match action.as_str() {
            "list" | "ls" | "show" => {
                self.list_repositories(repos)?;
                Ok(0)
            }
            "add" => {
                if name.is_none() {
                    return Err(RuntimeException::new("You must pass a repository name. Example: shirabe repo add foo vcs https://example.org".to_string()).into());
                }
                if arg1.is_none() {
                    return Err(RuntimeException::new(
                        "You must pass the type and a url, or a JSON string.".to_string(),
                    )
                    .into());
                }
                let arg1_str = arg1.as_deref().unwrap();
                let repo_config: PhpMixed = if preg_is_match(php_regex!(r"{^\s*\{}"), arg1_str) {
                    JsonFile::parse_json(Some(arg1_str), None)?
                } else {
                    if arg2.is_none() {
                        return Err(RuntimeException::new("You must pass the type and a url. Example: shirabe repo add foo vcs https://example.org".to_string()).into());
                    }
                    let mut m = IndexMap::new();
                    m.insert("type".to_string(), PhpMixed::String(arg1_str.to_string()));
                    m.insert("url".to_string(), PhpMixed::String(arg2.unwrap()));
                    PhpMixed::Array(m)
                };

                let before = input
                    .borrow()
                    .get_option("before")?
                    .as_string()
                    .map(|s| s.to_string());
                let after = input
                    .borrow()
                    .get_option("after")?
                    .as_string()
                    .map(|s| s.to_string());
                if before.is_some() && after.is_some() {
                    return Err(RuntimeException::new(
                        "You can not combine --before and --after".to_string(),
                    )
                    .into());
                }

                if before.is_some() || after.is_some() {
                    if matches!(repo_config, PhpMixed::Bool(false)) {
                        return Err(RuntimeException::new(
                            "Cannot use --before/--after with boolean repository values"
                                .to_string(),
                        )
                        .into());
                    }
                    let reference_name = before.as_deref().or(after.as_deref()).unwrap();
                    let offset: i64 = if after.is_some() { 1 } else { 0 };
                    self.config_source
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .insert_repository(
                            name.as_deref().unwrap(),
                            repo_config,
                            reference_name,
                            offset,
                        )?;
                    return Ok(0);
                }

                let append = input
                    .borrow()
                    .get_option("append")?
                    .as_bool()
                    .unwrap_or(false);
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .add_repository(name.as_deref().unwrap(), repo_config, append)?;
                Ok(0)
            }
            "remove" | "rm" | "delete" => {
                if name.is_none() {
                    return Err(RuntimeException::new(
                        "You must pass the repository name to remove.".to_string(),
                    )
                    .into());
                }
                let name_str = name.as_deref().unwrap();
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_repository(name_str)?;
                if ["packagist", "packagist.org"].contains(&name_str) {
                    self.config_source
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .add_repository("packagist.org", PhpMixed::Null, false)?;
                }
                Ok(0)
            }
            "set-url" | "seturl" => {
                if name.is_none() || arg1.is_none() {
                    return Err(RuntimeException::new(
                        "Usage: shirabe repo set-url <name> <new-url>".to_string(),
                    )
                    .into());
                }
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .set_repository_url(name.as_deref().unwrap(), arg1.as_deref().unwrap());
                Ok(0)
            }
            "get-url" | "geturl" => {
                if name.is_none() {
                    return Err(RuntimeException::new(
                        "Usage: shirabe repo get-url <name>".to_string(),
                    )
                    .into());
                }
                let name_str = name.as_deref().unwrap();
                if let Some(repo) = repos.get(name_str)
                    && let PhpMixed::Array(ref repo_map) = *repo
                {
                    let url = repo_map.get("url").and_then(|v| v.as_string());
                    if let Some(url) = url {
                        self.get_io().write(url);
                        return Ok(0);
                    }
                    return Err(InvalidArgumentException::new(format!(
                        "The {} repository does not have a URL",
                        name_str
                    ))
                    .into());
                }
                for (_key, val) in &repos {
                    if let PhpMixed::Array(ref repo_map) = *val
                        && let Some(n) = repo_map.get("name").and_then(|v| v.as_string())
                        && n == name_str
                    {
                        let url = repo_map.get("url").and_then(|v| v.as_string());
                        if let Some(url) = url {
                            self.get_io().write(url);
                            return Ok(0);
                        }
                        return Err(InvalidArgumentException::new(format!(
                            "The {} repository does not have a URL",
                            name_str
                        ))
                        .into());
                    }
                }
                Err(InvalidArgumentException::new(format!(
                    "There is no {} repository defined",
                    name_str
                ))
                .into())
            }
            "disable" => {
                if name.is_none() {
                    return Err(RuntimeException::new(
                        "Usage: shirabe repo disable packagist.org".to_string(),
                    )
                    .into());
                }
                let name_str = name.as_deref().unwrap();
                if ["packagist", "packagist.org"].contains(&name_str) {
                    let append = input
                        .borrow()
                        .get_option("append")?
                        .as_bool()
                        .unwrap_or(false);
                    self.config_source
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .add_repository("packagist.org", PhpMixed::Bool(false), append);
                    return Ok(0);
                }
                Err(RuntimeException::new("Only packagist.org can be enabled/disabled using this command. Use add/remove for other repositories.".to_string()).into())
            }
            "enable" => {
                if name.is_none() {
                    return Err(RuntimeException::new(
                        "Usage: shirabe repo enable packagist.org".to_string(),
                    )
                    .into());
                }
                let name_str = name.as_deref().unwrap();
                if ["packagist", "packagist.org"].contains(&name_str) {
                    self.config_source
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .remove_repository("packagist.org");
                    return Ok(0);
                }
                Err(RuntimeException::new(
                    "Only packagist.org can be enabled/disabled using this command.".to_string(),
                )
                .into())
            }
            _ => Err(InvalidArgumentException::new(format!(
                "Unknown action \"{}\". Use list, add, remove, set-url, get-url, enable, disable",
                action
            ))
            .into()),
        }
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        <Self as crate::command::base_config_command::BaseConfigCommand>::initialize(
            self, input, output,
        )
    }

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        crate::command::base_command::base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for RepositoryCommand {
    fn base_command_data(&self) -> &crate::command::BaseCommandData {
        &self.base_command_data
    }

    crate::delegate_base_command_trait_impls_to_inner!(base_command_data);
}

impl BaseConfigCommand for RepositoryCommand {
    fn config(&self) -> Option<std::rc::Rc<std::cell::RefCell<Config>>> {
        self.config.borrow().clone()
    }

    fn set_config(&self, config: Option<std::rc::Rc<std::cell::RefCell<Config>>>) {
        *self.config.borrow_mut() = config;
    }

    fn config_file(&self) -> Option<std::rc::Rc<std::cell::RefCell<JsonFile>>> {
        self.config_file.borrow().clone()
    }

    fn set_config_file(&self, file: Option<std::rc::Rc<std::cell::RefCell<JsonFile>>>) {
        *self.config_file.borrow_mut() = file;
    }

    fn set_config_source(&self, source: Option<JsonConfigSource>) {
        *self.config_source.borrow_mut() = source;
    }
}
