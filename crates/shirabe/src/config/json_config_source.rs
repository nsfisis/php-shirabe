//! ref: composer/src/Composer/Config/JsonConfigSource.php

use crate::util::Silencer;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PHP_EOL, PhpMixed, RuntimeException, array_unshift, chmod, explode, file_get_contents,
    file_put_contents, implode, is_writable, sprintf,
};

use crate::config::ConfigSourceInterface;
use crate::json::JsonFile;
use crate::json::JsonManipulator;
use crate::json::JsonValidationException;
use crate::util::Filesystem;

/// JSON Configuration Source
#[derive(Debug)]
pub struct JsonConfigSource {
    /// @var JsonFile
    file: std::rc::Rc<std::cell::RefCell<JsonFile>>,

    /// @var bool
    auth_config: bool,
}

impl JsonConfigSource {
    /// Constructor
    pub fn new(file: std::rc::Rc<std::cell::RefCell<JsonFile>>, auth_config: bool) -> Self {
        Self { file, auth_config }
    }

    /// @param mixed ...$args
    fn manipulate_json(
        &mut self,
        clean: impl FnOnce(&mut JsonManipulator) -> Result<bool>,
        // TODO(phase-b): callback signature uses &mut $config (PHP reference) and variadic args
        fallback: impl FnOnce(&mut PhpMixed, &mut Vec<PhpMixed>),
        mut args: Vec<PhpMixed>,
    ) -> Result<()> {
        let contents;
        if self.file.borrow().exists() {
            if !is_writable(self.file.borrow().get_path()) {
                return Err(RuntimeException {
                    message: sprintf(
                        "The file \"%s\" is not writable.",
                        &[PhpMixed::String(self.file.borrow().get_path().to_string())],
                    ),
                    code: 0,
                }
                .into());
            }

            if !Filesystem::is_readable(self.file.borrow().get_path()) {
                return Err(RuntimeException {
                    message: sprintf(
                        "The file \"%s\" is not readable.",
                        &[PhpMixed::String(self.file.borrow().get_path().to_string())],
                    ),
                    code: 0,
                }
                .into());
            }

            contents = file_get_contents(self.file.borrow().get_path()).unwrap_or_default();
        } else if self.auth_config {
            contents = "{\n}\n".to_string();
        } else {
            contents = "{\n    \"config\": {\n    }\n}\n".to_string();
        }

        let mut manipulator = JsonManipulator::new(contents.clone())?;

        let new_file = !self.file.borrow().exists();

        // try to update cleanly
        let manipulator_result: bool = clean(&mut manipulator)?;
        if manipulator_result {
            file_put_contents(
                self.file.borrow().get_path(),
                manipulator.get_contents().as_bytes(),
            );
        } else {
            // on failed clean update, call the fallback and rewrite the whole file
            let mut config = self.file.borrow_mut().read()?;
            self.array_unshift_ref(&mut args, &mut config);
            fallback(&mut config, &mut args);
            // avoid ending up with arrays for keys that should be objects
            for prop in [
                "require",
                "require-dev",
                "conflict",
                "provide",
                "replace",
                "suggest",
                "config",
                "autoload",
                "autoload-dev",
                "scripts",
                "scripts-descriptions",
                "scripts-aliases",
                "support",
            ] {
                if let PhpMixed::Array(map) = &mut config {
                    if let Some(boxed) = map.get(prop) {
                        if let PhpMixed::Array(inner) = boxed.as_ref() {
                            if inner.is_empty() {
                                // PHP: $config[$prop] = new \stdClass;
                                map.insert(
                                    prop.to_string(),
                                    Box::new(PhpMixed::Array(IndexMap::new())),
                                );
                            }
                        }
                    }
                }
            }
            for prop in ["psr-0", "psr-4"] {
                if let PhpMixed::Array(map) = &mut config {
                    if let Some(autoload) = map.get_mut("autoload") {
                        if let PhpMixed::Array(autoload_map) = autoload.as_mut() {
                            if let Some(inner) = autoload_map.get(prop) {
                                if let PhpMixed::Array(inner_map) = inner.as_ref() {
                                    if inner_map.is_empty() {
                                        autoload_map.insert(
                                            prop.to_string(),
                                            Box::new(PhpMixed::Array(IndexMap::new())),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    if let Some(autoload_dev) = map.get_mut("autoload-dev") {
                        if let PhpMixed::Array(autoload_dev_map) = autoload_dev.as_mut() {
                            if let Some(inner) = autoload_dev_map.get(prop) {
                                if let PhpMixed::Array(inner_map) = inner.as_ref() {
                                    if inner_map.is_empty() {
                                        autoload_dev_map.insert(
                                            prop.to_string(),
                                            Box::new(PhpMixed::Array(IndexMap::new())),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for prop in [
                "platform",
                "http-basic",
                "bearer",
                "gitlab-token",
                "gitlab-oauth",
                "github-oauth",
                "custom-headers",
                "forgejo-token",
                "preferred-install",
            ] {
                if let PhpMixed::Array(map) = &mut config {
                    if let Some(cfg) = map.get_mut("config") {
                        if let PhpMixed::Array(cfg_map) = cfg.as_mut() {
                            if let Some(inner) = cfg_map.get(prop) {
                                if let PhpMixed::Array(inner_map) = inner.as_ref() {
                                    if inner_map.is_empty() {
                                        cfg_map.insert(
                                            prop.to_string(),
                                            Box::new(PhpMixed::Array(IndexMap::new())),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            self.file.borrow().write(config)?;
        }

        match self
            .file
            .borrow()
            .validate_schema(JsonFile::LAX_SCHEMA, None)
        {
            Ok(_) => {}
            Err(e) => {
                let Some(jve) = e.downcast_ref::<JsonValidationException>() else {
                    return Err(e);
                };
                // restore contents to the original state
                file_put_contents(self.file.borrow().get_path(), contents.as_bytes());
                return Err(RuntimeException {
                    message: format!(
                        "Failed to update composer.json with a valid format, reverting to the original content. Please report an issue to us with details (command you run and a copy of your composer.json). {}{}",
                        PHP_EOL,
                        implode(PHP_EOL, jve.get_errors()),
                    ),
                    code: 0,
                }
                .into());
            }
        }

        if new_file {
            let path = self.file.borrow().get_path().to_string();
            let _ = Silencer::call(|| {
                chmod(&path, 0o600);
                Ok(())
            });
        }

        Ok(())
    }

    /// Prepend a reference to an element to the beginning of an array.
    ///
    /// @param  mixed[] $array
    /// @param  mixed $value
    fn array_unshift_ref(&self, array: &mut Vec<PhpMixed>, value: &mut PhpMixed) -> i64 {
        let return_val = array_unshift(array, PhpMixed::String(String::new()));
        // PHP: $array[0] = &$value; (PHP reference)
        // TODO(phase-b): retain reference semantics so later mutations of $value propagate
        array[0] = value.clone();

        let _ = return_val;
        array.len() as i64
    }
}

impl ConfigSourceInterface for JsonConfigSource {
    fn get_name(&self) -> String {
        self.file.borrow().get_path().to_string()
    }

    fn add_repository(&mut self, name: &str, config: PhpMixed, append: bool) -> Result<()> {
        let name_owned = name.to_string();
        let clean_name = name_owned.clone();
        let clean_config = config.clone();
        self.manipulate_json(
            move |m| m.add_repository(&clean_name, clean_config, append),
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                // TODO(phase-b): port the closure body — args are [$cfg, $repo, $repoConfig, $append]
                let _ = (cfg, args);
                todo!("addRepository fallback closure body");
            },
            vec![PhpMixed::String(name_owned), config, PhpMixed::Bool(append)],
        )
    }

    fn insert_repository(
        &mut self,
        name: &str,
        config: PhpMixed,
        reference_name: &str,
        offset: i64,
    ) -> Result<()> {
        let name_owned = name.to_string();
        let reference_name_owned = reference_name.to_string();
        let clean_name = name_owned.clone();
        let clean_reference_name = reference_name_owned.clone();
        let clean_config = config.clone();
        self.manipulate_json(
            move |m| m.insert_repository(&clean_name, clean_config, &clean_reference_name, offset),
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                // TODO(phase-b): port the closure body
                let _ = (cfg, args);
                todo!("insertRepository fallback closure body");
            },
            vec![
                PhpMixed::String(name_owned),
                config,
                PhpMixed::String(reference_name_owned),
                PhpMixed::Int(offset),
            ],
        )
    }

    fn set_repository_url(&mut self, name: &str, url: &str) -> Result<()> {
        let clean_name = name.to_string();
        let clean_url = url.to_string();
        self.manipulate_json(
            move |m| m.set_repository_url(&clean_name, &clean_url),
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                // PHP: foreach ($config['repositories'] ?? [] as $index => $repository) { ... }
                let _ = (cfg, args);
                todo!("setRepositoryUrl fallback closure body");
            },
            vec![
                PhpMixed::String(name.to_string()),
                PhpMixed::String(url.to_string()),
            ],
        )
    }

    fn remove_repository(&mut self, name: &str) -> Result<()> {
        let clean_name = name.to_string();
        self.manipulate_json(
            move |m| m.remove_repository(&clean_name),
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                let _ = (cfg, args);
                todo!("removeRepository fallback closure body");
            },
            vec![PhpMixed::String(name.to_string())],
        )
    }

    fn add_config_setting(&mut self, name: &str, value: PhpMixed) -> Result<()> {
        let auth_config = self.auth_config;
        let clean_name = name.to_string();
        let clean_value = value.clone();
        self.manipulate_json(
            // override manipulator method for auth config files
            move |m| {
                if auth_config {
                    let parts = explode(".", &clean_name);
                    m.add_sub_node(
                        parts.get(0).map(String::as_str).unwrap_or(""),
                        parts.get(1).map(String::as_str).unwrap_or(""),
                        clean_value,
                        false,
                    )
                } else {
                    m.add_config_setting(&clean_name, clean_value)
                }
            },
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                // PHP: [$key, $host] = explode('.', $key, 2);
                let _ = (cfg, args, auth_config);
                todo!("addConfigSetting fallback closure body");
            },
            vec![PhpMixed::String(name.to_string()), value],
        )
    }

    fn remove_config_setting(&mut self, name: &str) -> Result<()> {
        let auth_config = self.auth_config;
        let clean_name = name.to_string();
        self.manipulate_json(
            // override manipulator method for auth config files
            move |m| {
                if auth_config {
                    let parts = explode(".", &clean_name);
                    m.remove_sub_node(
                        parts.get(0).map(String::as_str).unwrap_or(""),
                        parts.get(1).map(String::as_str).unwrap_or(""),
                    )
                } else {
                    m.remove_config_setting(&clean_name)
                }
            },
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                let _ = (cfg, args, auth_config);
                todo!("removeConfigSetting fallback closure body");
            },
            vec![PhpMixed::String(name.to_string())],
        )
    }

    fn add_property(&mut self, name: &str, value: PhpMixed) -> Result<()> {
        let clean_name = name.to_string();
        let clean_value = value.clone();
        self.manipulate_json(
            move |m| m.add_property(&clean_name, clean_value),
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                let _ = (cfg, args);
                todo!("addProperty fallback closure body");
            },
            vec![PhpMixed::String(name.to_string()), value],
        )
    }

    fn remove_property(&mut self, name: &str) -> Result<()> {
        let clean_name = name.to_string();
        self.manipulate_json(
            move |m| m.remove_property(&clean_name),
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                let _ = (cfg, args);
                todo!("removeProperty fallback closure body");
            },
            vec![PhpMixed::String(name.to_string())],
        )
    }

    fn add_link(&mut self, r#type: &str, name: &str, value: &str) -> Result<()> {
        let clean_type = r#type.to_string();
        let clean_name = name.to_string();
        let clean_value = value.to_string();
        self.manipulate_json(
            move |m| m.add_link(&clean_type, &clean_name, &clean_value, false),
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                // PHP: $config[$type][$name] = $value;
                let _ = (cfg, args);
                todo!("addLink fallback closure body");
            },
            vec![
                PhpMixed::String(r#type.to_string()),
                PhpMixed::String(name.to_string()),
                PhpMixed::String(value.to_string()),
            ],
        )
    }

    fn remove_link(&mut self, r#type: &str, name: &str) -> Result<()> {
        let clean_type = r#type.to_string();
        let clean_name = name.to_string();
        self.manipulate_json(
            move |m| m.remove_sub_node(&clean_type, &clean_name),
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                // PHP: unset($config[$type][$name]);
                let _ = (cfg, args);
                todo!("removeLink fallback (unset subnode) closure body");
            },
            vec![
                PhpMixed::String(r#type.to_string()),
                PhpMixed::String(name.to_string()),
            ],
        )?;
        let clean_type = r#type.to_string();
        self.manipulate_json(
            move |m| m.remove_main_key_if_empty(&clean_type),
            move |cfg: &mut PhpMixed, args: &mut Vec<PhpMixed>| {
                // PHP: if (0 === count($config[$type])) { unset($config[$type]); }
                let _ = (cfg, args);
                todo!("removeLink fallback (unset main key if empty) closure body");
            },
            vec![PhpMixed::String(r#type.to_string())],
        )
    }
}
