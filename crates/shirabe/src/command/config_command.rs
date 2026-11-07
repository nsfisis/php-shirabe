//! ref: composer/src/Composer/Command/ConfigCommand.php sha1:81de5feaf3b7e975424e4cce2c773869b5283f7b

use crate::advisory::Auditor;
use crate::command::base_command::base_command_complete;
use crate::command::{BaseCommand, BaseCommandData, BaseConfigCommand};
use crate::config::{Config, ConfigSourceInterface, JsonConfigSource};
use crate::console::input::{InputArgument, InputOption, SuggestedValues};
use crate::delegate_base_command_trait_impls_to_inner;
use crate::factory::Factory;
use crate::io::{IOInterfaceWrite, io_interface};
use crate::json::{JsonEncodeOptions, JsonFile};
use crate::package::base_package::{self};
use crate::util::{Filesystem, Platform};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    InvalidArgumentException, PhpMixed, RuntimeException, array_is_list, array_key_first,
    array_merge, array_values_mixed, bconcat, chmod, escapeshellcmd, exec, explode, file_exists,
    impl_php_class, implode, in_array_loose, in_array_strict, intval, is_array, is_bool, is_dir,
    is_numeric, is_object, is_string, json_encode, path_from_bytes, path_to_bytes, php_regex,
    php_str, php_to_string, preg_is_match, preg_match, preg_replace, str_replace, strpos,
    strtolower, system, touch, trim_chars, var_export,
};
use shirabe_semver::VersionParser;
use shirabe_symfony_console::input::InputInterfaceByName;
use shirabe_symfony_console::{command::Command, input::InputInterface, output::OutputInterface};

#[derive(Debug)]
pub struct ConfigCommand {
    base_command_data: BaseCommandData,

    config: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<Config>>>>,
    config_file: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<JsonFile>>>>,
    config_source: std::cell::RefCell<Option<JsonConfigSource>>,

    auth_config_file: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<JsonFile>>>>,
    auth_config_source: std::cell::RefCell<Option<JsonConfigSource>>,
}

impl_php_class!(ConfigCommand, r"Composer\Command\ConfigCommand");

