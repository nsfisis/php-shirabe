//! ref: composer/src/Composer/Command/ConfigCommand.php

use crate::io::io_interface;
use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::console::command::command::Command;
use shirabe_external_packages::symfony::component::console::completion::completion_input::CompletionInput;
use shirabe_external_packages::symfony::component::console::input::input_interface::InputInterface;
use shirabe_external_packages::symfony::component::console::input::input_option::InputOption;
use shirabe_external_packages::symfony::component::console::output::output_interface::OutputInterface;
use shirabe_php_shim::{
    ArrayObject, InvalidArgumentException, JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE,
    JsonObject, PhpMixed, RuntimeException, array_filter, array_filter_use_key, array_is_list,
    array_map, array_merge, array_unique, call_user_func, count, escapeshellcmd, exec, explode,
    file_exists, file_get_contents, implode, in_array, is_array, is_bool, is_dir, is_numeric,
    is_object, is_string, json_encode, key, sort, sprintf, str_replace, str_starts_with, strpos,
    strtolower, system, touch, var_export,
};

use crate::advisory::auditor::Auditor;
use crate::command::base_command::BaseCommand;
use crate::command::base_config_command::BaseConfigCommand;
use crate::composer::Composer;
use crate::config::Config;
use crate::config::json_config_source::JsonConfigSource;
use crate::console::input::input_argument::InputArgument;
use crate::factory::Factory;
use crate::io::io_interface::IOInterface;
use crate::json::json_file::JsonFile;
use crate::package::base_package::{self, BasePackage};
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;
use crate::util::silencer::Silencer;
use shirabe_semver::version_parser::VersionParser;

#[derive(Debug)]
pub struct ConfigCommand {
    inner: Command,
    composer: Option<Composer>,
    io: Option<Box<dyn IOInterface>>,

    config: Option<Config>,
    config_file: Option<JsonFile>,
    config_source: Option<JsonConfigSource>,

    pub(crate) auth_config_file: Option<JsonFile>,
    pub(crate) auth_config_source: Option<JsonConfigSource>,
}

