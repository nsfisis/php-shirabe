//! ref: composer/src/Composer/Config/JsonConfigSource.php

use crate::util::Silencer;
use anyhow::Result;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PHP_EOL, PhpMixed, RuntimeException, chmod, explode, file_get_contents, file_put_contents,
    implode, is_writable, sprintf,
};

use crate::config::ConfigSourceInterface;
use crate::json::JsonFile;
use crate::json::JsonManipulator;
use crate::json::JsonValidationException;
use crate::util::Filesystem;

/// JSON Configuration Source
#[derive(Debug)]
pub struct JsonConfigSource {
    file: std::rc::Rc<std::cell::RefCell<JsonFile>>,
    auth_config: bool,
}

impl JsonConfigSource {
    pub fn new(file: std::rc::Rc<std::cell::RefCell<JsonFile>>, auth_config: bool) -> Self {
        Self { file, auth_config }
    }

    fn manipulate_json(
        &mut self,
        clean: impl FnOnce(&mut JsonManipulator) -> Result<bool>,
        fallback: impl FnOnce(&mut PhpMixed) -> Result<()>,
    ) -> Result<()> {
        let contents;
        if self.file.borrow().exists() {
            if !is_writable(self.file.borrow().get_path()) {
                return Err(RuntimeException {
                    message: format!(
                        "The file \"{}\" is not writable.",
                        PhpMixed::String(self.file.borrow().get_path().to_string()),
                    ),
                    code: 0,
                }
                .into());
            }

            if !Filesystem::is_readable(self.file.borrow().get_path()) {
                return Err(RuntimeException {
                    message: format!(
                        "The file \"{}\" is not readable.",
                        PhpMixed::String(self.file.borrow().get_path().to_string()),
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
            fallback(&mut config)?;
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
                if let PhpMixed::Array(map) = &mut config
                    && let Some(boxed) = map.get(prop)
                    && let PhpMixed::Array(inner) = boxed.as_ref()
                    && inner.is_empty()
                {
                    // PHP: $config[$prop] = new \stdClass;
                    map.insert(prop.to_string(), Box::new(PhpMixed::Array(IndexMap::new())));
                }
            }
            for prop in ["psr-0", "psr-4"] {
                if let PhpMixed::Array(map) = &mut config {
                    if let Some(autoload) = map.get_mut("autoload")
                        && let PhpMixed::Array(autoload_map) = autoload.as_mut()
                        && let Some(inner) = autoload_map.get(prop)
                        && let PhpMixed::Array(inner_map) = inner.as_ref()
                        && inner_map.is_empty()
                    {
                        autoload_map
                            .insert(prop.to_string(), Box::new(PhpMixed::Array(IndexMap::new())));
                    }
                    if let Some(autoload_dev) = map.get_mut("autoload-dev")
                        && let PhpMixed::Array(autoload_dev_map) = autoload_dev.as_mut()
                        && let Some(inner) = autoload_dev_map.get(prop)
                        && let PhpMixed::Array(inner_map) = inner.as_ref()
                        && inner_map.is_empty()
                    {
                        autoload_dev_map
                            .insert(prop.to_string(), Box::new(PhpMixed::Array(IndexMap::new())));
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
                if let PhpMixed::Array(map) = &mut config
                    && let Some(cfg) = map.get_mut("config")
                    && let PhpMixed::Array(cfg_map) = cfg.as_mut()
                    && let Some(inner) = cfg_map.get(prop)
                    && let PhpMixed::Array(inner_map) = inner.as_ref()
                    && inner_map.is_empty()
                {
                    cfg_map.insert(prop.to_string(), Box::new(PhpMixed::Array(IndexMap::new())));
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

    /// PHP:
    /// ```php
    /// if (!array_is_list($config['repositories'] ?? [])) { convert the keyed map to a list }
    /// ```
    fn normalize_repositories_to_list(root: &mut IndexMap<String, Box<PhpMixed>>) {
        let map = match root.get("repositories").map(|b| b.as_ref()) {
            Some(PhpMixed::Array(m)) if !m.is_empty() => m.clone(),
            _ => return,
        };
        let mut list: Vec<Box<PhpMixed>> = Vec::new();
        for (repository_index, repository) in map {
            match repository.as_ref() {
                PhpMixed::Array(repo_map) => {
                    let mut entry = repo_map.clone();
                    if !entry.contains_key("name") {
                        let mut with_name = IndexMap::new();
                        with_name.insert(
                            "name".to_string(),
                            Box::new(PhpMixed::String(repository_index.clone())),
                        );
                        for (k, v) in entry {
                            with_name.insert(k, v);
                        }
                        entry = with_name;
                    }
                    list.push(Box::new(PhpMixed::Array(entry)));
                }
                _ => {
                    let mut single = IndexMap::new();
                    single.insert(repository_index.clone(), repository.clone());
                    list.push(Box::new(PhpMixed::Array(single)));
                }
            }
        }
        root.insert("repositories".to_string(), Box::new(PhpMixed::List(list)));
    }

    /// PHP:
    /// ```php
    /// $config['repositories'] = array_values(
    ///     array_filter(
    ///         $config['repositories'] ?? [],
    ///         fn($val) => !isset($val['name']) || $val['name'] !== $name || $val !== [$name => false],
    ///     ),
    /// );
    /// ```
    fn dedupe_repositories_by_name(root: &mut IndexMap<String, Box<PhpMixed>>, name: &str) {
        let items: Vec<Box<PhpMixed>> = match root.get("repositories").map(|b| b.as_ref()) {
            Some(PhpMixed::List(items)) => items.clone(),
            Some(PhpMixed::Array(map)) => map.values().cloned().collect(),
            _ => return,
        };
        let filtered: Vec<Box<PhpMixed>> = items
            .into_iter()
            .filter(|val| {
                let name_set = val.as_ref().get("name").is_some();
                let name_eq = val.as_ref().get("name").and_then(|v| v.as_string()) == Some(name);
                let is_repo_false = matches!(val.as_ref(), PhpMixed::Array(m)
                    if m.len() == 1 && matches!(m.get(name).map(|b| b.as_ref()), Some(PhpMixed::Bool(false))));
                !name_set || !name_eq || !is_repo_false
            })
            .collect();
        root.insert(
            "repositories".to_string(),
            Box::new(PhpMixed::List(filtered)),
        );
    }

    /// Set a value at a nested key path, creating intermediate maps as needed.
    fn set_nested(root: &mut IndexMap<String, Box<PhpMixed>>, path: &[&str], value: PhpMixed) {
        let (last, heads) = path.split_last().unwrap();
        let mut cursor = root;
        for seg in heads {
            let entry = cursor
                .entry(seg.to_string())
                .or_insert_with(|| Box::new(PhpMixed::Array(IndexMap::new())));
            if !matches!(entry.as_ref(), PhpMixed::Array(_)) {
                **entry = PhpMixed::Array(IndexMap::new());
            }
            cursor = match entry.as_mut() {
                PhpMixed::Array(m) => m,
                _ => unreachable!(),
            };
        }
        cursor.insert(last.to_string(), Box::new(value));
    }

    /// Unset a value at a nested key path; a no-op if the path is absent.
    fn unset_nested(root: &mut IndexMap<String, Box<PhpMixed>>, path: &[&str]) {
        let (last, heads) = path.split_last().unwrap();
        let mut cursor = root;
        for seg in heads {
            cursor = match cursor.get_mut(*seg).map(|b| b.as_mut()) {
                Some(PhpMixed::Array(m)) => m,
                _ => return,
            };
        }
        cursor.shift_remove(*last);
    }

    fn is_auth_config_key(key: &str) -> bool {
        const PREFIXES: &[&str] = &[
            "bitbucket-oauth.",
            "github-oauth.",
            "gitlab-oauth.",
            "gitlab-token.",
            "bearer.",
            "http-basic.",
            "custom-headers.",
            "forgejo-token.",
            "platform.",
        ];
        PREFIXES.iter().any(|p| key.starts_with(p))
    }
}

impl ConfigSourceInterface for JsonConfigSource {
    fn get_name(&self) -> String {
        self.file.borrow().get_path().to_string()
    }

    fn add_repository(&mut self, name: &str, config: PhpMixed, append: bool) -> Result<()> {
        let config_cloned = config.clone();
        self.manipulate_json(
            move |m| m.add_repository(name, config_cloned, append),
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                Self::normalize_repositories_to_list(root);

                if matches!(config, PhpMixed::Bool(false)) {
                    if let Some(PhpMixed::List(list)) =
                        root.get_mut("repositories").map(|b| b.as_mut())
                    {
                        for repository in list.iter_mut() {
                            if repository.as_ref().get("name").and_then(|v| v.as_string())
                                == Some(name)
                            {
                                let mut replaced = IndexMap::new();
                                replaced.insert(name.to_string(), Box::new(PhpMixed::Bool(false)));
                                **repository = PhpMixed::Array(replaced);
                                return Ok(());
                            }
                            if let PhpMixed::Array(m) = repository.as_ref()
                                && m.len() == 1
                                && matches!(
                                    m.get(name).map(|b| b.as_ref()),
                                    Some(PhpMixed::Bool(false))
                                )
                            {
                                return Ok(());
                            }
                        }
                    } else {
                        root.insert(
                            "repositories".to_string(),
                            Box::new(PhpMixed::List(Vec::new())),
                        );
                    }
                    let mut entry = IndexMap::new();
                    entry.insert(name.to_string(), Box::new(PhpMixed::Bool(false)));
                    if let Some(PhpMixed::List(list)) =
                        root.get_mut("repositories").map(|b| b.as_mut())
                    {
                        list.push(Box::new(PhpMixed::Array(entry)));
                    }
                    return Ok(());
                }

                let mut repo_config = config;
                if let PhpMixed::Array(rc) = &repo_config
                    && !name.is_empty()
                    && !rc.contains_key("name")
                {
                    let mut with_name = IndexMap::new();
                    with_name.insert(
                        "name".to_string(),
                        Box::new(PhpMixed::String(name.to_string())),
                    );
                    for (k, v) in rc.clone() {
                        with_name.insert(k, v);
                    }
                    repo_config = PhpMixed::Array(with_name);
                }

                Self::dedupe_repositories_by_name(root, name);

                if !matches!(
                    root.get("repositories").map(|b| b.as_ref()),
                    Some(PhpMixed::List(_))
                ) {
                    root.insert(
                        "repositories".to_string(),
                        Box::new(PhpMixed::List(Vec::new())),
                    );
                }
                if let Some(PhpMixed::List(list)) = root.get_mut("repositories").map(|b| b.as_mut())
                {
                    if append {
                        list.push(Box::new(repo_config));
                    } else {
                        list.insert(0, Box::new(repo_config));
                    }
                }
                Ok(())
            },
        )
    }

    fn insert_repository(
        &mut self,
        name: &str,
        config: PhpMixed,
        reference_name: &str,
        offset: i64,
    ) -> Result<()> {
        let config_cloned = config.clone();
        self.manipulate_json(
            move |m| m.insert_repository(name, config_cloned, reference_name, offset),
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                Self::normalize_repositories_to_list(root);
                Self::dedupe_repositories_by_name(root, name);

                let mut index_to_insert: Option<usize> = None;
                if let Some(PhpMixed::List(list)) = root.get("repositories").map(|b| b.as_ref()) {
                    for (i, repository) in list.iter().enumerate() {
                        if repository.as_ref().get("name").and_then(|v| v.as_string())
                            == Some(reference_name)
                        {
                            index_to_insert = Some(i);
                            break;
                        }
                        if let PhpMixed::Array(m) = repository.as_ref()
                            && m.len() == 1
                            && matches!(
                                m.get(reference_name).map(|b| b.as_ref()),
                                Some(PhpMixed::Bool(false))
                            )
                        {
                            index_to_insert = Some(i);
                            break;
                        }
                    }
                }
                let Some(index_to_insert) = index_to_insert else {
                    return Err(RuntimeException {
                        message: format!(
                            "The referenced repository \"{}\" does not exist.",
                            PhpMixed::String(reference_name.to_string()),
                        ),
                        code: 0,
                    }
                    .into());
                };

                let mut repo_config = config;
                if let PhpMixed::Array(rc) = &repo_config
                    && !name.is_empty()
                    && !rc.contains_key("name")
                {
                    let mut with_name = IndexMap::new();
                    with_name.insert(
                        "name".to_string(),
                        Box::new(PhpMixed::String(name.to_string())),
                    );
                    for (k, v) in rc.clone() {
                        with_name.insert(k, v);
                    }
                    repo_config = PhpMixed::Array(with_name);
                }

                if let Some(PhpMixed::List(list)) = root.get_mut("repositories").map(|b| b.as_mut())
                {
                    let raw = index_to_insert as i64 + offset;
                    let pos = if raw < 0 {
                        (list.len() as i64 + raw).max(0)
                    } else {
                        raw.min(list.len() as i64)
                    } as usize;
                    list.insert(pos, Box::new(repo_config));
                }
                Ok(())
            },
        )
    }

    fn set_repository_url(&mut self, name: &str, url: &str) -> Result<()> {
        self.manipulate_json(
            move |m| m.set_repository_url(name, url),
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                match root.get_mut("repositories").map(|b| b.as_mut()) {
                    Some(PhpMixed::List(list)) => {
                        for repository in list.iter_mut() {
                            if repository.as_ref().get("name").and_then(|v| v.as_string())
                                == Some(name)
                            {
                                if let PhpMixed::Array(m) = repository.as_mut() {
                                    m.insert(
                                        "url".to_string(),
                                        Box::new(PhpMixed::String(url.to_string())),
                                    );
                                }
                                return Ok(());
                            }
                        }
                    }
                    Some(PhpMixed::Array(map)) => {
                        let mut target: Option<String> = None;
                        for (index, repository) in map.iter() {
                            if index == name
                                || repository.as_ref().get("name").and_then(|v| v.as_string())
                                    == Some(name)
                            {
                                target = Some(index.clone());
                                break;
                            }
                        }
                        if let Some(k) = target
                            && let Some(PhpMixed::Array(m)) = map.get_mut(&k).map(|b| b.as_mut())
                        {
                            m.insert(
                                "url".to_string(),
                                Box::new(PhpMixed::String(url.to_string())),
                            );
                        }
                    }
                    _ => {}
                }
                Ok(())
            },
        )
    }

    fn remove_repository(&mut self, name: &str) -> Result<()> {
        self.manipulate_json(
            move |m| m.remove_repository(name),
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                let had_key = matches!(
                    root.get("repositories").map(|b| b.as_ref()),
                    Some(PhpMixed::Array(m)) if m.contains_key(name)
                );
                if had_key {
                    if let Some(PhpMixed::Array(m)) =
                        root.get_mut("repositories").map(|b| b.as_mut())
                    {
                        m.shift_remove(name);
                    }
                } else {
                    Self::dedupe_repositories_by_name(root, name);
                }
                let is_empty = match root.get("repositories").map(|b| b.as_ref()) {
                    Some(PhpMixed::List(l)) => l.is_empty(),
                    Some(PhpMixed::Array(m)) => m.is_empty(),
                    _ => false,
                };
                if is_empty {
                    root.shift_remove("repositories");
                }
                Ok(())
            },
        )
    }

    fn add_config_setting(&mut self, name: &str, value: PhpMixed) -> Result<()> {
        let auth_config = self.auth_config;
        let value_cloned = value.clone();
        self.manipulate_json(
            // override manipulator method for auth config files
            move |m| {
                if auth_config {
                    let parts = explode(".", name);
                    m.add_sub_node(
                        parts.first().map(String::as_str).unwrap_or(""),
                        parts.get(1).map(String::as_str).unwrap_or(""),
                        value_cloned,
                        false,
                    )
                } else {
                    m.add_config_setting(name, value_cloned)
                }
            },
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                if Self::is_auth_config_key(name) {
                    let mut it = name.splitn(2, '.');
                    let key = it.next().unwrap_or("");
                    let host = it.next().unwrap_or("");
                    if auth_config {
                        Self::set_nested(root, &[key, host], value);
                    } else {
                        Self::set_nested(root, &["config", key, host], value);
                    }
                } else {
                    Self::set_nested(root, &["config", name], value);
                }
                Ok(())
            },
        )
    }

    fn remove_config_setting(&mut self, name: &str) -> Result<()> {
        let auth_config = self.auth_config;
        self.manipulate_json(
            // override manipulator method for auth config files
            move |m| {
                if auth_config {
                    let parts = explode(".", name);
                    m.remove_sub_node(
                        parts.first().map(String::as_str).unwrap_or(""),
                        parts.get(1).map(String::as_str).unwrap_or(""),
                    )
                } else {
                    m.remove_config_setting(name)
                }
            },
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                if Self::is_auth_config_key(name) {
                    let mut it = name.splitn(2, '.');
                    let key = it.next().unwrap_or("");
                    let host = it.next().unwrap_or("");
                    if auth_config {
                        Self::unset_nested(root, &[key, host]);
                    } else {
                        Self::unset_nested(root, &["config", key, host]);
                    }
                } else {
                    Self::unset_nested(root, &["config", name]);
                }
                Ok(())
            },
        )
    }

    fn add_property(&mut self, name: &str, value: PhpMixed) -> Result<()> {
        let value_cloned = value.clone();
        self.manipulate_json(
            move |m| m.add_property(name, value_cloned),
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                if name.starts_with("extra.") || name.starts_with("scripts.") {
                    let mut bits: Vec<&str> = name.split('.').collect();
                    let last = bits.pop().unwrap();
                    let first = bits[0];
                    let entry = root
                        .entry(first.to_string())
                        .or_insert_with(|| Box::new(PhpMixed::Array(IndexMap::new())));
                    if !matches!(entry.as_ref(), PhpMixed::Array(_)) {
                        **entry = PhpMixed::Array(IndexMap::new());
                    }
                    let mut cursor = match entry.as_mut() {
                        PhpMixed::Array(m) => m,
                        _ => unreachable!(),
                    };
                    for bit in &bits {
                        let e = cursor
                            .entry(bit.to_string())
                            .or_insert_with(|| Box::new(PhpMixed::Array(IndexMap::new())));
                        if !matches!(e.as_ref(), PhpMixed::Array(_)) {
                            **e = PhpMixed::Array(IndexMap::new());
                        }
                        cursor = match e.as_mut() {
                            PhpMixed::Array(m) => m,
                            _ => unreachable!(),
                        };
                    }
                    cursor.insert(last.to_string(), Box::new(value));
                } else {
                    root.insert(name.to_string(), Box::new(value));
                }
                Ok(())
            },
        )
    }

    fn remove_property(&mut self, name: &str) -> Result<()> {
        self.manipulate_json(
            move |m| m.remove_property(name),
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                let lower = name.to_ascii_lowercase();
                if name.starts_with("extra.")
                    || name.starts_with("scripts.")
                    || lower.starts_with("autoload.")
                    || lower.starts_with("autoload-dev.")
                {
                    let mut bits: Vec<&str> = name.split('.').collect();
                    let last = bits.pop().unwrap();
                    let first = bits[0];
                    let Some(entry) = root.get_mut(first) else {
                        return Ok(());
                    };
                    let mut cursor: &mut PhpMixed = entry.as_mut();
                    for bit in &bits {
                        cursor = match cursor {
                            PhpMixed::Array(m) if m.contains_key(*bit) => {
                                m.get_mut(*bit).unwrap().as_mut()
                            }
                            _ => return Ok(()),
                        };
                    }
                    if let PhpMixed::Array(m) = cursor {
                        m.shift_remove(last);
                    }
                } else {
                    root.shift_remove(name);
                }
                Ok(())
            },
        )
    }

    fn add_link(&mut self, r#type: &str, name: &str, value: &str) -> Result<()> {
        self.manipulate_json(
            move |m| m.add_link(r#type, name, value, false),
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                Self::set_nested(root, &[r#type, name], PhpMixed::String(value.to_string()));
                Ok(())
            },
        )
    }

    fn remove_link(&mut self, r#type: &str, name: &str) -> Result<()> {
        self.manipulate_json(
            move |m| m.remove_sub_node(r#type, name),
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                Self::unset_nested(root, &[r#type, name]);
                Ok(())
            },
        )?;
        self.manipulate_json(
            move |m| m.remove_main_key_if_empty(r#type),
            move |cfg| {
                let Some(root) = cfg.as_array_mut() else {
                    return Ok(());
                };
                let empty = match root.get(r#type).map(|b| b.as_ref()) {
                    Some(PhpMixed::Array(m)) => m.is_empty(),
                    Some(PhpMixed::List(l)) => l.is_empty(),
                    _ => false,
                };
                if empty {
                    root.shift_remove(r#type);
                }
                Ok(())
            },
        )
    }
}