impl ConfigCommand {
    /// List of additional configurable package-properties
    const CONFIGURABLE_PACKAGE_PROPERTIES: &'static [&'static BStr] = &[
        php_str(b"name"),
        php_str(b"type"),
        php_str(b"description"),
        php_str(b"homepage"),
        php_str(b"version"),
        php_str(b"minimum-stability"),
        php_str(b"prefer-stable"),
        php_str(b"keywords"),
        php_str(b"license"),
        php_str(b"repositories"),
        php_str(b"suggest"),
        php_str(b"extra"),
    ];

    pub fn new() -> Self {
        let command = ConfigCommand {
            base_command_data: BaseCommandData::new(None),
            config: std::cell::RefCell::new(None),
            config_file: std::cell::RefCell::new(None),
            config_source: std::cell::RefCell::new(None),
            auth_config_file: std::cell::RefCell::new(None),
            auth_config_source: std::cell::RefCell::new(None),
        };
        command
            .configure()
            .expect("ConfigCommand::configure uses static, valid metadata");
        command
    }

    fn handle_single_value(
        &self,
        key: &BStr,
        callbacks: &(ValidatorFn, NormalizerFn),
        values: &[BString],
        method: &BStr,
    ) -> anyhow::Result<()> {
        let (validator, normalizer) = callbacks;
        if 1 != values.len() {
            return Err(RuntimeException::new(BString::from(
                "You can only pass one value. Example: shirabe config process-timeout 300",
            ))
            .into());
        }

        let validation = validator(&PhpMixed::string(values[0].clone()));
        if validation.as_bool() != Some(true) {
            let suffix = if !validation.is_null() && validation.as_bool() != Some(false) {
                bconcat!(" (", validation.as_string().unwrap_or_default(), ")")
            } else {
                BString::default()
            };
            return Err(RuntimeException::new(bconcat!(
                "\"",
                values[0],
                "\" is an invalid value",
                suffix
            ))
            .into());
        }

        let normalized_value = normalizer(&PhpMixed::string(values[0].clone()));

        if key == "disable-tls" {
            let config = self.config.borrow().as_ref().unwrap().clone();
            if !normalized_value.as_bool().unwrap_or(false)
                && config.borrow().get_bool("disable-tls")
            {
                self.get_io().write_error(
                    "<info>You are now running Shirabe with SSL/TLS protection enabled.</info>",
                );
            } else if normalized_value.as_bool().unwrap_or(false)
                && !config.borrow().get_bool("disable-tls")
            {
                self.get_io().write_error("<warning>You are now running Shirabe with SSL/TLS protection disabled.</warning>");
            }
        }

        let mut config_source = self.config_source.borrow_mut();
        let config_source = config_source.as_mut().unwrap();
        match method.as_bytes() {
            b"addConfigSetting" => {
                config_source.add_config_setting(bstr::BStr::new(key), normalized_value)?
            }
            b"addProperty" => config_source.add_property(bstr::BStr::new(key), normalized_value)?,
            _ => unreachable!(),
        }
        Ok(())
    }

    fn handle_multi_value(
        &self,
        key: &BStr,
        callbacks: &(ValidatorFn, NormalizerFn),
        values: &[BString],
        method: &BStr,
    ) -> anyhow::Result<()> {
        let (validator, normalizer) = callbacks;
        let values_mixed =
            PhpMixed::List(values.iter().map(|s| PhpMixed::string(s.clone())).collect());
        let validation = validator(&values_mixed);
        if validation.as_bool() != Some(true) {
            let suffix = if !validation.is_null() && validation.as_bool() != Some(false) {
                bconcat!(" (", validation.as_string().unwrap_or_default(), ")")
            } else {
                BString::default()
            };
            return Err(RuntimeException::new(bconcat!(
                json_encode(&values_mixed).ok().unwrap_or_default(),
                " is an invalid value",
                suffix
            ))
            .into());
        }

        let mut config_source = self.config_source.borrow_mut();
        let config_source = config_source.as_mut().unwrap();
        match method.as_bytes() {
            b"addConfigSetting" => {
                config_source.add_config_setting(bstr::BStr::new(key), normalizer(&values_mixed))?
            }
            b"addProperty" => {
                config_source.add_property(bstr::BStr::new(key), normalizer(&values_mixed))?
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    /// Display the contents of the file in a pretty formatted way
    fn list_configuration(
        &self,
        contents: PhpMixed,
        raw_contents: PhpMixed,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        k: Option<BString>,
        show_source: bool,
    ) -> anyhow::Result<()> {
        let orig_k = k.clone();
        let contents_arr = contents.as_array().cloned().unwrap_or_default();
        let raw_contents_arr = raw_contents.as_array().cloned().unwrap_or_default();
        let mut k = k;
        for (key, value) in &contents_arr {
            if k.is_none() && !matches!(key.as_slice(), b"config" | b"repositories") {
                continue;
            }

            let raw_val = raw_contents_arr
                .get(B(key))
                .cloned()
                .unwrap_or(PhpMixed::Null);

            let mut value_inner = value.clone();

            if is_array(&value_inner)
                && (!is_numeric(&array_key_first(&value_inner).unwrap_or(PhpMixed::Null))
                    || (key == "repositories" && k.is_none()))
            {
                let mut new_k = k.clone().unwrap_or_default();
                new_k.extend_from_slice(&preg_replace(
                    php_regex!("{^config\\.}"),
                    "",
                    bconcat!(key, "."),
                ));
                k = Some(new_k);
                self.list_configuration(
                    value_inner,
                    raw_val,
                    output.clone(),
                    k.clone(),
                    show_source,
                )?;
                k = orig_k.clone();

                continue;
            }

            if is_array(&value_inner) {
                let arr_strs: Vec<BString> = array_values_mixed(&value_inner)
                    .iter()
                    .map(|val| {
                        if is_array(val) {
                            json_encode(val).unwrap_or_default()
                        } else {
                            php_to_string(val)
                        }
                    })
                    .collect();
                value_inner = PhpMixed::string(bconcat!("[", implode(", ", &arr_strs), "]"));
            }

            if is_bool(&value_inner) {
                value_inner = PhpMixed::string(var_export(&value_inner, true));
            }

            let source = if show_source {
                bconcat!(
                    " (",
                    self.config
                        .borrow()
                        .as_ref()
                        .unwrap()
                        .borrow()
                        .get_source_of_value(bconcat!(k.clone().unwrap_or_default(), key))?,
                    ")"
                )
            } else {
                BString::default()
            };

            let link: BString =
                if k.is_some() && strpos(k.as_ref().unwrap(), "repositories") == Some(0) {
                    "https://getcomposer.org/doc/05-repositories.md".into()
                } else {
                    let id_source = if k.as_ref().is_none_or(|k| k.is_empty()) {
                        key.clone()
                    } else {
                        k.clone().unwrap()
                    };
                    let id = preg_replace(php_regex!("{\\..*$}"), "", &id_source);
                    let id = preg_replace(
                        php_regex!("{[^a-z0-9]}i"),
                        "-",
                        strtolower(trim_chars(&id, " \t\n\r\0\u{0B}")),
                    );
                    let id = preg_replace(php_regex!("{-+}"), "-", &id);
                    bconcat!("https://getcomposer.org/doc/06-config.md#", id)
                };
            if is_string(&raw_val) && raw_val.as_string() != value_inner.as_string() {
                self.get_io().write_with_verbosity(
                    bconcat!(
                        "[<fg=yellow;href=",
                        link,
                        ">",
                        k.clone().unwrap_or_default(),
                        key,
                        "</>] <info>",
                        raw_val.as_string().unwrap_or_default(),
                        " (",
                        php_to_string(&value_inner),
                        ")</info>",
                        source
                    ),
                    io_interface::QUIET,
                );
            } else {
                self.get_io().write_with_verbosity(
                    bconcat!(
                        "[<fg=yellow;href=",
                        link,
                        ">",
                        k.clone().unwrap_or_default(),
                        key,
                        "</>] <info>",
                        php_to_string(&value_inner),
                        "</info>",
                        source
                    ),
                    io_interface::QUIET,
                );
            }
        }
        Ok(())
    }

    /// Suggest setting-keys, while taking given options in account.
    fn suggest_setting_keys(&self) -> SuggestedValues {
        SuggestedValues::Closure(Box::new(|this, input, _suggestions| {
            if input.get_option("list")?.to_bool()
                || input.get_option("editor")?.to_bool()
                || input.get_option("auth")?.to_bool()
            {
                return Ok(vec![]);
            }

            let this = this
                .as_any()
                .downcast_ref::<ConfigCommand>()
                .expect("suggestSettingKeys is bound to ConfigCommand");
            // PHP passes the CompletionInput itself; the accessors only read from it, so a
            // clone behind a fresh handle is equivalent.
            let input_handle: std::rc::Rc<
                std::cell::RefCell<dyn shirabe_symfony_console::input::InputInterface>,
            > = std::rc::Rc::new(std::cell::RefCell::new(input.clone()));

            // initialize configuration
            let mut config = Factory::create_config(None, None)?;

            // load configuration
            let config_file = JsonFile::new(
                this.get_composer_config_file(input_handle.clone(), &config)?,
                None,
                None,
            )?;
            if config_file.exists() {
                let path = config_file.get_path().to_path_buf();
                let data = config_file.read()?.as_array().cloned().unwrap_or_default();
                config.merge(&data, path_to_bytes(&path));
            }

            // load auth-configuration
            let auth_config_file = JsonFile::new(
                this.get_auth_config_file(input_handle.clone(), &config)?,
                None,
                None,
            )?;
            if auth_config_file.exists() {
                let path = auth_config_file.get_path().to_path_buf();
                let mut data = IndexMap::new();
                data.insert("config".into(), auth_config_file.read()?);
                config.merge(&data, path_to_bytes(&path));
            }

            // collect all configuration setting-keys
            let raw_config = config.raw();
            let mut keys = flatten_setting_keys(
                raw_config
                    .get(B("config"))
                    .cloned()
                    .unwrap_or(PhpMixed::Null),
                "",
            );
            keys.extend(flatten_setting_keys(
                raw_config
                    .get(B("repositories"))
                    .cloned()
                    .unwrap_or(PhpMixed::Null),
                "repositories.",
            ));

            // if unsetting …
            if input.get_option("unset")?.to_bool() {
                // … keep only the currently customized setting-keys …
                let sources = [
                    BString::from(path_to_bytes(config_file.get_path())),
                    BString::from(path_to_bytes(auth_config_file.get_path())),
                ];
                let mut customized_keys = Vec::new();
                for key in keys {
                    if sources.contains(&config.get_source_of_value(&key)?) {
                        customized_keys.push(key);
                    }
                }
                keys = customized_keys;

            // … else if showing or setting a value …
            } else {
                // … add all configurable package-properties, no matter if it exist
                keys.extend(
                    Self::CONFIGURABLE_PACKAGE_PROPERTIES
                        .iter()
                        .map(|property| BString::from(*property)),
                );

                // it would be nice to distinguish between showing and setting
                // a value, but that makes the implementation much more complex
                // and partially impossible because symfony's implementation
                // does not complete arguments followed by other arguments
            }

            // add all existing configurable package-properties
            if config_file.exists() {
                let properties: IndexMap<BString, PhpMixed> = config_file
                    .read()?
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(key, _)| {
                        Self::CONFIGURABLE_PACKAGE_PROPERTIES
                            .iter()
                            .any(|property| key == property)
                    })
                    .collect();

                keys.extend(flatten_setting_keys(PhpMixed::Array(properties), ""));
            }

            // filter settings-keys by completion value
            let completion_value = input.get_completion_value();

            if !completion_value.is_empty() {
                keys.retain(|key| key.as_bytes().starts_with(&completion_value));
            }

            keys.sort();

            keys.dedup();
            Ok(keys)
        }))
    }
}

impl Default for ConfigCommand {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait(?Send)]
impl Command for ConfigCommand {
    fn configure(&self) -> anyhow::Result<()> {
        self.set_name("config".into())?;
        self.set_description("Sets config options".into());
        self.set_definition(&[
                InputOption::new("global", Some("g".into()), Some(InputOption::VALUE_NONE), "Apply command to the global config file", None).unwrap().into(),
        InputOption::new("editor", Some("e".into()), Some(InputOption::VALUE_NONE), "Open editor", None).unwrap().into(),
        InputOption::new("auth", Some("a".into()), Some(InputOption::VALUE_NONE), "Affect auth config file (only used for --editor)", None).unwrap().into(),
        InputOption::new("unset", None, Some(InputOption::VALUE_NONE), "Unset the given setting-key", None).unwrap().into(),
        InputOption::new("list", Some("l".into()), Some(InputOption::VALUE_NONE), "List configuration settings", None).unwrap().into(),
        InputOption::new("file", Some("f".into()), Some(InputOption::VALUE_REQUIRED), "If you want to choose a different composer.json or config.json", None).unwrap().into(),
        InputOption::new("absolute", None, Some(InputOption::VALUE_NONE), "Returns absolute paths when fetching *-dir config values instead of relative", None).unwrap().into(),
        InputOption::new("json", Some("j".into()), Some(InputOption::VALUE_NONE), "JSON decode the setting value, to be used with extra.* keys", None).unwrap().into(),
        InputOption::new("merge", Some("m".into()), Some(InputOption::VALUE_NONE), "Merge the setting value with the current value, to be used with extra.* or audit.ignore[-abandoned] keys in combination with --json", None).unwrap().into(),
        InputOption::new("append", None, Some(InputOption::VALUE_NONE), "When adding a repository, append it (lowest priority) to the existing ones instead of prepending it (highest priority)", None).unwrap().into(),
        InputOption::new("source", None, Some(InputOption::VALUE_NONE), "Display where the config value is loaded from", None).unwrap().into(),
        InputArgument::with_suggested_values("setting-key", None, "Setting key", None, self.suggest_setting_keys()).unwrap().into(),
        InputArgument::new("setting-value", Some(InputArgument::IS_ARRAY), "Setting value", None).unwrap().into(),
            ]);
        self.set_help(
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
                 Read more at https://getcomposer.org/doc/03-cli.md#config".into(),
            );
        Ok(())
    }

    fn initialize(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<()> {
        <Self as BaseConfigCommand>::initialize(self, input.clone(), output)?;

        let config = self.config.borrow().as_ref().unwrap().clone();
        let auth_config_file = self.get_auth_config_file(input.clone(), &config.borrow())?;

        let auth_config_file_jf = std::rc::Rc::new(std::cell::RefCell::new(JsonFile::new(
            auth_config_file,
            None,
            Some(self.get_io().clone()),
        )?));
        *self.auth_config_file.borrow_mut() = Some(auth_config_file_jf.clone());
        *self.auth_config_source.borrow_mut() =
            Some(JsonConfigSource::new(auth_config_file_jf, true));

        // Initialize the global file if it's not there, ignoring any warnings or notices
        let auth_config_file = self.auth_config_file.borrow().as_ref().unwrap().clone();
        if input.borrow().get_option("global")?.as_bool() == Some(true)
            && !auth_config_file.borrow().exists()
        {
            touch(auth_config_file.borrow().get_path());
            let mut empty_objs: IndexMap<BString, PhpMixed> = IndexMap::new();
            for k in &[
                "bitbucket-oauth",
                "github-oauth",
                "gitlab-oauth",
                "gitlab-token",
                "http-basic",
                "bearer",
                "forgejo-token",
            ] {
                empty_objs.insert(bconcat!(k), PhpMixed::Object(IndexMap::new()));
            }
            auth_config_file
                .borrow()
                .write(PhpMixed::Array(empty_objs))?;
            let path_clone = auth_config_file.borrow().get_path().to_path_buf();
            chmod(&path_clone, 0o600);
        }
        Ok(())
    }

    async fn execute(
        &self,
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<i64> {
        // Open file in editor
        if input.borrow().get_option("editor")?.as_bool() == Some(true) {
            let mut editor = Platform::get_env("EDITOR");
            if editor.as_ref().is_none_or(|editor| editor.is_empty()) {
                if Platform::is_windows() {
                    editor = Some("notepad".into());
                } else {
                    for candidate in &["editor", "vim", "vi", "nano", "pico", "ed"] {
                        if !exec(bconcat!("which ", candidate), None, None)
                            .unwrap_or_default()
                            .is_empty()
                        {
                            editor = Some((*candidate).into());
                            break;
                        }
                    }
                }
            } else {
                editor = Some(escapeshellcmd(editor.unwrap()));
            }

            let file = if input.borrow().get_option("auth")?.as_bool() == Some(true) {
                self.auth_config_file
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .get_path()
                    .to_path_buf()
            } else {
                self.config_file
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .get_path()
                    .to_path_buf()
            };
            system(
                bconcat!(
                    editor.unwrap_or_default(),
                    " ",
                    file,
                    if Platform::is_windows() {
                        ""
                    } else {
                        " > `tty`"
                    }
                ),
                None,
            );

            return Ok(0);
        }

        let config = self.config.borrow().as_ref().unwrap().clone();
        let config_file = self.config_file.borrow().as_ref().unwrap().clone();
        let auth_config_file = self.auth_config_file.borrow().as_ref().unwrap().clone();
        if input.borrow().get_option("global")?.as_bool() != Some(true) {
            let config_read = config_file.borrow_mut().read()?;
            let config_map = match config_read {
                PhpMixed::Array(m) => m,
                _ => IndexMap::new(),
            };
            let config_file_path = config_file.borrow().get_path().to_path_buf();
            config
                .borrow_mut()
                .merge(&config_map, path_to_bytes(&config_file_path));
            let auth_data: PhpMixed = if auth_config_file.borrow().exists() {
                auth_config_file.borrow_mut().read()?
            } else {
                PhpMixed::Array(IndexMap::new())
            };
            let mut wrap: IndexMap<BString, PhpMixed> = IndexMap::new();
            wrap.insert("config".into(), auth_data);
            let auth_config_file_path = auth_config_file.borrow().get_path().to_path_buf();
            config
                .borrow_mut()
                .merge(&wrap, path_to_bytes(&auth_config_file_path));
        }

        {
            let config_rc = config.clone();
            self.get_io()
                .borrow_mut()
                .load_configuration(&mut config_rc.borrow_mut())?;
        }

        // List the configuration of the file settings
        if input.borrow().get_option("list")?.as_bool() == Some(true) {
            let all_map = config.borrow().all(0)?;
            let raw_map = config.borrow().raw();
            let to_mixed = |m: IndexMap<BString, PhpMixed>| -> PhpMixed {
                PhpMixed::Array(m.into_iter().collect())
            };
            self.list_configuration(
                to_mixed(all_map),
                to_mixed(raw_map),
                output,
                None,
                input.borrow().get_option("source")?.as_bool() == Some(true),
            )?;

            return Ok(0);
        }

        let setting_key_arg = input.borrow().get_argument("setting-key")?;
        let setting_key = match setting_key_arg.as_string() {
            Some(s) => BString::from(s),
            None => return Ok(0),
        };

        // If the user enters in a config variable, parse it and save to file
        let setting_values_raw = input.borrow().get_argument("setting-value")?;
        let setting_values: Vec<bstr::BString> = setting_values_raw
            .as_array()
            .map(<[bstr::BString]>::to_vec)
            .unwrap_or_default();
        if !setting_values.is_empty() && input.borrow().get_option("unset")?.as_bool() == Some(true)
        {
            return Err(RuntimeException::new(BString::from(
                "You can not combine a setting value with --unset",
            ))
            .into());
        }

        // show the value if no value is provided
        if setting_values.is_empty() && input.borrow().get_option("unset")?.as_bool() != Some(true)
        {
            let properties: Vec<&'static BStr> = Self::CONFIGURABLE_PACKAGE_PROPERTIES.to_vec();
            let mut properties_defaults: IndexMap<BString, PhpMixed> = IndexMap::new();
            properties_defaults.insert("type".into(), PhpMixed::string(BString::from("library")));
            properties_defaults.insert("description".into(), PhpMixed::string(BString::default()));
            properties_defaults.insert("homepage".into(), PhpMixed::string(BString::default()));
            properties_defaults.insert(
                "minimum-stability".into(),
                PhpMixed::string(BString::from("stable")),
            );
            properties_defaults.insert("prefer-stable".into(), PhpMixed::Bool(false));
            properties_defaults.insert("keywords".into(), PhpMixed::List(vec![]));
            properties_defaults.insert("license".into(), PhpMixed::List(vec![]));
            properties_defaults.insert("suggest".into(), PhpMixed::List(vec![]));
            properties_defaults.insert("extra".into(), PhpMixed::List(vec![]));
            let raw_data = config_file.borrow_mut().read()?;
            let data = config.borrow_mut().all(0)?;
            let mut source = config.borrow().get_source_of_value(&setting_key)?;

            let mut value: PhpMixed;
            if let Some(matches) = preg_match(
                php_regex!("/^repos?(?:itories)?(?:\\.(.+))?/"),
                &setting_key,
            ) {
                if matches.get(1).is_none() {
                    value = data
                        .get(B("repositories"))
                        .cloned()
                        .unwrap_or_else(|| PhpMixed::Array(IndexMap::new()));
                } else {
                    let repo_key = matches.get(1).unwrap_or_default();
                    let repos = data.get(B("repositories")).cloned();
                    value = match repos
                        .as_ref()
                        .and_then(|r| r.as_array().and_then(|a| a.get(B(repo_key))))
                    {
                        Some(v) => v.clone(),
                        None => {
                            return Err(InvalidArgumentException::new(bconcat!(
                                "There is no ",
                                repo_key,
                                " repository defined"
                            ))
                            .into());
                        }
                    };
                }
            } else if strpos(&setting_key, ".").is_some() {
                let bits = explode(".", &setting_key);
                // PHP: $data here is the mixed dot-segment cursor; the rest of the loop walks it.
                let mut cursor: PhpMixed = if bits[0] == "extra" || bits[0] == "suggest" {
                    PhpMixed::Array(raw_data.as_array().cloned().unwrap_or_else(IndexMap::new))
                } else {
                    data.get(B("config")).cloned().unwrap_or(PhpMixed::Null)
                };
                let mut r#match = false;
                let mut key_acc: Option<BString> = None;
                for bit in &bits {
                    let new_key = match &key_acc {
                        Some(k) => bconcat!(k, ".", bit),
                        None => bit.clone(),
                    };
                    key_acc = Some(new_key.clone());
                    r#match = false;
                    if let Some(arr) = cursor.as_array()
                        && let Some(v) = arr.get(B(&new_key))
                    {
                        r#match = true;
                        cursor = v.clone();
                        key_acc = None;
                    }
                }

                if !r#match {
                    return Err(
                        RuntimeException::new(bconcat!(setting_key, " is not defined.")).into(),
                    );
                }

                value = cursor;
            } else if data
                .get(B("config"))
                .and_then(|c| c.as_array())
                .map(|c| c.contains_key(B(&setting_key)))
                .unwrap_or(false)
            {
                value = config.borrow_mut().get_with_flags(
                    &setting_key,
                    if input.borrow().get_option("absolute")?.as_bool() == Some(true) {
                        0
                    } else {
                        Config::RELATIVE_PATHS
                    },
                )?;
                // ensure we get {} output for properties which are objects
                if value.as_array().map(|a| a.is_empty()).unwrap_or(false) {
                    let schema = JsonFile::parse_json(
                        Some(JsonFile::COMPOSER_SCHEMA_JSON),
                        Some(bstr::BStr::new("composer.schema.json")),
                    )?;
                    let type_value = schema
                        .as_array()
                        .and_then(|a| a.get(B("properties")))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get(B("config")))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get(B("properties")))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get(B(&setting_key)))
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.get(B("type")))
                        .cloned();
                    if let Some(tv) = type_value {
                        let type_array = match &tv {
                            PhpMixed::List(_) | PhpMixed::Array(_) => tv,
                            other => PhpMixed::List(vec![other.clone()]),
                        };
                        if type_array.as_list().is_some_and(|l| {
                            l.iter().any(|v| v.as_string() == Some("object".into()))
                        }) {
                            value = PhpMixed::Object(IndexMap::new());
                        }
                    }
                }
            } else if raw_data
                .as_array()
                .and_then(|a| a.get(B(&setting_key)))
                .is_some()
                && in_array_strict(
                    setting_key.clone(),
                    &properties
                        .iter()
                        .map(|s| PhpMixed::string(*s))
                        .collect::<Vec<_>>(),
                )
            {
                value = raw_data
                    .as_array()
                    .unwrap()
                    .get(B(&setting_key))
                    .unwrap()
                    .clone();
                source = path_to_bytes(config_file.borrow().get_path()).into();
            } else if let Some(v) = properties_defaults.get(B(&setting_key)) {
                value = v.clone();
                source = "defaults".into();
            } else {
                return Err(RuntimeException::new(bconcat!(setting_key, " is not defined")).into());
            }

            let value_str = if is_array(&value) || is_object(&value) || is_bool(&value) {
                JsonFile::encode_with_options(
                    &value,
                    JsonEncodeOptions {
                        pretty_print: false,
                        ..Default::default()
                    },
                )?
            } else {
                php_to_string(&value)
            };

            let mut source_of_config_value = BString::default();
            if input.borrow().get_option("source")?.as_bool() == Some(true) {
                source_of_config_value = bconcat!(" (", source, ")");
            }

            self.get_io().write_with_verbosity(
                bconcat!(value_str, source_of_config_value),
                io_interface::QUIET,
            );

            return Ok(0);
        }

        let values: Vec<BString> = setting_values; // what the user is trying to add/change

        let boolean_validator = |val: &PhpMixed| -> bool {
            matches!(
                val.as_string().unwrap_or_default().as_bytes(),
                b"true" | b"false" | b"1" | b"0"
            )
        };
        let boolean_normalizer = |val: &PhpMixed| -> PhpMixed {
            let s = val.as_string().unwrap_or_default();
            PhpMixed::Bool(s != "false" && !s.is_empty() && s != "0")
        };

        // handle config values
        let unique_config_values = build_unique_config_values();
        let multi_config_values = build_multi_config_values();

        // allow unsetting audit config entirely
        if input.borrow().get_option("unset")?.as_bool() == Some(true) && setting_key == "audit" {
            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .remove_config_setting(bstr::BStr::new(&setting_key));

            return Ok(0);
        }

        if input.borrow().get_option("unset")?.as_bool() == Some(true)
            && (unique_config_values.contains_key(&setting_key)
                || multi_config_values.contains_key(&setting_key))
        {
            if setting_key == "disable-tls" && config.borrow().get_bool("disable-tls") {
                self.get_io().write_error(
                    "<info>You are now running Shirabe with SSL/TLS protection enabled.</info>",
                );
            }

            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .remove_config_setting(bstr::BStr::new(&setting_key));

            return Ok(0);
        }
        if let Some(callbacks) = unique_config_values.get(&setting_key) {
            self.handle_single_value(
                setting_key.as_bstr(),
                callbacks,
                &values,
                BStr::new("addConfigSetting"),
            )?;

            return Ok(0);
        }
        if let Some(callbacks) = multi_config_values.get(&setting_key) {
            self.handle_multi_value(
                setting_key.as_bstr(),
                callbacks,
                &values,
                BStr::new("addConfigSetting"),
            )?;

            return Ok(0);
        }
        // handle preferred-install per-package config
        if let Some(matches) = preg_match(php_regex!("/^preferred-install\\.(.+)/"), &setting_key) {
            if input.borrow().get_option("unset")?.as_bool() == Some(true) {
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&setting_key));

                return Ok(0);
            }

            let validator = &unique_config_values.get(B("preferred-install")).unwrap().0;
            if !validator(&PhpMixed::string(values[0].clone()))
                .as_bool()
                .unwrap_or(false)
            {
                return Err(RuntimeException::new(bconcat!(
                    "Invalid value for ",
                    setting_key,
                    ". Should be one of: auto, source, or dist"
                ))
                .into());
            }

            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .add_config_setting(
                    bstr::BStr::new(&setting_key),
                    PhpMixed::string(values[0].clone()),
                );

            return Ok(0);
        }

        // handle allow-plugins config setting elements true or false to add/remove
        if let Some(matches) = preg_match(
            php_regex!("{^allow-plugins\\.([a-zA-Z0-9/*-]+)}"),
            &setting_key,
        ) {
            if input.borrow().get_option("unset")?.as_bool() == Some(true) {
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&setting_key));

                return Ok(0);
            }

            if !boolean_validator(&PhpMixed::string(values[0].clone())) {
                return Err(RuntimeException::new(bconcat!(
                    "\"",
                    values[0],
                    "\" is an invalid value"
                ))
                .into());
            }

            let normalized_value = boolean_normalizer(&PhpMixed::string(values[0].clone()));

            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .add_config_setting(bstr::BStr::new(&setting_key), normalized_value);

            return Ok(0);
        }

        // handle properties
        let unique_props = build_unique_props();
        let multi_props = build_multi_props();

        if input.borrow().get_option("global")?.as_bool() == Some(true)
            && (unique_props.contains_key(&setting_key)
                || multi_props.contains_key(&setting_key)
                || strpos(&setting_key, "extra.") == Some(0))
        {
            return Err(InvalidArgumentException::new(bconcat!("The ", setting_key, " property can not be set in the global config.json file. Use `shirabe global config` to apply changes to the global composer.json"))
            .into());
        }
        if input.borrow().get_option("unset")?.as_bool() == Some(true)
            && (unique_props.contains_key(&setting_key) || multi_props.contains_key(&setting_key))
        {
            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .remove_property(bstr::BStr::new(&setting_key));

            return Ok(0);
        }
        if let Some(callbacks) = unique_props.get(&setting_key) {
            self.handle_single_value(
                setting_key.as_bstr(),
                callbacks,
                &values,
                BStr::new("addProperty"),
            )?;

            return Ok(0);
        }
        if let Some(callbacks) = multi_props.get(&setting_key) {
            self.handle_multi_value(
                setting_key.as_bstr(),
                callbacks,
                &values,
                BStr::new("addProperty"),
            )?;

            return Ok(0);
        }

        // handle repositories
        if let Some(matches) = preg_match(php_regex!("/^repos?(?:itories)?\\.(.+)/"), &setting_key)
        {
            if input.borrow().get_option("unset")?.as_bool() == Some(true) {
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_repository(bstr::BStr::new(matches.get(1).unwrap()));

                return Ok(0);
            }

            if 2 == values.len() {
                let mut repo: IndexMap<BString, PhpMixed> = IndexMap::new();
                repo.insert("type".into(), PhpMixed::string(values[0].clone()));
                repo.insert("url".into(), PhpMixed::string(values[1].clone()));
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .add_repository(
                        bstr::BStr::new(matches.get(1).unwrap()),
                        PhpMixed::Array(repo),
                        input.borrow().get_option("append")?.as_bool() == Some(true),
                    );

                return Ok(0);
            }

            if 1 == values.len() {
                let value = strtolower(&values[0]);
                if boolean_validator(&PhpMixed::string(value.clone())) {
                    if !boolean_normalizer(&PhpMixed::string(value))
                        .as_bool()
                        .unwrap_or(false)
                    {
                        self.config_source
                            .borrow_mut()
                            .as_mut()
                            .unwrap()
                            .add_repository(
                                bstr::BStr::new(matches.get(1).unwrap()),
                                PhpMixed::Bool(false),
                                input.borrow().get_option("append")?.as_bool() == Some(true),
                            );

                        return Ok(0);
                    }
                } else {
                    let value = JsonFile::parse_json(
                        Some(&values[0]),
                        Some(bstr::BStr::new("composer.json")),
                    )?;
                    self.config_source
                        .borrow_mut()
                        .as_mut()
                        .unwrap()
                        .add_repository(
                            bstr::BStr::new(matches.get(1).unwrap()),
                            value,
                            input.borrow().get_option("append")?.as_bool() == Some(true),
                        );

                    return Ok(0);
                }
            }

            return Err(RuntimeException::new(BString::from("You must pass the type and a url. Example: shirabe config repositories.foo vcs https://bar.com"))
        .into());
        }

        // handle extra
        if let Some(matches) = preg_match(php_regex!("/^extra\\.(.+)/"), &setting_key) {
            if input.borrow().get_option("unset")?.as_bool() == Some(true) {
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_property(bstr::BStr::new(&setting_key));

                return Ok(0);
            }

            let mut value = PhpMixed::string(values[0].clone());
            if input.borrow().get_option("json")?.as_bool() == Some(true) {
                value =
                    JsonFile::parse_json(Some(&values[0]), Some(bstr::BStr::new("composer.json")))?;
                if input.borrow().get_option("merge")?.as_bool() == Some(true) {
                    let current_value_outer = config_file.borrow_mut().read()?;
                    let bits = explode(".", &setting_key);
                    let mut current_value: PhpMixed = current_value_outer;
                    for bit in &bits {
                        current_value = current_value
                            .as_array()
                            .and_then(|a| a.get(B(bit)))
                            .cloned()
                            .unwrap_or(PhpMixed::Null);
                    }
                    if is_array(&current_value) && is_array(&value) {
                        if array_is_list(&current_value) && array_is_list(&value) {
                            value = array_merge(
                                PhpMixed::List(
                                    current_value.as_list().cloned().unwrap_or_default(),
                                ),
                                PhpMixed::List(value.as_list().cloned().unwrap_or_default()),
                            );
                        } else {
                            // PHP "+" operator on arrays: keep keys from left, fill from right.
                            // A list participates with its integer indices as keys.
                            let mut merged: IndexMap<BString, PhpMixed> = match &value {
                                PhpMixed::List(l) => l
                                    .iter()
                                    .enumerate()
                                    .map(|(i, v)| (bconcat!(i), v.clone()))
                                    .collect(),
                                _ => value.as_array().cloned().unwrap_or_default(),
                            };
                            let fill: IndexMap<BString, PhpMixed> = match &current_value {
                                PhpMixed::List(l) => l
                                    .iter()
                                    .enumerate()
                                    .map(|(i, v)| (bconcat!(i), v.clone()))
                                    .collect(),
                                _ => current_value.as_array().cloned().unwrap_or_default(),
                            };
                            for (k, v) in fill {
                                if !merged.contains_key(&k) {
                                    merged.insert(k, v);
                                }
                            }
                            value = PhpMixed::Array(merged);
                        }
                    }
                }
            }
            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .add_property(bstr::BStr::new(&setting_key), value);

            return Ok(0);
        }

        // handle suggest
        if let Some(matches) = preg_match(php_regex!("/^suggest\\.(.+)/"), &setting_key) {
            if input.borrow().get_option("unset")?.as_bool() == Some(true) {
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_property(bstr::BStr::new(&setting_key));

                return Ok(0);
            }

            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .add_property(
                    bstr::BStr::new(&setting_key),
                    PhpMixed::string(implode(" ", &values)),
                );

            return Ok(0);
        }

        // handle unsetting extra/suggest
        if matches!(setting_key.as_slice(), b"suggest" | b"extra")
            && input.borrow().get_option("unset")?.as_bool() == Some(true)
        {
            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .remove_property(bstr::BStr::new(&setting_key));

            return Ok(0);
        }

        // handle platform
        if let Some(matches) = preg_match(php_regex!("/^platform\\.(.+)/"), &setting_key) {
            if input.borrow().get_option("unset")?.as_bool() == Some(true) {
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&setting_key));

                return Ok(0);
            }

            let value = if values[0] == "false" {
                PhpMixed::Bool(false)
            } else {
                PhpMixed::string(values[0].clone())
            };
            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .add_config_setting(bstr::BStr::new(&setting_key), value);

            return Ok(0);
        }

        // handle unsetting platform
        if setting_key == "platform" && input.borrow().get_option("unset")?.as_bool() == Some(true)
        {
            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .remove_config_setting(bstr::BStr::new(&setting_key));

            return Ok(0);
        }

        // handle audit.ignore and audit.ignore-abandoned with --merge support
        if matches!(
            setting_key.as_slice(),
            b"audit.ignore" | b"audit.ignore-abandoned"
        ) {
            if input.borrow().get_option("unset")?.as_bool() == Some(true) {
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&setting_key));

                return Ok(0);
            }

            let mut value: PhpMixed =
                PhpMixed::List(values.iter().map(|s| PhpMixed::string(s.clone())).collect());
            if input.borrow().get_option("json")?.as_bool() == Some(true) {
                value =
                    JsonFile::parse_json(Some(&values[0]), Some(bstr::BStr::new("composer.json")))?;
                if !is_array(&value) {
                    return Err(RuntimeException::new(bconcat!(
                        "Expected an array or object for ",
                        setting_key
                    ))
                    .into());
                }
            }

            if input.borrow().get_option("merge")?.as_bool() == Some(true) {
                let current_config = config_file.borrow_mut().read()?;
                let key_suffix = str_replace("audit.", "", &setting_key);
                let current_value = current_config
                    .as_array()
                    .and_then(|a| a.get(B("config")))
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(B("audit")))
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(B(&key_suffix)))
                    .cloned()
                    .unwrap_or(PhpMixed::Null);

                if !current_value.is_null() && is_array(&current_value) && is_array(&value) {
                    if array_is_list(&current_value) && array_is_list(&value) {
                        // Both are lists, merge them
                        value = array_merge(
                            PhpMixed::List(current_value.as_list().cloned().unwrap_or_default()),
                            PhpMixed::List(value.as_list().cloned().unwrap_or_default()),
                        );
                    } else if !array_is_list(&current_value) && !array_is_list(&value) {
                        // Both are associative arrays (objects), merge them
                        let mut merged: IndexMap<BString, PhpMixed> =
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
                        return Err(RuntimeException::new(bconcat!(
                            "Cannot merge array and object for ",
                            setting_key
                        ))
                        .into());
                    }
                }
            }

            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .add_config_setting(bstr::BStr::new(&setting_key), value);

            return Ok(0);
        }

        // handle auth
        if let Some(matches) = preg_match(
            php_regex!(
                "/^(bitbucket-oauth|github-oauth|gitlab-oauth|gitlab-token|http-basic|custom-headers|bearer|forgejo-token)\\.(.+)/"
            ),
            &setting_key,
        ) {
            if input.borrow().get_option("unset")?.as_bool() == Some(true) {
                self.auth_config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&bconcat!(
                        matches.get(1).unwrap(),
                        ".",
                        matches.get(2).unwrap()
                    )));
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&bconcat!(
                        matches.get(1).unwrap(),
                        ".",
                        matches.get(2).unwrap()
                    )));

                return Ok(0);
            }

            let key = bconcat!(matches.get(1).unwrap(), ".", matches.get(2).unwrap());
            if matches.get(1).unwrap() == "bitbucket-oauth" {
                if 2 != values.len() {
                    return Err(RuntimeException::new(bconcat!(
                        "Expected two arguments (consumer-key, consumer-secret), got ",
                        values.len()
                    ))
                    .into());
                }
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&key));
                let mut obj: IndexMap<BString, PhpMixed> = IndexMap::new();
                obj.insert("consumer-key".into(), PhpMixed::string(values[0].clone()));
                obj.insert(
                    "consumer-secret".into(),
                    PhpMixed::string(values[1].clone()),
                );
                self.auth_config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .add_config_setting(bstr::BStr::new(&key), PhpMixed::Array(obj));
            } else if matches.get(1).unwrap() == "gitlab-token" && 2 == values.len() {
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&key));
                let mut obj: IndexMap<BString, PhpMixed> = IndexMap::new();
                obj.insert("username".into(), PhpMixed::string(values[0].clone()));
                obj.insert("token".into(), PhpMixed::string(values[1].clone()));
                self.auth_config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .add_config_setting(bstr::BStr::new(&key), PhpMixed::Array(obj));
            } else if matches!(
                matches.get(1).unwrap().as_bytes(),
                b"github-oauth" | b"gitlab-oauth" | b"gitlab-token" | b"bearer"
            ) {
                if 1 != values.len() {
                    return Err(RuntimeException::new(BString::from(
                        "Too many arguments, expected only one token",
                    ))
                    .into());
                }
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&key));
                self.auth_config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .add_config_setting(bstr::BStr::new(&key), PhpMixed::string(values[0].clone()));
            } else if matches.get(1).unwrap() == "http-basic" {
                if 2 != values.len() {
                    return Err(RuntimeException::new(bconcat!(
                        "Expected two arguments (username, password), got ",
                        values.len()
                    ))
                    .into());
                }
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&key));
                let mut obj: IndexMap<BString, PhpMixed> = IndexMap::new();
                obj.insert("username".into(), PhpMixed::string(values[0].clone()));
                obj.insert("password".into(), PhpMixed::string(values[1].clone()));
                self.auth_config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .add_config_setting(bstr::BStr::new(&key), PhpMixed::Array(obj));
            } else if matches.get(1).unwrap() == "custom-headers" {
                if values.is_empty() {
                    return Err(RuntimeException::new(BString::from(
                        "Expected at least one argument (header), got none",
                    ))
                    .into());
                }

                // Validate headers format
                let mut formatted_headers: Vec<PhpMixed> = vec![];
                for header in &values {
                    if !is_string(&PhpMixed::string(header.clone())) {
                        return Err(RuntimeException::new(BString::from(
                            "Headers must be strings in \"Header-Name: Header-Value\" format",
                        ))
                        .into());
                    }

                    // Check if the header is in correct "Name: Value" format
                    if !preg_is_match(php_regex!("/^[^:]+:\\s*.+$/"), header) {
                        return Err(RuntimeException::new(bconcat!(
                            "Header \"",
                            header,
                            "\" is not in \"Header-Name: Header-Value\" format"
                        ))
                        .into());
                    }

                    formatted_headers.push(PhpMixed::string(header.clone()));
                }

                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&key));
                self.auth_config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .add_config_setting(bstr::BStr::new(&key), PhpMixed::List(formatted_headers));
            } else if matches.get(1).unwrap() == "forgejo-token" {
                if 2 != values.len() {
                    return Err(RuntimeException::new(bconcat!(
                        "Expected two arguments (username, access token), got ",
                        values.len()
                    ))
                    .into());
                }
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_config_setting(bstr::BStr::new(&key));
                let mut obj: IndexMap<BString, PhpMixed> = IndexMap::new();
                obj.insert("username".into(), PhpMixed::string(values[0].clone()));
                obj.insert("token".into(), PhpMixed::string(values[1].clone()));
                self.auth_config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .add_config_setting(bstr::BStr::new(&key), PhpMixed::Array(obj));
            }

            return Ok(0);
        }

        // handle script
        if let Some(matches) = preg_match(php_regex!("/^scripts\\.(.+)/"), &setting_key) {
            if input.borrow().get_option("unset")?.as_bool() == Some(true) {
                self.config_source
                    .borrow_mut()
                    .as_mut()
                    .unwrap()
                    .remove_property(bstr::BStr::new(&setting_key));

                return Ok(0);
            }

            let value: PhpMixed = if values.len() > 1 {
                PhpMixed::List(values.iter().map(|s| PhpMixed::string(s.clone())).collect())
            } else {
                PhpMixed::string(values[0].clone())
            };
            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .add_property(bstr::BStr::new(&setting_key), value);

            return Ok(0);
        }

        // handle unsetting other top level properties
        if input.borrow().get_option("unset")?.as_bool() == Some(true) {
            self.config_source
                .borrow_mut()
                .as_mut()
                .unwrap()
                .remove_property(bstr::BStr::new(&setting_key));

            return Ok(0);
        }

        Err(InvalidArgumentException::new(bconcat!(
            "Setting ",
            setting_key,
            " does not exist or is not supported by this command"
        ))
        .into())
    }

    fn complete(
        &self,
        input: &shirabe_symfony_console::completion::CompletionInput,
        suggestions: &mut shirabe_symfony_console::completion::CompletionSuggestions,
    ) -> anyhow::Result<()> {
        base_command_complete(self, input, suggestions)
    }

    shirabe_symfony_console::delegate_command_trait_impls_to_inner!(base_command_data);
}