impl ConfigCommand {
    /// List of additional configurable package-properties
    pub(crate) const CONFIGURABLE_PACKAGE_PROPERTIES: &'static [&'static str] = &[
        "name",
        "type",
        "description",
        "homepage",
        "version",
        "minimum-stability",
        "prefer-stable",
        "keywords",
        "license",
        "repositories",
        "suggest",
        "extra",
    ];

    pub(crate) fn configure(&mut self) {
        let suggest_setting_keys = self.suggest_setting_keys();
        self.inner
            .inner
            .set_name("config")
            .set_description("Sets config options")
            .set_definition(vec![
                InputOption::new("global", Some("g"), Some(InputOption::VALUE_NONE), "Apply command to the global config file", None, vec![]),
                InputOption::new("editor", Some("e"), Some(InputOption::VALUE_NONE), "Open editor", None, vec![]),
                InputOption::new("auth", Some("a"), Some(InputOption::VALUE_NONE), "Affect auth config file (only used for --editor)", None, vec![]),
                InputOption::new("unset", None, Some(InputOption::VALUE_NONE), "Unset the given setting-key", None, vec![]),
                InputOption::new("list", Some("l"), Some(InputOption::VALUE_NONE), "List configuration settings", None, vec![]),
                InputOption::new("file", Some("f"), Some(InputOption::VALUE_REQUIRED), "If you want to choose a different composer.json or config.json", None, vec![]),
                InputOption::new("absolute", None, Some(InputOption::VALUE_NONE), "Returns absolute paths when fetching *-dir config values instead of relative", None, vec![]),
                InputOption::new("json", Some("j"), Some(InputOption::VALUE_NONE), "JSON decode the setting value, to be used with extra.* keys", None, vec![]),
                InputOption::new("merge", Some("m"), Some(InputOption::VALUE_NONE), "Merge the setting value with the current value, to be used with extra.* or audit.ignore[-abandoned] keys in combination with --json", None, vec![]),
                InputOption::new("append", None, Some(InputOption::VALUE_NONE), "When adding a repository, append it (lowest priority) to the existing ones instead of prepending it (highest priority)", None, vec![]),
                InputOption::new("source", None, Some(InputOption::VALUE_NONE), "Display where the config value is loaded from", None, vec![]),
                InputArgument::new("setting-key", None, "Setting key", None, suggest_setting_keys),
                InputArgument::new("setting-value", Some(InputArgument::IS_ARRAY), "Setting value", None, Box::new(|_| vec![])),
            ])
            .set_help(
                "This command allows you to edit composer config settings and repositories\n\
                 in either the local composer.json file or the global config.json file.\n\n\
                 Additionally it lets you edit most properties in the local composer.json.\n\n\
                 To set a config setting:\n\n\
                 \t<comment>%command.full_name% bin-dir bin/</comment>\n\n\
                 To read a config setting:\n\n\
                 \t<comment>%command.full_name% bin-dir</comment>\n\
                 \tOutputs: <info>bin</info>\n\n\
                 To edit the global config.json file:\n\n\
                 \t<comment>%command.full_name% --global</comment>\n\n\
                 To add a repository:\n\n\
                 \t<comment>%command.full_name% repositories.foo vcs https://bar.com</comment>\n\n\
                 To remove a repository (repo is a short alias for repositories):\n\n\
                 \t<comment>%command.full_name% --unset repo.foo</comment>\n\n\
                 To disable packagist.org:\n\n\
                 \t<comment>%command.full_name% repo.packagist.org false</comment>\n\n\
                 You can alter repositories in the global config.json file by passing in the\n\
                 <info>--global</info> option.\n\n\
                 To add or edit suggested packages you can use:\n\n\
                 \t<comment>%command.full_name% suggest.package reason for the suggestion</comment>\n\n\
                 To add or edit extra properties you can use:\n\n\
                 \t<comment>%command.full_name% extra.property value</comment>\n\n\
                 Or to add a complex value you can use json with:\n\n\
                 \t<comment>%command.full_name% extra.property --json '{\"foo\":true, \"bar\": []}'</comment>\n\n\
                 To edit the file in an external editor:\n\n\
                 \t<comment>%command.full_name% --editor</comment>\n\n\
                 To choose your editor you can set the \"EDITOR\" env variable.\n\n\
                 To get a list of configuration values in the file:\n\n\
                 \t<comment>%command.full_name% --list</comment>\n\n\
                 You can always pass more than one option. As an example, if you want to edit the\n\
                 global config.json file.\n\n\
                 \t<comment>%command.full_name% --editor --global</comment>\n\n\
                 Read more at https://getcomposer.org/doc/03-cli.md#config",
            );
    }

    pub(crate) fn initialize(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> anyhow::Result<()> {
        self.inner.initialize(input, output)?;

        let auth_config_file = self
            .inner
            .get_auth_config_file(input, self.inner.config.as_ref().unwrap());

        self.auth_config_file = Some(JsonFile::new(
            auth_config_file,
            None,
            Some(self.inner.inner.get_io()),
        ));
        self.auth_config_source = Some(JsonConfigSource::new_with_auth(
            self.auth_config_file.as_ref().unwrap(),
            true,
        ));

        // Initialize the global file if it's not there, ignoring any warnings or notices
        if input.get_option("global").as_bool() == Some(true)
            && !self.auth_config_file.as_ref().unwrap().exists()
        {
            touch(self.auth_config_file.as_ref().unwrap().get_path());
            let mut empty_objs: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            for k in &[
                "bitbucket-oauth",
                "github-oauth",
                "gitlab-oauth",
                "gitlab-token",
                "http-basic",
                "bearer",
                "forgejo-token",
            ] {
                empty_objs.insert(
                    k.to_string(),
                    Box::new(PhpMixed::Object(ArrayObject::new())),
                );
            }
            self.auth_config_file
                .as_mut()
                .unwrap()
                .write(PhpMixed::Array(empty_objs))?;
            let path_clone = self
                .auth_config_file
                .as_ref()
                .unwrap()
                .get_path()
                .to_string();
            Silencer::call(|| {
                shirabe_php_shim::chmod(&path_clone, 0o600);
                Ok(())
            });
        }
        Ok(())
    }

    pub(crate) fn execute(
        &mut self,
        input: &dyn InputInterface,
        output: &dyn OutputInterface,
    ) -> anyhow::Result<i64> {
        // Open file in editor
        if input.get_option("editor").as_bool() == Some(true) {
            let mut editor = Platform::get_env("EDITOR");
            if editor.is_none() || editor.as_deref() == Some("") {
                if Platform::is_windows() {
                    editor = Some("notepad".to_string());
                } else {
                    for candidate in &["editor", "vim", "vi", "nano", "pico", "ed"] {
                        if !exec(&format!("which {}", candidate)).is_empty() {
                            editor = Some(candidate.to_string());
                            break;
                        }
                    }
                }
            } else {
                editor = Some(escapeshellcmd(&editor.unwrap()));
            }

            let file = if input.get_option("auth").as_bool() == Some(true) {
                self.auth_config_file
                    .as_ref()
                    .unwrap()
                    .get_path()
                    .to_string()
            } else {
                self.inner
                    .config_file
                    .as_ref()
                    .unwrap()
                    .get_path()
                    .to_string()
            };
            system(&format!(
                "{} {}{}",
                editor.unwrap_or_default(),
                file,
                if Platform::is_windows() {
                    ""
                } else {
                    " > `tty`"
                }
            ));

            return Ok(0);
        }

        if input.get_option("global").as_bool() != Some(true) {
            self.inner.config.as_mut().unwrap().merge(
                self.inner.config_file.as_ref().unwrap().read()?,
                self.inner.config_file.as_ref().unwrap().get_path(),
            );
            let auth_data: PhpMixed = if self.auth_config_file.as_ref().unwrap().exists() {
                self.auth_config_file.as_ref().unwrap().read()?
            } else {
                PhpMixed::Array(IndexMap::new())
            };
            let mut wrap: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            wrap.insert("config".to_string(), Box::new(auth_data));
            self.inner.config.as_mut().unwrap().merge(
                PhpMixed::Array(wrap),
                self.auth_config_file.as_ref().unwrap().get_path(),
            );
        }

        self.inner
            .inner
            .get_io()
            .load_configuration(self.inner.config.as_ref().unwrap());

        // List the configuration of the file settings
        if input.get_option("list").as_bool() == Some(true) {
            self.list_configuration(
                self.inner.config.as_ref().unwrap().all(),
                self.inner.config.as_ref().unwrap().raw(),
                output,
                None,
                input.get_option("source").as_bool() == Some(true),
            );

            return Ok(0);
        }

        let setting_key_arg = input.get_argument("setting-key");
        let setting_key = match setting_key_arg.as_string() {
            Some(s) => s.to_string(),
            None => return Ok(0),
        };

        // If the user enters in a config variable, parse it and save to file
        let setting_values_raw = input.get_argument("setting-value");
        let setting_values: Vec<String> = setting_values_raw
            .as_list()
            .map(|l| {
                l.iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if !setting_values.is_empty() && input.get_option("unset").as_bool() == Some(true) {
            return Err(RuntimeException {
                message: "You can not combine a setting value with --unset".to_string(),
                code: 0,
            }
            .into());
        }

        // show the value if no value is provided
        if setting_values.is_empty() && input.get_option("unset").as_bool() != Some(true) {
            let properties: Vec<&'static str> = Self::CONFIGURABLE_PACKAGE_PROPERTIES.to_vec();
            let mut properties_defaults: IndexMap<String, PhpMixed> = IndexMap::new();
            properties_defaults.insert("type".to_string(), PhpMixed::String("library".to_string()));
            properties_defaults.insert("description".to_string(), PhpMixed::String(String::new()));
            properties_defaults.insert("homepage".to_string(), PhpMixed::String(String::new()));
            properties_defaults.insert(
                "minimum-stability".to_string(),
                PhpMixed::String("stable".to_string()),
            );
            properties_defaults.insert("prefer-stable".to_string(), PhpMixed::Bool(false));
            properties_defaults.insert("keywords".to_string(), PhpMixed::List(vec![]));
            properties_defaults.insert("license".to_string(), PhpMixed::List(vec![]));
            properties_defaults.insert("suggest".to_string(), PhpMixed::List(vec![]));
            properties_defaults.insert("extra".to_string(), PhpMixed::List(vec![]));
            let raw_data = self.inner.config_file.as_ref().unwrap().read()?;
            let mut data = self.inner.config.as_ref().unwrap().all();
            let mut source = self
                .inner
                .config
                .as_ref()
                .unwrap()
                .get_source_of_value(&setting_key);

            let mut value: PhpMixed;
            let mut matches: Vec<String> = vec![];
            if Preg::is_match(
                "/^repos?(?:itories)?(?:\\.(.+))?/",
                &setting_key,
                Some(&mut matches),
            )
            .unwrap_or(false)
            {
                if matches.get(1).is_none() {
                    value = data
                        .as_array()
                        .and_then(|a| a.get("repositories"))
                        .map(|v| (**v).clone())
                        .unwrap_or_else(|| PhpMixed::Array(IndexMap::new()));
                } else {
                    let repo_key = matches[1].clone();
                    let repos = data
                        .as_array()
                        .and_then(|a| a.get("repositories"))
                        .map(|v| (**v).clone());
                    value = match repos
                        .as_ref()
                        .and_then(|r| r.as_array().and_then(|a| a.get(&repo_key)))
                    {
                        Some(v) => (**v).clone(),
                        None => {
                            return Err(InvalidArgumentException {
                                message: format!("There is no {} repository defined", repo_key),
                                code: 0,
                            }
                            .into());
                        }
                    };
                }
            } else if strpos(&setting_key, ".").is_some() {
                let bits = explode(".", &setting_key);
                if bits[0] == "extra" || bits[0] == "suggest" {
                    data = raw_data.clone();
                } else {
                    data = data
                        .as_array()
                        .and_then(|a| a.get("config"))
                        .map(|v| (**v).clone())
                        .unwrap_or(PhpMixed::Null);
                }
                let mut r#match = false;
                let mut key_acc: Option<String> = None;
                for bit in &bits {
                    let new_key = match &key_acc {
                        Some(k) => format!("{}.{}", k, bit),
                        None => bit.clone(),
                    };
                    key_acc = Some(new_key.clone());
                    r#match = false;
                    if let Some(arr) = data.as_array() {
                        if let Some(v) = arr.get(&new_key) {
                            r#match = true;
                            data = (**v).clone();
                            key_acc = None;
                        }
                    }
                }

                if !r#match {
                    return Err(RuntimeException {
                        message: format!("{} is not defined.", setting_key),
                        code: 0,
                    }
                    .into());
                }

                value = data;
            } else if data
                .as_array()
                .and_then(|a| a.get("config"))
                .and_then(|c| c.as_array())
                .map(|c| c.contains_key(&setting_key))
                .unwrap_or(false)
            {
                value = self.inner.config.as_ref().unwrap().get_with_flags(
                    &setting_key,
                    if input.get_option("absolute").as_bool() == Some(true) {
                        0
                    } else {
                        Config::RELATIVE_PATHS
                    },
                );
                // ensure we get {} output for properties which are objects
                if value.as_array().map(|a| a.is_empty()).unwrap_or(false) {
                    let schema = JsonFile::parse_json(
                        &file_get_contents(JsonFile::COMPOSER_SCHEMA_PATH).unwrap_or_default(),
                        "composer.schema.json",
                    )?;
                    let type_value = schema
                        .as_array()
                        .and_then(|a| a.get("properties"))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("config"))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("properties"))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get(&setting_key))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get("type"))
                        .map(|v| (**v).clone());
                    if let Some(tv) = type_value {
                        let type_array = match &tv {
                            PhpMixed::List(_) | PhpMixed::Array(_) => tv,
                            other => PhpMixed::List(vec![Box::new(other.clone())]),
                        };
                        if in_array(
                            "object",
                            &type_array
                                .as_list()
                                .map(|l| {
                                    l.iter()
                                        .filter_map(|v| v.as_string().map(|s| s.to_string()))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                            true,
                        ) {
                            value = PhpMixed::Object(ArrayObject::new());
                        }
                    }
                }
            } else if raw_data
                .as_array()
                .and_then(|a| a.get(&setting_key))
                .is_some()
                && in_array(setting_key.as_str(), &properties, true)
            {
                value = (**raw_data.as_array().unwrap().get(&setting_key).unwrap()).clone();
                source = self
                    .inner
                    .config_file
                    .as_ref()
                    .unwrap()
                    .get_path()
                    .to_string();
            } else if let Some(v) = properties_defaults.get(&setting_key) {
                value = v.clone();
                source = "defaults".to_string();
            } else {
                return Err(RuntimeException {
                    message: format!("{} is not defined", setting_key),
                    code: 0,
                }
                .into());
            }

            let value_str = if is_array(&value) || is_object(&value) || is_bool(&value) {
                JsonFile::encode(&value, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE)?
            } else {
                value.as_string().unwrap_or("").to_string()
            };

            let mut source_of_config_value = String::new();
            if input.get_option("source").as_bool() == Some(true) {
                source_of_config_value = format!(" ({})", source);
            }

            self.inner.inner.get_io().write(
                &format!("{}{}", value_str, source_of_config_value),
                true,
                io_interface::QUIET,
            );

            return Ok(0);
        }

        let values: Vec<String> = setting_values; // what the user is trying to add/change

        let boolean_validator = |val: &PhpMixed| -> bool {
            in_array(
                val.as_string().unwrap_or(""),
                &vec![
                    "true".to_string(),
                    "false".to_string(),
                    "1".to_string(),
                    "0".to_string(),
                ],
                true,
            )
        };
        let boolean_normalizer = |val: &PhpMixed| -> PhpMixed {
            let s = val.as_string().unwrap_or("");
            PhpMixed::Bool(s != "false" && s != "" && s != "0")
        };

        // handle config values
        let unique_config_values = build_unique_config_values();
        let multi_config_values = build_multi_config_values();

        // allow unsetting audit config entirely
        if input.get_option("unset").as_bool() == Some(true) && setting_key == "audit" {
            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .remove_config_setting(&setting_key);

            return Ok(0);
        }

        if input.get_option("unset").as_bool() == Some(true)
            && (unique_config_values.contains_key(&setting_key)
                || multi_config_values.contains_key(&setting_key))
        {
            if setting_key == "disable-tls"
                && self
                    .inner
                    .config
                    .as_ref()
                    .unwrap()
                    .get("disable-tls")
                    .as_bool()
                    .unwrap_or(false)
            {
                self.inner.inner.get_io().write_error(
                    "<info>You are now running Composer with SSL/TLS protection enabled.</info>",
                );
            }

            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .remove_config_setting(&setting_key);

            return Ok(0);
        }
        if let Some(callbacks) = unique_config_values.get(&setting_key) {
            self.handle_single_value(&setting_key, callbacks, &values, "addConfigSetting")?;

            return Ok(0);
        }
        if let Some(callbacks) = multi_config_values.get(&setting_key) {
            self.handle_multi_value(&setting_key, callbacks, &values, "addConfigSetting")?;

            return Ok(0);
        }
        // handle preferred-install per-package config
        let mut matches: Vec<String> = vec![];
        if Preg::is_match(
            "/^preferred-install\\.(.+)/",
            &setting_key,
            Some(&mut matches),
        )
        .unwrap_or(false)
        {
            if input.get_option("unset").as_bool() == Some(true) {
                self.inner
                    .config_source
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(&setting_key);

                return Ok(0);
            }

            let validator = &unique_config_values.get("preferred-install").unwrap().0;
            if !validator(&PhpMixed::String(values[0].clone()))
                .as_bool()
                .unwrap_or(false)
            {
                return Err(RuntimeException {
                    message: format!(
                        "Invalid value for {}. Should be one of: auto, source, or dist",
                        setting_key
                    ),
                    code: 0,
                }
                .into());
            }

            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .add_config_setting(&setting_key, PhpMixed::String(values[0].clone()));

            return Ok(0);
        }

        // handle allow-plugins config setting elements true or false to add/remove
        let mut matches: Vec<String> = vec![];
        if Preg::is_match(
            "{^allow-plugins\\.([a-zA-Z0-9/*-]+)}",
            &setting_key,
            Some(&mut matches),
        )
        .unwrap_or(false)
        {
            if input.get_option("unset").as_bool() == Some(true) {
                self.inner
                    .config_source
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(&setting_key);

                return Ok(0);
            }

            if !boolean_validator(&PhpMixed::String(values[0].clone())) {
                return Err(RuntimeException {
                    message: sprintf("\"%s\" is an invalid value", &[values[0].clone().into()]),
                    code: 0,
                }
                .into());
            }

            let normalized_value = boolean_normalizer(&PhpMixed::String(values[0].clone()));

            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .add_config_setting(&setting_key, normalized_value);

            return Ok(0);
        }

        // handle properties
        let unique_props = build_unique_props();
        let multi_props = build_multi_props();

        if input.get_option("global").as_bool() == Some(true)
            && (unique_props.contains_key(&setting_key)
                || multi_props.contains_key(&setting_key)
                || strpos(&setting_key, "extra.") == Some(0))
        {
            return Err(InvalidArgumentException {
                message: format!("The {} property can not be set in the global config.json file. Use `composer global config` to apply changes to the global composer.json", setting_key),
                code: 0,
            }
            .into());
        }
        if input.get_option("unset").as_bool() == Some(true)
            && (unique_props.contains_key(&setting_key) || multi_props.contains_key(&setting_key))
        {
            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .remove_property(&setting_key);

            return Ok(0);
        }
        if let Some(callbacks) = unique_props.get(&setting_key) {
            self.handle_single_value(&setting_key, callbacks, &values, "addProperty")?;

            return Ok(0);
        }
        if let Some(callbacks) = multi_props.get(&setting_key) {
            self.handle_multi_value(&setting_key, callbacks, &values, "addProperty")?;

            return Ok(0);
        }

        // handle repositories
        let mut matches: Vec<String> = vec![];
        if Preg::is_match_strict_groups(
            "/^repos?(?:itories)?\\.(.+)/",
            &setting_key,
            Some(&mut matches),
        )
        .unwrap_or(false)
        {
            if input.get_option("unset").as_bool() == Some(true) {
                self.inner
                    .config_source
                    .as_mut()
                    .unwrap()
                    .remove_repository(&matches[1]);

                return Ok(0);
            }

            if 2 == count(&values) {
                let mut repo: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                repo.insert(
                    "type".to_string(),
                    Box::new(PhpMixed::String(values[0].clone())),
                );
                repo.insert(
                    "url".to_string(),
                    Box::new(PhpMixed::String(values[1].clone())),
                );
                self.inner.config_source.as_mut().unwrap().add_repository(
                    &matches[1],
                    PhpMixed::Array(repo),
                    input.get_option("append").as_bool() == Some(true),
                );

                return Ok(0);
            }

            if 1 == count(&values) {
                let value = strtolower(&values[0]);
                if boolean_validator(&PhpMixed::String(value.clone())) {
                    if !boolean_normalizer(&PhpMixed::String(value.clone()))
                        .as_bool()
                        .unwrap_or(false)
                    {
                        self.inner.config_source.as_mut().unwrap().add_repository(
                            &matches[1],
                            PhpMixed::Bool(false),
                            input.get_option("append").as_bool() == Some(true),
                        );

                        return Ok(0);
                    }
                } else {
                    let value = JsonFile::parse_json(&values[0], "composer.json")?;
                    self.inner.config_source.as_mut().unwrap().add_repository(
                        &matches[1],
                        value,
                        input.get_option("append").as_bool() == Some(true),
                    );

                    return Ok(0);
                }
            }

            return Err(RuntimeException {
                message: "You must pass the type and a url. Example: php composer.phar config repositories.foo vcs https://bar.com".to_string(),
                code: 0,
            }
            .into());
        }

        // handle extra
        let mut matches: Vec<String> = vec![];
        if Preg::is_match("/^extra\\.(.+)/", &setting_key, Some(&mut matches)).unwrap_or(false) {
            if input.get_option("unset").as_bool() == Some(true) {
                self.inner
                    .config_source
                    .as_mut()
                    .unwrap()
                    .remove_property(&setting_key);

                return Ok(0);
            }

            let mut value = PhpMixed::String(values[0].clone());
            if input.get_option("json").as_bool() == Some(true) {
                value = JsonFile::parse_json(&values[0], "composer.json")?;
                if input.get_option("merge").as_bool() == Some(true) {
                    let current_value_outer = self.inner.config_file.as_ref().unwrap().read()?;
                    let bits = explode(".", &setting_key);
                    let mut current_value: PhpMixed = current_value_outer;
                    for bit in &bits {
                        current_value = current_value
                            .as_array()
                            .and_then(|a| a.get(bit))
                            .map(|v| (**v).clone())
                            .unwrap_or(PhpMixed::Null);
                    }
                    if is_array(&current_value) && is_array(&value) {
                        if array_is_list(&current_value) && array_is_list(&value) {
                            value = PhpMixed::List(array_merge(
                                current_value.as_list().cloned().unwrap_or_default(),
                                value.as_list().cloned().unwrap_or_default(),
                            ));
                        } else {
                            // PHP "+" operator on arrays: keep keys from left, fill from right
                            let mut merged: IndexMap<String, Box<PhpMixed>> =
                                value.as_array().cloned().unwrap_or_default();
                            if let Some(cv) = current_value.as_array() {
                                for (k, v) in cv {
                                    if !merged.contains_key(k) {
                                        merged.insert(k.clone(), v.clone());
                                    }
                                }
                            }
                            value = PhpMixed::Array(merged);
                        }
                    }
                }
            }
            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .add_property(&setting_key, value);

            return Ok(0);
        }

        // handle suggest
        let mut matches: Vec<String> = vec![];
        if Preg::is_match("/^suggest\\.(.+)/", &setting_key, Some(&mut matches)).unwrap_or(false) {
            if input.get_option("unset").as_bool() == Some(true) {
                self.inner
                    .config_source
                    .as_mut()
                    .unwrap()
                    .remove_property(&setting_key);

                return Ok(0);
            }

            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .add_property(&setting_key, PhpMixed::String(implode(" ", &values)));

            return Ok(0);
        }

        // handle unsetting extra/suggest
        if in_array(
            setting_key.as_str(),
            &vec!["suggest".to_string(), "extra".to_string()],
            true,
        ) && input.get_option("unset").as_bool() == Some(true)
        {
            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .remove_property(&setting_key);

            return Ok(0);
        }

        // handle platform
        let mut matches: Vec<String> = vec![];
        if Preg::is_match("/^platform\\.(.+)/", &setting_key, Some(&mut matches)).unwrap_or(false) {
            if input.get_option("unset").as_bool() == Some(true) {
                self.inner
                    .config_source
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(&setting_key);

                return Ok(0);
            }

            let value = if values[0] == "false" {
                PhpMixed::Bool(false)
            } else {
                PhpMixed::String(values[0].clone())
            };
            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .add_config_setting(&setting_key, value);

            return Ok(0);
        }

        // handle unsetting platform
        if setting_key == "platform" && input.get_option("unset").as_bool() == Some(true) {
            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .remove_config_setting(&setting_key);

            return Ok(0);
        }

        // handle audit.ignore and audit.ignore-abandoned with --merge support
        if in_array(
            setting_key.as_str(),
            &vec![
                "audit.ignore".to_string(),
                "audit.ignore-abandoned".to_string(),
            ],
            true,
        ) {
            if input.get_option("unset").as_bool() == Some(true) {
                self.inner
                    .config_source
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(&setting_key);

                return Ok(0);
            }

            let mut value: PhpMixed = PhpMixed::List(
                values
                    .iter()
                    .map(|s| Box::new(PhpMixed::String(s.clone())))
                    .collect(),
            );
            if input.get_option("json").as_bool() == Some(true) {
                value = JsonFile::parse_json(&values[0], "composer.json")?;
                if !is_array(&value) {
                    return Err(RuntimeException {
                        message: format!("Expected an array or object for {}", setting_key),
                        code: 0,
                    }
                    .into());
                }
            }

            if input.get_option("merge").as_bool() == Some(true) {
                let current_config = self.inner.config_file.as_ref().unwrap().read()?;
                let key_suffix = str_replace("audit.", "", &setting_key);
                let current_value = current_config
                    .as_array()
                    .and_then(|a| a.get("config"))
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get("audit"))
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(&key_suffix))
                    .map(|v| (**v).clone())
                    .unwrap_or(PhpMixed::Null);

                if !current_value.is_null() && is_array(&current_value) && is_array(&value) {
                    if array_is_list(&current_value) && array_is_list(&value) {
                        // Both are lists, merge them
                        value = PhpMixed::List(array_merge(
                            current_value.as_list().cloned().unwrap_or_default(),
                            value.as_list().cloned().unwrap_or_default(),
                        ));
                    } else if !array_is_list(&current_value) && !array_is_list(&value) {
                        // Both are associative arrays (objects), merge them
                        let mut merged: IndexMap<String, Box<PhpMixed>> =
                            value.as_array().cloned().unwrap_or_default();
                        if let Some(cv) = current_value.as_array() {
                            for (k, v) in cv {
                                if !merged.contains_key(k) {
                                    merged.insert(k.clone(), v.clone());
                                }
                            }
                        }
                        value = PhpMixed::Array(merged);
                    } else {
                        return Err(RuntimeException {
                            message: format!("Cannot merge array and object for {}", setting_key),
                            code: 0,
                        }
                        .into());
                    }
                }
            }

            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .add_config_setting(&setting_key, value);

            return Ok(0);
        }

        // handle auth
        let mut matches: Vec<String> = vec![];
        if Preg::is_match(
            "/^(bitbucket-oauth|github-oauth|gitlab-oauth|gitlab-token|http-basic|custom-headers|bearer|forgejo-token)\\.(.+)/",
            &setting_key,
            Some(&mut matches),
        ).unwrap_or(false) {
            if input.get_option("unset").as_bool() == Some(true) {
                self.auth_config_source.as_mut().unwrap().remove_config_setting(&format!("{}.{}", matches[1], matches[2]));
                self.inner.config_source.as_mut().unwrap().remove_config_setting(&format!("{}.{}", matches[1], matches[2]));

                return Ok(0);
            }

            let key = format!("{}.{}", matches[1], matches[2]);
            if matches[1] == "bitbucket-oauth" {
                if 2 != count(&values) {
                    return Err(RuntimeException {
                        message: format!("Expected two arguments (consumer-key, consumer-secret), got {}", count(&values)),
                        code: 0,
                    }
                    .into());
                }
                self.inner.config_source.as_mut().unwrap().remove_config_setting(&key);
                let mut obj: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                obj.insert("consumer-key".to_string(), Box::new(PhpMixed::String(values[0].clone())));
                obj.insert("consumer-secret".to_string(), Box::new(PhpMixed::String(values[1].clone())));
                self.auth_config_source.as_mut().unwrap().add_config_setting(&key, PhpMixed::Array(obj));
            } else if matches[1] == "gitlab-token" && 2 == count(&values) {
                self.inner.config_source.as_mut().unwrap().remove_config_setting(&key);
                let mut obj: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                obj.insert("username".to_string(), Box::new(PhpMixed::String(values[0].clone())));
                obj.insert("token".to_string(), Box::new(PhpMixed::String(values[1].clone())));
                self.auth_config_source.as_mut().unwrap().add_config_setting(&key, PhpMixed::Array(obj));
            } else if in_array(
                matches[1].as_str(),
                &vec!["github-oauth".to_string(), "gitlab-oauth".to_string(), "gitlab-token".to_string(), "bearer".to_string()],
                true,
            ) {
                if 1 != count(&values) {
                    return Err(RuntimeException {
                        message: "Too many arguments, expected only one token".to_string(),
                        code: 0,
                    }
                    .into());
                }
                self.inner.config_source.as_mut().unwrap().remove_config_setting(&key);
                self.auth_config_source.as_mut().unwrap().add_config_setting(&key, PhpMixed::String(values[0].clone()));
            } else if matches[1] == "http-basic" {
                if 2 != count(&values) {
                    return Err(RuntimeException {
                        message: format!("Expected two arguments (username, password), got {}", count(&values)),
                        code: 0,
                    }
                    .into());
                }
                self.inner.config_source.as_mut().unwrap().remove_config_setting(&key);
                let mut obj: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                obj.insert("username".to_string(), Box::new(PhpMixed::String(values[0].clone())));
                obj.insert("password".to_string(), Box::new(PhpMixed::String(values[1].clone())));
                self.auth_config_source.as_mut().unwrap().add_config_setting(&key, PhpMixed::Array(obj));
            } else if matches[1] == "custom-headers" {
                if count(&values) == 0 {
                    return Err(RuntimeException {
                        message: "Expected at least one argument (header), got none".to_string(),
                        code: 0,
                    }
                    .into());
                }

                // Validate headers format
                let mut formatted_headers: Vec<Box<PhpMixed>> = vec![];
                for header in &values {
                    if !is_string(&PhpMixed::String(header.clone())) {
                        return Err(RuntimeException {
                            message: "Headers must be strings in \"Header-Name: Header-Value\" format".to_string(),
                            code: 0,
                        }
                        .into());
                    }

                    // Check if the header is in correct "Name: Value" format
                    let mut header_parts: Vec<String> = vec![];
                    if !Preg::is_match("/^[^:]+:\\s*.+$/", header, Some(&mut header_parts)).unwrap_or(false) {
                        return Err(RuntimeException {
                            message: format!("Header \"{}\" is not in \"Header-Name: Header-Value\" format", header),
                            code: 0,
                        }
                        .into());
                    }

                    formatted_headers.push(Box::new(PhpMixed::String(header.clone())));
                }

                self.inner.config_source.as_mut().unwrap().remove_config_setting(&key);
                self.auth_config_source.as_mut().unwrap().add_config_setting(&key, PhpMixed::List(formatted_headers));
            } else if matches[1] == "forgejo-token" {
                if 2 != count(&values) {
                    return Err(RuntimeException {
                        message: format!("Expected two arguments (username, access token), got {}", count(&values)),
                        code: 0,
                    }
                    .into());
                }
                self.inner.config_source.as_mut().unwrap().remove_config_setting(&key);
                let mut obj: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                obj.insert("username".to_string(), Box::new(PhpMixed::String(values[0].clone())));
                obj.insert("token".to_string(), Box::new(PhpMixed::String(values[1].clone())));
                self.auth_config_source.as_mut().unwrap().add_config_setting(&key, PhpMixed::Array(obj));
            }

            return Ok(0);
        }

        // handle script
        let mut matches: Vec<String> = vec![];
        if Preg::is_match("/^scripts\\.(.+)/", &setting_key, Some(&mut matches)).unwrap_or(false) {
            if input.get_option("unset").as_bool() == Some(true) {
                self.inner
                    .config_source
                    .as_mut()
                    .unwrap()
                    .remove_property(&setting_key);

                return Ok(0);
            }

            let value: PhpMixed = if count(&values) > 1 {
                PhpMixed::List(
                    values
                        .iter()
                        .map(|s| Box::new(PhpMixed::String(s.clone())))
                        .collect(),
                )
            } else {
                PhpMixed::String(values[0].clone())
            };
            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .add_property(&setting_key, value);

            return Ok(0);
        }

        // handle unsetting other top level properties
        if input.get_option("unset").as_bool() == Some(true) {
            self.inner
                .config_source
                .as_mut()
                .unwrap()
                .remove_property(&setting_key);

            return Ok(0);
        }

        Err(InvalidArgumentException {
            message: format!(
                "Setting {} does not exist or is not supported by this command",
                setting_key
            ),
            code: 0,
        }
        .into())
    }

    pub(crate) fn handle_single_value(
        &mut self,
        key: &str,
        callbacks: &(ValidatorFn, NormalizerFn),
        values: &Vec<String>,
        method: &str,
    ) -> anyhow::Result<()> {
        let (validator, normalizer) = callbacks;
        if 1 != count(values) {
            return Err(RuntimeException {
                message: "You can only pass one value. Example: php composer.phar config process-timeout 300".to_string(),
                code: 0,
            }
            .into());
        }

        let validation = validator(&PhpMixed::String(values[0].clone()));
        if validation.as_bool() != Some(true) {
            let suffix = if !validation.is_null() && validation.as_bool() != Some(false) {
                format!(" ({})", validation.as_string().unwrap_or(""))
            } else {
                String::new()
            };
            return Err(RuntimeException {
                message: sprintf(
                    &format!("\"%s\" is an invalid value{}", suffix),
                    &[values[0].clone().into()],
                ),
                code: 0,
            }
            .into());
        }

        let normalized_value = normalizer(&PhpMixed::String(values[0].clone()));

        if key == "disable-tls" {
            if !normalized_value.as_bool().unwrap_or(false)
                && self
                    .inner
                    .config
                    .as_ref()
                    .unwrap()
                    .get("disable-tls")
                    .as_bool()
                    .unwrap_or(false)
            {
                self.inner.inner.get_io().write_error(
                    "<info>You are now running Composer with SSL/TLS protection enabled.</info>",
                );
            } else if normalized_value.as_bool().unwrap_or(false)
                && !self
                    .inner
                    .config
                    .as_ref()
                    .unwrap()
                    .get("disable-tls")
                    .as_bool()
                    .unwrap_or(false)
            {
                self.inner.inner.get_io().write_error("<warning>You are now running Composer with SSL/TLS protection disabled.</warning>");
            }
        }

        call_user_func(
            self.inner.config_source.as_mut().unwrap(),
            method,
            vec![PhpMixed::String(key.to_string()), normalized_value],
        );
        Ok(())
    }

    pub(crate) fn handle_multi_value(
        &mut self,
        key: &str,
        callbacks: &(ValidatorFn, NormalizerFn),
        values: &Vec<String>,
        method: &str,
    ) -> anyhow::Result<()> {
        let (validator, normalizer) = callbacks;
        let values_mixed = PhpMixed::List(
            values
                .iter()
                .map(|s| Box::new(PhpMixed::String(s.clone())))
                .collect(),
        );
        let validation = validator(&values_mixed);
        if validation.as_bool() != Some(true) {
            let suffix = if !validation.is_null() && validation.as_bool() != Some(false) {
                format!(" ({})", validation.as_string().unwrap_or(""))
            } else {
                String::new()
            };
            return Err(RuntimeException {
                message: sprintf(
                    &format!("%s is an invalid value{}", suffix),
                    &[json_encode(&values_mixed, 0).into()],
                ),
                code: 0,
            }
            .into());
        }

        call_user_func(
            self.inner.config_source.as_mut().unwrap(),
            method,
            vec![PhpMixed::String(key.to_string()), normalizer(&values_mixed)],
        );
        Ok(())
    }

    /// Display the contents of the file in a pretty formatted way
    pub(crate) fn list_configuration(
        &self,
        contents: PhpMixed,
        raw_contents: PhpMixed,
        output: &dyn OutputInterface,
        k: Option<String>,
        show_source: bool,
    ) {
        let orig_k = k.clone();
        let io = self.inner.inner.get_io();
        let contents_arr = contents.as_array().cloned().unwrap_or_default();
        let raw_contents_arr = raw_contents.as_array().cloned().unwrap_or_default();
        let mut k = k;
        for (key, value) in &contents_arr {
            if k.is_none()
                && !in_array(
                    key.as_str(),
                    &vec!["config".to_string(), "repositories".to_string()],
                    true,
                )
            {
                continue;
            }

            let raw_val = raw_contents_arr
                .get(key)
                .map(|v| (**v).clone())
                .unwrap_or(PhpMixed::Null);

            let value_inner = (**value).clone();

            if is_array(&value_inner)
                && (!is_numeric(&key_first_key(&value_inner).unwrap_or_default())
                    || (key == "repositories" && k.is_none()))
            {
                let mut new_k = k.clone().unwrap_or_default();
                new_k.push_str(&Preg::replace("{^config\\.}", "", &format!("{}.", key)));
                k = Some(new_k);
                self.list_configuration(value_inner, raw_val, output, k.clone(), show_source);
                k = orig_k.clone();

                continue;
            }

            let value_display: String = if is_array(&value_inner) {
                let arr_strs: Vec<String> = value_inner
                    .as_list()
                    .map(|l| {
                        l.iter()
                            .map(|val| {
                                if is_array(val) {
                                    json_encode(val, 0)
                                } else {
                                    val.as_string().unwrap_or("").to_string()
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                format!("[{}]", implode(", ", &arr_strs))
            } else if is_bool(&value_inner) {
                var_export(&value_inner, true)
            } else {
                value_inner.as_string().unwrap_or("").to_string()
            };

            let source = if show_source {
                format!(
                    " ({})",
                    self.inner
                        .config
                        .as_ref()
                        .unwrap()
                        .get_source_of_value(&format!("{}{}", k.clone().unwrap_or_default(), key))
                )
            } else {
                String::new()
            };

            let link: String;
            if k.is_some() && strpos(k.as_ref().unwrap(), "repositories") == Some(0) {
                link = "https://getcomposer.org/doc/05-repositories.md".to_string();
            } else {
                let id_source = if k.as_deref() == Some("") || k.is_none() {
                    key.clone()
                } else {
                    k.clone().unwrap()
                };
                let id = Preg::replace("{\\..*$}", "", &id_source);
                let id = Preg::replace(
                    "{[^a-z0-9]}i",
                    "-",
                    &strtolower(&shirabe_php_shim::trim(&id, " \t\n\r\0\u{0B}")),
                );
                let id = Preg::replace("{-+}", "-", &id);
                link = format!("https://getcomposer.org/doc/06-config.md#{}", id);
            }
            if is_string(&raw_val)
                && raw_val
                    .as_string()
                    .map(|s| s.to_string())
                    .unwrap_or_default()
                    != value_display
            {
                io.write(
                    &format!(
                        "[<fg=yellow;href={}>{}{}</>] <info>{} ({})</info>{}",
                        link,
                        k.clone().unwrap_or_default(),
                        key,
                        raw_val.as_string().unwrap_or(""),
                        value_display,
                        source
                    ),
                    true,
                    io_interface::QUIET,
                );
            } else {
                io.write(
                    &format!(
                        "[<fg=yellow;href={}>{}{}</>] <info>{}</info>{}",
                        link,
                        k.clone().unwrap_or_default(),
                        key,
                        value_display,
                        source
                    ),
                    true,
                    io_interface::QUIET,
                );
            }
        }
    }

    /// Suggest setting-keys, while taking given options in account.
    fn suggest_setting_keys(&self) -> Box<dyn Fn(&CompletionInput) -> Vec<String>> {
        Box::new(|input: &CompletionInput| -> Vec<String> {
            if input.get_option("list").as_bool() == Some(true)
                || input.get_option("editor").as_bool() == Some(true)
                || input.get_option("auth").as_bool() == Some(true)
            {
                return vec![];
            }

            // initialize configuration
            let mut config = match Factory::create_config(None) {
                Ok(c) => c,
                Err(_) => return vec![],
            };

            // load configuration
            // TODO: BaseConfigCommand::get_composer_config_file is an instance method; using a free helper here.
            let config_file =
                JsonFile::new(get_composer_config_file_static(input, &config), None, None);
            if config_file.exists() {
                config.merge(
                    config_file.read().unwrap_or(PhpMixed::Null),
                    config_file.get_path(),
                );
            }

            // load auth-configuration
            let auth_config_file =
                JsonFile::new(get_auth_config_file_static(input, &config), None, None);
            if auth_config_file.exists() {
                let mut wrap: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
                wrap.insert(
                    "config".to_string(),
                    Box::new(auth_config_file.read().unwrap_or(PhpMixed::Null)),
                );
                config.merge(PhpMixed::Array(wrap), auth_config_file.get_path());
            }

            // collect all configuration setting-keys
            let raw_config = config.raw();
            let raw_arr = raw_config.as_array().cloned().unwrap_or_default();
            let mut keys: Vec<String> = array_merge(
                flatten_setting_keys(
                    raw_arr
                        .get("config")
                        .map(|v| (**v).clone())
                        .unwrap_or(PhpMixed::Null),
                    "",
                ),
                flatten_setting_keys(
                    raw_arr
                        .get("repositories")
                        .map(|v| (**v).clone())
                        .unwrap_or(PhpMixed::Null),
                    "repositories.",
                ),
            );

            // if unsetting …
            if input.get_option("unset").as_bool() == Some(true) {
                // … keep only the currently customized setting-keys …
                let sources = vec![
                    config_file.get_path().to_string(),
                    auth_config_file.get_path().to_string(),
                ];
                keys = array_filter(keys, |k: &String| -> bool {
                    in_array(config.get_source_of_value(k).as_str(), &sources, true)
                });
            } else {
                // … add all configurable package-properties, no matter if it exist
                let configurable: Vec<String> = ConfigCommand::CONFIGURABLE_PACKAGE_PROPERTIES
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                keys = array_merge(keys, configurable);

                // it would be nice to distinguish between showing and setting
                // a value, but that makes the implementation much more complex
                // and partially impossible because symfony's implementation
                // does not complete arguments followed by other arguments
            }

            // add all existing configurable package-properties
            if config_file.exists() {
                let configurable: Vec<String> = ConfigCommand::CONFIGURABLE_PACKAGE_PROPERTIES
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                let properties = array_filter_use_key(
                    config_file
                        .read()
                        .unwrap_or(PhpMixed::Null)
                        .as_array()
                        .cloned()
                        .unwrap_or_default(),
                    |key: &String| -> bool { in_array(key.as_str(), &configurable, true) },
                );

                keys = array_merge(keys, flatten_setting_keys(PhpMixed::Array(properties), ""));
            }

            // filter settings-keys by completion value
            let completion_value = input.get_completion_value();

            if completion_value != "" {
                keys = array_filter(keys, |key: &String| -> bool {
                    str_starts_with(key, &completion_value)
                });
            }

            sort(&mut keys);

            array_unique(keys)
        })
    }
}

// PHP signature: function ($val): bool / ($val) -> bool/string
pub type ValidatorFn = Box<dyn Fn(&PhpMixed) -> PhpMixed>;
pub type NormalizerFn = Box<dyn Fn(&PhpMixed) -> PhpMixed>;

fn boolean_validator(val: &PhpMixed) -> PhpMixed {
    PhpMixed::Bool(in_array(
        val.as_string().unwrap_or(""),
        &vec![
            "true".to_string(),
            "false".to_string(),
            "1".to_string(),
            "0".to_string(),
        ],
        true,
    ))
}

fn boolean_normalizer(val: &PhpMixed) -> PhpMixed {
    let s = val.as_string().unwrap_or("");
    PhpMixed::Bool(s != "false" && s != "" && s != "0")
}

fn build_unique_config_values() -> IndexMap<String, (ValidatorFn, NormalizerFn)> {
    let mut m: IndexMap<String, (ValidatorFn, NormalizerFn)> = IndexMap::new();

    let identity: NormalizerFn = Box::new(|val: &PhpMixed| -> PhpMixed { val.clone() });

    m.insert(
        "process-timeout".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_numeric(val.as_string().unwrap_or("")))),
            Box::new(|val| PhpMixed::Int(shirabe_php_shim::intval(val.as_string().unwrap_or("0")))),
        ),
    );
    m.insert(
        "use-include-path".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "use-github-api".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "preferred-install".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array(
                    val.as_string().unwrap_or(""),
                    &vec!["auto".to_string(), "source".to_string(), "dist".to_string()],
                    true,
                ))
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "gitlab-protocol".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array(
                    val.as_string().unwrap_or(""),
                    &vec!["git".to_string(), "http".to_string(), "https".to_string()],
                    true,
                ))
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "store-auths".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array(
                    val.as_string().unwrap_or(""),
                    &vec![
                        "true".to_string(),
                        "false".to_string(),
                        "prompt".to_string(),
                    ],
                    true,
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or("");
                if s == "prompt" {
                    PhpMixed::String("prompt".to_string())
                } else {
                    PhpMixed::Bool(s != "false" && s != "" && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "notify-on-install".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "vendor-dir".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "bin-dir".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "archive-dir".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "archive-format".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "data-dir".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-dir".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-files-dir".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-repo-dir".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-vcs-dir".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-ttl".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_numeric(val.as_string().unwrap_or("")))),
            Box::new(|val| PhpMixed::Int(shirabe_php_shim::intval(val.as_string().unwrap_or("0")))),
        ),
    );
    m.insert(
        "cache-files-ttl".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_numeric(val.as_string().unwrap_or("")))),
            Box::new(|val| PhpMixed::Int(shirabe_php_shim::intval(val.as_string().unwrap_or("0")))),
        ),
    );
    m.insert(
        "cache-files-maxsize".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(
                    Preg::is_match(
                        "/^\\s*([0-9.]+)\\s*(?:([kmg])(?:i?b)?)?\\s*$/i",
                        val.as_string().unwrap_or(""),
                        None,
                    )
                    .unwrap_or(false),
                )
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "bin-compat".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array(
                    val.as_string().unwrap_or(""),
                    &vec![
                        "auto".to_string(),
                        "full".to_string(),
                        "proxy".to_string(),
                        "symlink".to_string(),
                    ],
                    false,
                ))
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "discard-changes".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array(
                    val.as_string().unwrap_or(""),
                    &vec![
                        "stash".to_string(),
                        "true".to_string(),
                        "false".to_string(),
                        "1".to_string(),
                        "0".to_string(),
                    ],
                    true,
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or("");
                if s == "stash" {
                    PhpMixed::String("stash".to_string())
                } else {
                    PhpMixed::Bool(s != "false" && s != "" && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "autoloader-suffix".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| {
                if val.as_string() == Some("null") {
                    PhpMixed::Null
                } else {
                    val.clone()
                }
            }),
        ),
    );
    m.insert(
        "sort-packages".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "optimize-autoloader".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "classmap-authoritative".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "apcu-autoloader".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "prepend-autoloader".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "update-with-minimal-changes".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "disable-tls".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "secure-http".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "bump-after-update".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array(
                    val.as_string().unwrap_or(""),
                    &vec![
                        "dev".to_string(),
                        "no-dev".to_string(),
                        "true".to_string(),
                        "false".to_string(),
                        "1".to_string(),
                        "0".to_string(),
                    ],
                    true,
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or("");
                if s == "dev" || s == "no-dev" {
                    val.clone()
                } else {
                    PhpMixed::Bool(s != "false" && s != "" && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "cafile".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(
                    file_exists(val.as_string().unwrap_or(""))
                        && Filesystem::is_readable(val.as_string().unwrap_or("")),
                )
            }),
            Box::new(|val| {
                if val.as_string() == Some("null") {
                    PhpMixed::Null
                } else {
                    val.clone()
                }
            }),
        ),
    );
    m.insert(
        "capath".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(
                    is_dir(val.as_string().unwrap_or(""))
                        && Filesystem::is_readable(val.as_string().unwrap_or("")),
                )
            }),
            Box::new(|val| {
                if val.as_string() == Some("null") {
                    PhpMixed::Null
                } else {
                    val.clone()
                }
            }),
        ),
    );
    m.insert(
        "github-expose-hostname".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "htaccess-protect".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "lock".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "allow-plugins".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "platform-check".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array(
                    val.as_string().unwrap_or(""),
                    &vec![
                        "php-only".to_string(),
                        "true".to_string(),
                        "false".to_string(),
                        "1".to_string(),
                        "0".to_string(),
                    ],
                    true,
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or("");
                if s == "php-only" {
                    PhpMixed::String("php-only".to_string())
                } else {
                    PhpMixed::Bool(s != "false" && s != "" && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "use-parent-dir".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array(
                    val.as_string().unwrap_or(""),
                    &vec![
                        "true".to_string(),
                        "false".to_string(),
                        "prompt".to_string(),
                    ],
                    true,
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or("");
                if s == "prompt" {
                    PhpMixed::String("prompt".to_string())
                } else {
                    PhpMixed::Bool(s != "false" && s != "" && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "audit.abandoned".to_string(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array(
                    val.as_string().unwrap_or(""),
                    &vec![
                        Auditor::ABANDONED_IGNORE.to_string(),
                        Auditor::ABANDONED_REPORT.to_string(),
                        Auditor::ABANDONED_FAIL.to_string(),
                    ],
                    true,
                ))
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "audit.ignore-unreachable".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "audit.block-insecure".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "audit.block-abandoned".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );

    let _ = identity;
    m
}

fn build_multi_config_values() -> IndexMap<String, (ValidatorFn, NormalizerFn)> {
    let mut m: IndexMap<String, (ValidatorFn, NormalizerFn)> = IndexMap::new();
    m.insert(
        "github-protocols".to_string(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::String("array expected".to_string());
                }
                if let Some(list) = vals.as_list() {
                    for val in list {
                        if !in_array(
                            val.as_string().unwrap_or(""),
                            &vec!["git".to_string(), "https".to_string(), "ssh".to_string()],
                            false,
                        ) {
                            return PhpMixed::String(
                                "valid protocols include: git, https, ssh".to_string(),
                            );
                        }
                    }
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m.insert(
        "github-domains".to_string(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::String("array expected".to_string());
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m.insert(
        "gitlab-domains".to_string(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::String("array expected".to_string());
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m.insert(
        "audit.ignore-severity".to_string(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::String("array expected".to_string());
                }
                if let Some(list) = vals.as_list() {
                    for val in list {
                        if !in_array(
                            val.as_string().unwrap_or(""),
                            &vec![
                                "low".to_string(),
                                "medium".to_string(),
                                "high".to_string(),
                                "critical".to_string(),
                            ],
                            true,
                        ) {
                            return PhpMixed::String(
                                "valid severities include: low, medium, high, critical".to_string(),
                            );
                        }
                    }
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m
}

fn build_unique_props() -> IndexMap<String, (ValidatorFn, NormalizerFn)> {
    let mut m: IndexMap<String, (ValidatorFn, NormalizerFn)> = IndexMap::new();
    m.insert(
        "name".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "type".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "description".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "homepage".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "version".to_string(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "minimum-stability".to_string(),
        (
            Box::new(|val| {
                let normalized = VersionParser::normalize_stability(val.as_string().unwrap_or(""));
                PhpMixed::Bool(base_package::STABILITIES.contains_key(normalized.as_str()))
            }),
            Box::new(|val| {
                PhpMixed::String(VersionParser::normalize_stability(
                    val.as_string().unwrap_or(""),
                ))
            }),
        ),
    );
    m.insert(
        "prefer-stable".to_string(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m
}

fn build_multi_props() -> IndexMap<String, (ValidatorFn, NormalizerFn)> {
    let mut m: IndexMap<String, (ValidatorFn, NormalizerFn)> = IndexMap::new();
    m.insert(
        "keywords".to_string(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::String("array expected".to_string());
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m.insert(
        "license".to_string(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::String("array expected".to_string());
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m
}

/// build a flat list of dot-separated setting-keys from given config
fn flatten_setting_keys(config: PhpMixed, prefix: &str) -> Vec<String> {
    let mut keys: Vec<Vec<String>> = vec![];
    let arr = match config.as_array() {
        Some(a) => a.clone(),
        None => return vec![],
    };
    for (key, value) in &arr {
        keys.push(vec![format!("{}{}", prefix, key)]);
        // array-lists must not be added to completion
        // sub-keys of repository-keys must not be added to completion
        if is_array(value) && !array_is_list(value) && prefix != "repositories." {
            keys.push(flatten_setting_keys(
                (**value).clone(),
                &format!("{}{}.", prefix, key),
            ));
        }
    }

    let mut merged: Vec<String> = vec![];
    for k in keys {
        merged = array_merge(merged, k);
    }
    merged
}

// Helpers for the suggester since BaseConfigCommand methods need an instance.
fn get_composer_config_file_static(input: &CompletionInput, config: &Config) -> String {
    if input.get_option("global").as_bool() == Some(true) {
        format!(
            "{}/config.json",
            config.get("home").as_string().unwrap_or("")
        )
    } else {
        input
            .get_option("file")
            .as_string()
            .map(|s| s.to_string())
            .unwrap_or_else(|| Factory::get_composer_file())
    }
}

fn get_auth_config_file_static(input: &CompletionInput, config: &Config) -> String {
    if input.get_option("global").as_bool() == Some(true) {
        format!("{}/auth.json", config.get("home").as_string().unwrap_or(""))
    } else {
        let composer_config = get_composer_config_file_static(input, config);
        let parent = std::path::Path::new(&composer_config)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        format!("{}/auth.json", parent)
    }
}

// PHP key($value) — first key of an array
fn key_first_key(value: &PhpMixed) -> Option<String> {
    if let Some(arr) = value.as_array() {
        return arr.keys().next().cloned();
    }
    if let Some(list) = value.as_list() {
        if !list.is_empty() {
            return Some("0".to_string());
        }
    }
    None
}

impl BaseCommand for ConfigCommand {
    fn inner(&self) -> &Command {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Command {
        &mut self.inner
    }

    fn composer(&self) -> Option<&Composer> {
        self.composer.as_ref()
    }

    fn composer_mut(&mut self) -> Option<&mut Composer> {
        self.composer.as_mut()
    }

    fn io(&self) -> Option<&dyn IOInterface> {
        self.io.as_ref()
    }

    fn io_mut(&mut self) -> Option<&mut dyn IOInterface> {
        self.io.as_mut()
    }
}

impl BaseCommand for ConfigCommand {
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

impl BaseConfigCommand for ConfigCommand {
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