impl BaseCommand for ConfigCommand {
    fn base_command_data(&self) -> &BaseCommandData {
        &self.base_command_data
    }

    delegate_base_command_trait_impls_to_inner!(base_command_data);
}

// PHP signature: function ($val): bool / ($val) -> bool/string
pub type ValidatorFn = Box<dyn Fn(&PhpMixed) -> PhpMixed>;
pub type NormalizerFn = Box<dyn Fn(&PhpMixed) -> PhpMixed>;

fn boolean_validator(val: &PhpMixed) -> PhpMixed {
    PhpMixed::Bool(matches!(
        val.as_string().unwrap_or_default().as_bytes(),
        b"true" | b"false" | b"1" | b"0"
    ))
}

fn boolean_normalizer(val: &PhpMixed) -> PhpMixed {
    let s = val.as_string().unwrap_or_default();
    PhpMixed::Bool(s != "false" && !s.is_empty() && s != "0")
}

fn build_unique_config_values() -> IndexMap<BString, (ValidatorFn, NormalizerFn)> {
    let mut m: IndexMap<BString, (ValidatorFn, NormalizerFn)> = IndexMap::new();

    let identity: NormalizerFn = Box::new(|val: &PhpMixed| -> PhpMixed { val.clone() });

    m.insert(
        "process-timeout".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_numeric(&val.as_string().unwrap_or_default().into()))),
            Box::new(|val| PhpMixed::Int(intval(&val.as_string().unwrap_or("0".into()).into()))),
        ),
    );
    m.insert(
        "use-include-path".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "use-github-api".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "preferred-install".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(matches!(
                    val.as_string().unwrap_or_default().as_bytes(),
                    b"auto" | b"source" | b"dist"
                ))
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "gitlab-protocol".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(matches!(
                    val.as_string().unwrap_or_default().as_bytes(),
                    b"git" | b"http" | b"https"
                ))
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "store-auths".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(matches!(
                    val.as_string().unwrap_or_default().as_bytes(),
                    b"true" | b"false" | b"prompt"
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or_default();
                if s == "prompt" {
                    PhpMixed::string(BString::from("prompt"))
                } else {
                    PhpMixed::Bool(s != "false" && !s.is_empty() && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "notify-on-install".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "vendor-dir".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "bin-dir".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "archive-dir".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "archive-format".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "data-dir".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-dir".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-files-dir".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-repo-dir".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-vcs-dir".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "cache-ttl".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_numeric(&val.as_string().unwrap_or_default().into()))),
            Box::new(|val| PhpMixed::Int(intval(&val.as_string().unwrap_or("0".into()).into()))),
        ),
    );
    m.insert(
        "cache-files-ttl".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_numeric(&val.as_string().unwrap_or_default().into()))),
            Box::new(|val| PhpMixed::Int(intval(&val.as_string().unwrap_or("0".into()).into()))),
        ),
    );
    m.insert(
        "cache-files-maxsize".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(preg_is_match(
                    php_regex!("/^\\s*([0-9.]+)\\s*(?:([kmg])(?:i?b)?)?\\s*$/i"),
                    val.as_string().unwrap_or_default(),
                ))
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "bin-compat".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(in_array_loose(
                    val.as_string().unwrap_or_default(),
                    &[
                        PhpMixed::string(BString::from("auto")),
                        PhpMixed::string(BString::from("full")),
                        PhpMixed::string(BString::from("proxy")),
                        PhpMixed::string(BString::from("symlink")),
                    ],
                ))
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "discard-changes".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(matches!(
                    val.as_string().unwrap_or_default().as_bytes(),
                    b"stash" | b"true" | b"false" | b"1" | b"0"
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or_default();
                if s == "stash" {
                    PhpMixed::string(BString::from("stash"))
                } else {
                    PhpMixed::Bool(s != "false" && !s.is_empty() && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "autoloader-suffix".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| {
                if val.as_string() == Some("null".into()) {
                    PhpMixed::Null
                } else {
                    val.clone()
                }
            }),
        ),
    );
    m.insert(
        "sort-packages".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "optimize-autoloader".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "classmap-authoritative".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "apcu-autoloader".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "prepend-autoloader".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "update-with-minimal-changes".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "disable-tls".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "secure-http".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "bump-after-update".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(matches!(
                    val.as_string().unwrap_or_default().as_bytes(),
                    b"dev" | b"no-dev" | b"true" | b"false" | b"1" | b"0"
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or_default();
                if s == "dev" || s == "no-dev" {
                    val.clone()
                } else {
                    PhpMixed::Bool(s != "false" && !s.is_empty() && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "cafile".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(
                    file_exists(path_from_bytes(val.as_string().unwrap_or_default()))
                        && Filesystem::is_readable(path_from_bytes(
                            val.as_string().unwrap_or_default(),
                        )),
                )
            }),
            Box::new(|val| {
                if val.as_string() == Some("null".into()) {
                    PhpMixed::Null
                } else {
                    val.clone()
                }
            }),
        ),
    );
    m.insert(
        "capath".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(
                    is_dir(path_from_bytes(val.as_string().unwrap_or_default()))
                        && Filesystem::is_readable(path_from_bytes(
                            val.as_string().unwrap_or_default(),
                        )),
                )
            }),
            Box::new(|val| {
                if val.as_string() == Some("null".into()) {
                    PhpMixed::Null
                } else {
                    val.clone()
                }
            }),
        ),
    );
    m.insert(
        "github-expose-hostname".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "htaccess-protect".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "lock".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "allow-plugins".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "platform-check".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(matches!(
                    val.as_string().unwrap_or_default().as_bytes(),
                    b"php-only" | b"true" | b"false" | b"1" | b"0"
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or_default();
                if s == "php-only" {
                    PhpMixed::string(BString::from("php-only"))
                } else {
                    PhpMixed::Bool(s != "false" && !s.is_empty() && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "use-parent-dir".into(),
        (
            Box::new(|val| {
                PhpMixed::Bool(matches!(
                    val.as_string().unwrap_or_default().as_bytes(),
                    b"true" | b"false" | b"prompt"
                ))
            }),
            Box::new(|val| {
                let s = val.as_string().unwrap_or_default();
                if s == "prompt" {
                    PhpMixed::string(BString::from("prompt"))
                } else {
                    PhpMixed::Bool(s != "false" && !s.is_empty() && s != "0")
                }
            }),
        ),
    );
    m.insert(
        "audit.abandoned".into(),
        (
            Box::new(|val| {
                let val = val.as_string().unwrap_or_default();
                PhpMixed::Bool(
                    val == Auditor::ABANDONED_IGNORE
                        || val == Auditor::ABANDONED_REPORT
                        || val == Auditor::ABANDONED_FAIL,
                )
            }),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "audit.ignore-unreachable".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "audit.block-insecure".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m.insert(
        "audit.block-abandoned".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );

    let _ = identity;
    m
}

fn build_multi_config_values() -> IndexMap<BString, (ValidatorFn, NormalizerFn)> {
    let mut m: IndexMap<BString, (ValidatorFn, NormalizerFn)> = IndexMap::new();
    m.insert(
        "github-protocols".into(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::string(BString::from("array expected"));
                }
                if let Some(list) = vals.as_list() {
                    for val in list {
                        if !in_array_loose(
                            val.as_string().unwrap_or_default(),
                            &[
                                PhpMixed::string(BString::from("git")),
                                PhpMixed::string(BString::from("https")),
                                PhpMixed::string(BString::from("ssh")),
                            ],
                        ) {
                            return PhpMixed::string(BString::from(
                                "valid protocols include: git, https, ssh",
                            ));
                        }
                    }
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m.insert(
        "github-domains".into(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::string(BString::from("array expected"));
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m.insert(
        "gitlab-domains".into(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::string(BString::from("array expected"));
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m.insert(
        "audit.ignore-severity".into(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::string(BString::from("array expected"));
                }
                if let Some(list) = vals.as_list() {
                    for val in list {
                        if !matches!(
                            val.as_string().unwrap_or_default().as_bytes(),
                            b"low" | b"medium" | b"high" | b"critical"
                        ) {
                            return PhpMixed::string(BString::from(
                                "valid severities include: low, medium, high, critical",
                            ));
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

fn build_unique_props() -> IndexMap<BString, (ValidatorFn, NormalizerFn)> {
    let mut m: IndexMap<BString, (ValidatorFn, NormalizerFn)> = IndexMap::new();
    m.insert(
        "name".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "type".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "description".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "homepage".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "version".into(),
        (
            Box::new(|val| PhpMixed::Bool(is_string(val))),
            Box::new(|val| val.clone()),
        ),
    );
    m.insert(
        "minimum-stability".into(),
        (
            Box::new(|val| {
                let normalized =
                    VersionParser::normalize_stability(val.as_string().unwrap_or_default())
                        .unwrap_or_default();
                PhpMixed::Bool(base_package::stability_priority(&normalized).is_some())
            }),
            Box::new(|val| {
                PhpMixed::string(
                    VersionParser::normalize_stability(val.as_string().unwrap_or_default())
                        .unwrap_or_default(),
                )
            }),
        ),
    );
    m.insert(
        "prefer-stable".into(),
        (Box::new(boolean_validator), Box::new(boolean_normalizer)),
    );
    m
}

fn build_multi_props() -> IndexMap<BString, (ValidatorFn, NormalizerFn)> {
    let mut m: IndexMap<BString, (ValidatorFn, NormalizerFn)> = IndexMap::new();
    m.insert(
        "keywords".into(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::string(BString::from("array expected"));
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m.insert(
        "license".into(),
        (
            Box::new(|vals| {
                if !is_array(vals) {
                    return PhpMixed::string(BString::from("array expected"));
                }
                PhpMixed::Bool(true)
            }),
            Box::new(|vals| vals.clone()),
        ),
    );
    m
}

/// build a flat list of dot-separated setting-keys from given config
fn flatten_setting_keys(config: PhpMixed, prefix: impl AsRef<[u8]>) -> Vec<BString> {
    let prefix = prefix.as_ref();
    let mut keys: Vec<Vec<BString>> = vec![];
    let arr = match config.as_array() {
        Some(a) => a.clone(),
        None => return vec![],
    };
    for (key, value) in &arr {
        keys.push(vec![bconcat!(prefix, key)]);
        // array-lists must not be added to completion
        // sub-keys of repository-keys must not be added to completion
        if is_array(value) && !array_is_list(value) && prefix != b"repositories." {
            keys.push(flatten_setting_keys(
                value.clone(),
                &bconcat!(prefix, key, "."),
            ));
        }
    }

    let mut merged: Vec<BString> = vec![];
    for k in keys {
        merged.extend(k);
    }
    merged
}

impl BaseConfigCommand for ConfigCommand {
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
