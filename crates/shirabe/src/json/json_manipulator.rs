//! ref: composer/src/Composer/Json/JsonManipulator.php

use crate::json::JsonFile;
use crate::json::json_grammar::{self, ValueKind};
use crate::repository::PlatformRepository;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, addcslashes, array_key_exists, array_keys,
    array_reverse, empty, explode, implode, in_array, is_array, is_int, is_numeric, json_decode,
    preg_quote, rtrim, str_contains, str_repeat, str_replace, strlen, strnatcmp, strpos, substr,
    trim, uksort,
};

#[derive(Debug)]
pub struct JsonManipulator {
    contents: String,
    newline: String,
    indent: String,
}

impl JsonManipulator {
    const DEFINES: &'static str = "(?(DEFINE)
       (?<number>    -? (?= [1-9]|0(?!\\d) ) \\d++ (?:\\.\\d++)? (?:[eE] [+-]?+ \\d++)? )
       (?<boolean>   true | false | null )
       (?<string>    \" (?:[^\"\\\\]*+ | \\\\ [\"\\\\bfnrt\\/] | \\\\ u [0-9A-Fa-f]{4} )* \" )
       (?<array>     \\[  (?:  (?&json) \\s*+ (?: , (?&json) \\s*+ )*+  )?+  \\s*+ \\] )
       (?<pair>      \\s*+ (?&string) \\s*+ : (?&json) \\s*+ )
       (?<object>    \\{  (?:  (?&pair)  (?: , (?&pair)  )*+  )?+  \\s*+ \\} )
       (?<json>      \\s*+ (?: (?&number) | (?&boolean) | (?&string) | (?&array) | (?&object) ) )
    )";

    pub fn new(contents: String) -> anyhow::Result<Self> {
        let mut contents = trim(&contents, Some(" \t\n\r\0\u{0B}"));
        if contents.is_empty() {
            contents = "{}".to_string();
        }
        if !Preg::is_match3("#^\\{(.*)\\}$#s", &contents, None) {
            return Err(InvalidArgumentException {
                message: "The json file must be an object ({})".to_string(),
                code: 0,
            }
            .into());
        }
        let newline = if strpos(&contents, "\r\n").is_some() {
            "\r\n".to_string()
        } else {
            "\n".to_string()
        };
        let mut s = Self {
            contents: if contents == "{}" {
                format!("{{{}}}", newline)
            } else {
                contents
            },
            newline,
            indent: String::new(),
        };
        s.detect_indenting();
        Ok(s)
    }

    pub fn get_contents(&self) -> String {
        format!("{}{}", self.contents, self.newline)
    }

    pub fn add_link(
        &mut self,
        r#type: &str,
        package: &str,
        constraint: &str,
        sort_packages: bool,
    ) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;

        // no link of that type yet
        if decoded.as_array().and_then(|a| a.get(r#type)).is_none() {
            let mut arr: IndexMap<String, PhpMixed> = IndexMap::new();
            arr.insert(
                package.to_string(),
                PhpMixed::String(constraint.to_string()),
            );
            return self.add_main_key(r#type, PhpMixed::Array(arr));
        }

        let m = match json_grammar::find_top_level_key(
            self.contents.as_bytes(),
            JsonFile::encode(r#type).as_bytes(),
            ValueKind::Json,
        ) {
            Some(m) => m,
            None => return Ok(false),
        };
        let start = self.contents[..m.key_pos].to_string();
        let property = self.contents[m.key_pos..m.value_pos].to_string();
        let end = self.contents[m.value_end..].to_string();

        let mut links = self.contents[m.value_pos..m.value_end].to_string();

        // try to find existing link
        if let Some((key_start, key_end, value_start, value_end)) =
            find_package_link(&links, package)
        {
            // update existing link: re-encode the existing key, keep its separator, swap the value
            let existing_package = links[key_start + 1..key_end - 1].to_string();
            let separator = links[key_end..value_start].to_string();
            links = format!(
                "{}{}{}\"{}\"{}",
                &links[..key_start],
                JsonFile::encode(&str_replace("\\/", "/", &existing_package)),
                separator,
                constraint,
                &links[value_end..]
            );
        } else {
            let mut groups: IndexMap<CaptureKey, String> = IndexMap::new();
            if Preg::is_match3(
                "#^\\s*\\{\\s*\\S+.*?(\\s*\\}\\s*)$#s",
                &links,
                Some(&mut groups),
            ) {
                let groups_1 = groups
                    .get(&CaptureKey::ByIndex(1))
                    .cloned()
                    .unwrap_or_default();
                // link missing but non empty links
                links = Preg::replace(
                    &format!("{{{}$}}", preg_quote(&groups_1, None)),
                    // addcslashes is used to double up backslashes/$ since preg_replace resolves them as back references otherwise, see #1588
                    &addcslashes(
                        &format!(
                            ",{}{}{}{}: {}{}",
                            self.newline,
                            self.indent,
                            self.indent,
                            JsonFile::encode(package),
                            JsonFile::encode(constraint),
                            groups_1
                        ),
                        "\\$",
                    ),
                    &links,
                );
            } else {
                // links empty
                links = format!(
                    "{{{}{}{}{}: {}{}{}}}",
                    self.newline,
                    self.indent,
                    self.indent,
                    JsonFile::encode(package),
                    JsonFile::encode(constraint),
                    self.newline,
                    self.indent
                );
            }
        }

        if sort_packages {
            let mut requirements = json_decode(&links, true)?;
            Self::sort_packages(&mut requirements);
            links = self.format(&requirements, 0, false)?;
        }

        self.contents = format!("{}{}{}{}", start, property, links, end);

        Ok(true)
    }

    /// Sorts packages by importance (platform packages first, then PHP dependencies) and alphabetically.
    fn sort_packages(packages: &mut PhpMixed) {
        let prefix = |requirement: &str| -> String {
            if PlatformRepository::is_platform_package(requirement) {
                let patterns = ["/^php/", "/^hhvm/", "/^ext/", "/^lib/", "/^\\D/"];
                let replacements = ["0-$0", "1-$0", "2-$0", "3-$0", "4-$0"];
                let mut result = requirement.to_string();
                for (p, r) in patterns.iter().zip(replacements.iter()) {
                    result = Preg::replace(p, r, &result);
                }
                result
            } else {
                format!("5-{}", requirement)
            }
        };

        if let Some(arr) = packages.as_array_mut() {
            uksort(arr, |a: &str, b: &str| -> i64 {
                strnatcmp(&prefix(a), &prefix(b))
            });
        }
    }

    pub fn add_repository(
        &mut self,
        name: &str,
        config: PhpMixed,
        append: bool,
    ) -> anyhow::Result<bool> {
        if !name.is_empty() && !self.do_remove_repository(name)? {
            return Ok(false);
        }

        if !self.do_convert_repositories_from_assoc_to_list()? {
            return Ok(false);
        }

        let final_config = if is_array(&config)
            && !is_numeric(&PhpMixed::String(name.to_string()))
            && !name.is_empty()
        {
            // PHP: ['name' => $name] + $config — preserve $config keys
            let mut merged: IndexMap<String, PhpMixed> = IndexMap::new();
            merged.insert("name".to_string(), PhpMixed::String(name.to_string()));
            if let Some(arr) = config.as_array() {
                for (k, v) in arr {
                    if !merged.contains_key(k) {
                        merged.insert(k.clone(), v.clone());
                    }
                }
            }
            PhpMixed::Array(merged)
        } else if config.as_bool() == Some(false) {
            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
            m.insert(name.to_string(), PhpMixed::Bool(false));
            PhpMixed::Array(m)
        } else {
            config
        };

        self.add_list_item("repositories", final_config, append)
    }

    fn do_convert_repositories_from_assoc_to_list(&mut self) -> anyhow::Result<bool> {
        let decoded = json_decode(&self.contents, false)?;

        let repositories_value: Option<PhpMixed> = decoded
            .as_object()
            .and_then(|o| o.get("repositories").cloned());
        let is_std_class = repositories_value
            .as_ref()
            .map(|v| v.as_object().is_some())
            .unwrap_or(false);

        if is_std_class {
            // delete from bottom to top, to ensure keys stay the same
            let repos_arr: IndexMap<String, PhpMixed> = repositories_value
                .as_ref()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            let entries_to_revert: Vec<String> = array_reverse(&array_keys(&repos_arr), false);

            for entry_key in &entries_to_revert {
                if !self.remove_sub_node("repositories", entry_key)? {
                    return Ok(false);
                }
            }

            self.change_empty_main_key_from_assoc_to_list("repositories")?;

            // re-add in order
            for (repository_name, repository) in &repos_arr {
                let is_obj = repository.as_object().is_some();
                if !is_obj {
                    let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
                    m.insert(repository_name.clone(), repository.clone());
                    if !self.add_list_item("repositories", PhpMixed::Array(m), true)? {
                        return Ok(false);
                    }
                } else if is_numeric(&PhpMixed::String(repository_name.clone())) {
                    if !self.add_list_item("repositories", repository.clone(), true)? {
                        return Ok(false);
                    }
                } else {
                    let repo: IndexMap<String, PhpMixed> = repository
                        .as_array()
                        .or_else(|| repository.as_object())
                        .cloned()
                        .unwrap_or_default();
                    // prepend name property
                    let mut prepended: IndexMap<String, PhpMixed> = IndexMap::new();
                    prepended.insert(
                        "name".to_string(),
                        PhpMixed::String(repository_name.clone()),
                    );
                    for (k, v) in &repo {
                        if !prepended.contains_key(k) {
                            prepended.insert(k.clone(), v.clone());
                        }
                    }
                    if !self.add_list_item("repositories", PhpMixed::Array(prepended), true)? {
                        return Ok(false);
                    }
                }
            }
        }

        Ok(true)
    }

    pub fn set_repository_url(&mut self, name: &str, url: &str) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;
        let mut repository_index: Option<PhpMixed> = None;

        let repos_value = decoded
            .as_array()
            .and_then(|a| a.get("repositories"))
            .cloned();
        // A list keeps integer indices (matching PHP's `is_int($index)` branch); an associative
        // "repositories" object keeps string keys.
        let is_list = repos_value.as_ref().and_then(|v| v.as_list()).is_some();
        let repos: Vec<(String, PhpMixed)> =
            if let Some(list) = repos_value.as_ref().and_then(|v| v.as_list()) {
                list.iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string(), v.clone()))
                    .collect()
            } else {
                repos_value
                    .as_ref()
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect()
            };
        for (index, repository) in &repos {
            let index_value = if is_list {
                PhpMixed::Int(index.parse::<i64>().unwrap_or(0))
            } else {
                PhpMixed::String(index.clone())
            };
            if name == index.as_str() {
                repository_index = Some(index_value);
                break;
            }

            let repo_name = repository
                .as_array()
                .and_then(|a| a.get("name"))
                .and_then(|v| v.as_string());
            if Some(name) == repo_name {
                repository_index = Some(index_value);
                break;
            }
        }

        let repository_index = match repository_index {
            Some(r) => r,
            None => return Ok(false),
        };

        // Locate the byte span of the target repository object within the contents.
        let repo_span: Option<(usize, usize)> = if is_int(&repository_index) {
            let n = repository_index.as_int().unwrap_or(0).max(0);
            json_grammar::find_top_level_key(
                self.contents.as_bytes(),
                b"\"repositories\"",
                ValueKind::Array,
            )
            .and_then(|reps| {
                let cb = self.contents.as_bytes();
                let mut i = json_grammar::skip_ws(cb, reps.value_pos + 1); // '[' then \s*
                for _ in 0..n {
                    i = json_grammar::scan_value(cb, i)?;
                    i = json_grammar::skip_ws(cb, i);
                    if cb.get(i) != Some(&b',') {
                        return None;
                    }
                    i = json_grammar::skip_ws(cb, i + 1);
                }
                if cb.get(i) != Some(&b'{') {
                    return None;
                }
                let e = json_grammar::scan_object(cb, i)?;
                Some((i, e))
            })
        } else {
            json_grammar::find_top_level_key(
                self.contents.as_bytes(),
                b"\"repositories\"",
                ValueKind::Object,
            )
            .and_then(|reps| {
                let obj = self.contents[reps.value_pos..reps.value_end].to_string();
                let key = JsonFile::encode(&repository_index);
                json_grammar::find_top_level_key(obj.as_bytes(), key.as_bytes(), ValueKind::Object)
                    .map(|inner| {
                        (
                            reps.value_pos + inner.value_pos,
                            reps.value_pos + inner.value_end,
                        )
                    })
            })
        };

        let (repo_pos, repo_end) = match repo_span {
            Some(span) => span,
            None => return Ok(false),
        };

        let raw_repo = self.contents[repo_pos..repo_end].to_string();
        // invalid match due to un-regexable content, abort
        if json_decode(&raw_repo, false)?.as_bool() == Some(false) {
            return Ok(false);
        }

        let start_outer = self.contents[..repo_pos].to_string();
        let end_outer = self.contents[repo_end..].to_string();

        // Replace the repository's "url" value, leaving the rest of the object untouched.
        let new_raw_repo = match json_grammar::find_top_level_key(
            raw_repo.as_bytes(),
            b"\"url\"",
            ValueKind::Json,
        ) {
            Some(u) => format!(
                "{}{}{}",
                &raw_repo[..u.value_pos],
                JsonFile::encode(url),
                &raw_repo[u.value_end..]
            ),
            None => raw_repo,
        };

        self.contents = format!("{}{}{}", start_outer, new_raw_repo, end_outer);

        Ok(true)
    }

    pub fn insert_repository(
        &mut self,
        name: &str,
        config: PhpMixed,
        reference_name: &str,
        offset: i64,
    ) -> anyhow::Result<bool> {
        if !name.is_empty() && !self.do_remove_repository(name)? {
            return Ok(false);
        }

        if !self.do_convert_repositories_from_assoc_to_list()? {
            return Ok(false);
        }

        let mut index_to_insert: Option<i64> = None;
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;

        let repos = decoded
            .as_array()
            .and_then(|a| a.get("repositories"))
            .and_then(|v| v.as_list())
            .cloned()
            .unwrap_or_default();
        for (i, repository) in repos.iter().enumerate() {
            let repo_name = repository
                .as_array()
                .and_then(|a| a.get("name"))
                .and_then(|v| v.as_string());
            if Some(reference_name) == repo_name {
                index_to_insert = Some(i as i64);
                break;
            }

            // PHP: $repositoryIndex === $referenceName — comparing list index to a string is rare; skip in Rust port
            // PHP: [$referenceName => false] === $repository
            if let Some(arr) = repository.as_array()
                && arr.len() == 1
                && arr
                    .get(reference_name)
                    .map(|v| v.as_bool() == Some(false))
                    .unwrap_or(false)
            {
                index_to_insert = Some(i as i64);
                break;
            }
        }

        let index_to_insert = match index_to_insert {
            Some(i) => i,
            None => return Ok(false),
        };

        let final_config = if is_array(&config)
            && !is_numeric(&PhpMixed::String(name.to_string()))
            && !name.is_empty()
        {
            let mut merged: IndexMap<String, PhpMixed> = IndexMap::new();
            merged.insert("name".to_string(), PhpMixed::String(name.to_string()));
            if let Some(arr) = config.as_array() {
                for (k, v) in arr {
                    if !merged.contains_key(k) {
                        merged.insert(k.clone(), v.clone());
                    }
                }
            }
            PhpMixed::Array(merged)
        } else if config.as_bool() == Some(false) {
            let mut m: IndexMap<String, PhpMixed> = IndexMap::new();
            m.insert("name".to_string(), PhpMixed::Bool(false));
            PhpMixed::Array(m)
        } else {
            config
        };

        self.insert_list_item("repositories", final_config, index_to_insert + offset)
    }

    pub fn remove_repository(&mut self, name: &str) -> anyhow::Result<bool> {
        Ok(self.do_remove_repository(name)? && self.remove_main_key_if_empty("repositories")?)
    }

    fn do_remove_repository(&mut self, name: &str) -> anyhow::Result<bool> {
        let decoded = json_decode(&self.contents, false)?;
        let repositories_value: Option<PhpMixed> = decoded
            .as_object()
            .and_then(|o| o.get("repositories").cloned());
        let is_assoc = repositories_value
            .as_ref()
            .map(|v| v.as_object().is_some())
            .unwrap_or(false);

        // `repositories` may be an associative object or a positional list; in either case iterate
        // it as (index, repository) pairs.
        let repos: Vec<(String, PhpMixed)> =
            if let Some(o) = repositories_value.as_ref().and_then(|v| v.as_object()) {
                o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            } else if let Some(l) = repositories_value.as_ref().and_then(|v| v.as_list()) {
                l.iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string(), v.clone()))
                    .collect()
            } else {
                Vec::new()
            };

        for (repository_index, repository) in &repos {
            if repository_index == name && is_assoc {
                if !self.remove_sub_node("repositories", repository_index)? {
                    return Ok(false);
                }

                break;
            }

            let repo_name_owned: Option<String> = repository
                .as_object()
                .and_then(|o| o.get("name").cloned())
                .and_then(|v| v.as_string().map(|s| s.to_string()));
            if Some(name) == repo_name_owned.as_deref() {
                if is_assoc {
                    if !self.remove_sub_node("repositories", repository_index)? {
                        return Ok(false);
                    }
                } else {
                    let idx: i64 = repository_index.parse().unwrap_or(0);
                    if !self.remove_list_item("repositories", idx)? {
                        return Ok(false);
                    }
                }

                break;
            }

            if is_assoc {
                if name == repository_index && repository.as_bool() == Some(false) {
                    if !self.remove_sub_node("repositories", repository_index)? {
                        return Ok(false);
                    }

                    return Ok(true);
                }
            } else {
                let repository_as_array: IndexMap<String, PhpMixed> = repository
                    .as_array()
                    .or_else(|| repository.as_object())
                    .cloned()
                    .unwrap_or_default();

                if repository_as_array
                    .get(name)
                    .map(|v| v.as_bool() == Some(false))
                    .unwrap_or(false)
                    && 1 == repository_as_array.len()
                {
                    let idx: i64 = repository_index.parse().unwrap_or(0);
                    if !self.remove_list_item("repositories", idx)? {
                        return Ok(false);
                    }

                    return Ok(true);
                }
            }
        }

        Ok(true)
    }

    pub fn add_config_setting(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<bool> {
        self.add_sub_node("config", name, value, true)
    }

    pub fn remove_config_setting(&mut self, name: &str) -> anyhow::Result<bool> {
        self.remove_sub_node("config", name)
    }

    pub fn add_property(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<bool> {
        if strpos(name, "suggest.") == Some(0) {
            return self.add_sub_node("suggest", &substr(name, 8, None), value, true);
        }

        if strpos(name, "extra.") == Some(0) {
            return self.add_sub_node("extra", &substr(name, 6, None), value, true);
        }

        if strpos(name, "scripts.") == Some(0) {
            return self.add_sub_node("scripts", &substr(name, 8, None), value, true);
        }

        self.add_main_key(name, value)
    }

    pub fn remove_property(&mut self, name: &str) -> anyhow::Result<bool> {
        if strpos(name, "suggest.") == Some(0) {
            return self.remove_sub_node("suggest", &substr(name, 8, None));
        }

        if strpos(name, "extra.") == Some(0) {
            return self.remove_sub_node("extra", &substr(name, 6, None));
        }

        if strpos(name, "scripts.") == Some(0) {
            return self.remove_sub_node("scripts", &substr(name, 8, None));
        }

        if strpos(name, "autoload.") == Some(0) {
            return self.remove_sub_node("autoload", &substr(name, 9, None));
        }

        if strpos(name, "autoload-dev.") == Some(0) {
            return self.remove_sub_node("autoload-dev", &substr(name, 13, None));
        }

        self.remove_main_key(name)
    }

    pub fn add_sub_node(
        &mut self,
        main_node: &str,
        name: &str,
        value: PhpMixed,
        append: bool,
    ) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;

        let mut name_owned = name.to_string();
        let mut sub_name: Option<String> = None;
        if in_array(
            PhpMixed::String(main_node.to_string()),
            &PhpMixed::List(vec![
                PhpMixed::String("config".to_string()),
                PhpMixed::String("extra".to_string()),
                PhpMixed::String("scripts".to_string()),
            ]),
            false,
        ) && strpos(name, ".").is_some()
        {
            let parts = explode(".", name);
            // PHP: explode('.', $name, 2)
            let first = parts[0].clone();
            let rest = parts[1..].join(".");
            name_owned = first;
            sub_name = Some(rest);
        }

        // no main node yet
        if decoded.as_array().and_then(|a| a.get(main_node)).is_none() {
            if let Some(ref sub) = sub_name {
                let mut inner: IndexMap<String, PhpMixed> = IndexMap::new();
                inner.insert(sub.clone(), value.clone());
                let mut outer: IndexMap<String, PhpMixed> = IndexMap::new();
                outer.insert(name_owned.clone(), PhpMixed::Array(inner));
                self.add_main_key(main_node, PhpMixed::Array(outer))?;
            } else {
                let mut outer: IndexMap<String, PhpMixed> = IndexMap::new();
                outer.insert(name_owned.clone(), value.clone());
                self.add_main_key(main_node, PhpMixed::Array(outer))?;
            }

            return Ok(true);
        }

        // main node content not match-able
        let node = match json_grammar::find_top_level_key(
            self.contents.as_bytes(),
            JsonFile::encode(main_node).as_bytes(),
            ValueKind::Object,
        ) {
            Some(node) => node,
            None => return Ok(false),
        };
        let node_start = self.contents[..node.value_pos].to_string();
        let node_end = self.contents[node.value_end..].to_string();
        let mut children = self.contents[node.value_pos..node.value_end].to_string();
        // invalid match due to un-regexable content, abort
        if json_decode(&children, false)?.is_null()
            || json_decode(&children, false)?.as_bool() == Some(false)
        {
            return Ok(false);
        }

        // child exists. The child pattern looks for the raw `"name"` key token within the children
        // object and rewrites its value in place; the surrounding `"name"\s*:\s*` and optional comma
        // are preserved as-is.
        let child_key = format!("\"{}\"", name_owned);
        let child = json_grammar::find_top_level_key(
            children.as_bytes(),
            child_key.as_bytes(),
            ValueKind::Json,
        );
        if let Some(cm) = child {
            let content_str = children[cm.value_pos..cm.value_end].to_string();
            let mut value_local = value.clone();
            if sub_name.is_some() {
                let mut cur_val = json_decode(&content_str, true).unwrap_or(PhpMixed::Null);
                if !is_array(&cur_val) {
                    cur_val = PhpMixed::Array(IndexMap::new());
                }
                if let Some(arr) = cur_val.as_array_mut() {
                    arr.insert(sub_name.clone().unwrap(), value_local.clone());
                }
                value_local = cur_val;
            }
            let formatted = self.format(&value_local, 1, false)?;
            children = format!(
                "{}{}{}",
                &children[..cm.value_pos],
                formatted,
                &children[cm.value_end..]
            );
        } else {
            let mut leading_match: IndexMap<String, String> = IndexMap::new();
            if Preg::is_match_named(
                "#^\\{(?P<leadingspace>\\s*?)(?P<content>\\S+.*?)?(?P<trailingspace>\\s*)\\}$#s",
                &children,
                &mut leading_match,
            ) {
                let mut whitespace = leading_match
                    .get("trailingspace")
                    .cloned()
                    .unwrap_or_default();
                let leading_space = leading_match
                    .get("leadingspace")
                    .cloned()
                    .unwrap_or_default();
                let content_present = leading_match.get("content").is_some();
                if content_present {
                    let mut value_local = value.clone();
                    if let Some(ref sub) = sub_name {
                        let mut wrap: IndexMap<String, PhpMixed> = IndexMap::new();
                        wrap.insert(sub.clone(), value_local.clone());
                        value_local = PhpMixed::Array(wrap);
                    }

                    // child missing but non empty children
                    if append {
                        children = Preg::replace(
                            &format!("#{}}}$#", whitespace),
                            &addcslashes(
                                &format!(
                                    ",{}{}{}{}: {}{}}}",
                                    self.newline,
                                    self.indent,
                                    self.indent,
                                    JsonFile::encode(&name_owned),
                                    self.format(&value_local, 1, false)?,
                                    whitespace
                                ),
                                "\\$",
                            ),
                            &children,
                        );
                    } else {
                        whitespace = leading_space.clone();
                        children = Preg::replace(
                            &format!("#^{{{}#", whitespace),
                            &addcslashes(
                                &format!(
                                    "{{{}{}: {},{}{}{}",
                                    whitespace,
                                    JsonFile::encode(&name_owned),
                                    self.format(&value_local, 1, false)?,
                                    self.newline,
                                    self.indent,
                                    self.indent
                                ),
                                "\\$",
                            ),
                            &children,
                        );
                    }
                } else {
                    let mut value_local = value.clone();
                    if let Some(ref sub) = sub_name {
                        let mut wrap: IndexMap<String, PhpMixed> = IndexMap::new();
                        wrap.insert(sub.clone(), value_local.clone());
                        value_local = PhpMixed::Array(wrap);
                    }

                    // children present but empty
                    children = format!(
                        "{{{}{}{}{}: {}{}}}",
                        self.newline,
                        self.indent,
                        self.indent,
                        JsonFile::encode(&name_owned),
                        self.format(&value_local, 1, false)?,
                        whitespace
                    );
                }
            } else {
                return Err(LogicException {
                    message: format!("Nothing matched above for: {}", children),
                    code: 0,
                }
                .into());
            }
        }

        self.contents = format!("{}{}{}", node_start, children, node_end);

        Ok(true)
    }

    pub fn remove_sub_node(&mut self, main_node: &str, name: &str) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;

        // no node or empty node
        let main_node_value = decoded.as_array().and_then(|a| a.get(main_node));
        if main_node_value.map(empty).unwrap_or(true) {
            return Ok(true);
        }

        // no node content match-able
        let node = match json_grammar::find_top_level_key(
            self.contents.as_bytes(),
            JsonFile::encode(main_node).as_bytes(),
            ValueKind::Object,
        ) {
            Some(node) => node,
            None => return Ok(false),
        };
        let node_start = self.contents[..node.value_pos].to_string();
        let node_end = self.contents[node.value_end..].to_string();
        let children = self.contents[node.value_pos..node.value_end].to_string();

        // invalid match due to un-regexable content, abort
        if json_decode(&children, true)?.is_null()
            || json_decode(&children, true)?.as_bool() == Some(false)
        {
            return Ok(false);
        }

        let mut name_owned = name.to_string();
        let mut sub_name: Option<String> = None;
        if in_array(
            PhpMixed::String(main_node.to_string()),
            &PhpMixed::List(vec![
                PhpMixed::String("config".to_string()),
                PhpMixed::String("extra".to_string()),
                PhpMixed::String("scripts".to_string()),
            ]),
            false,
        ) && strpos(name, ".").is_some()
        {
            let parts = explode(".", name);
            let first = parts[0].clone();
            let rest = parts[1..].join(".");
            name_owned = first;
            sub_name = Some(rest);
        }

        // no node to remove
        let main_arr = main_node_value
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        if !main_arr.contains_key(&name_owned)
            || (sub_name.is_some()
                && !main_arr
                    .get(&name_owned)
                    .and_then(|v| v.as_array())
                    .map(|a| a.contains_key(sub_name.as_ref().unwrap()))
                    .unwrap_or(false))
        {
            return Ok(true);
        }

        // try and find a match for the subkey
        let key_regex = str_replace("/", "\\\\?/", &preg_quote(&name_owned, None));
        let mut children_clean: Option<String> = None;
        if Preg::is_match3(&format!("{{\"{}\"\\s*:}}i", key_regex), &children, None) {
            // find best match for the value of "name". The PHP pattern `"name"\s*:\s*(?&json)` is
            // not anchored, so it can match the key at several nesting levels; collect every such
            // occurrence and keep the longest, reproducing PHP's behaviour.
            let key_value_matches = find_key_value_matches(&children, &name_owned);
            if !key_value_matches.is_empty() {
                let mut best_match: String = String::new();
                for m in &key_value_matches {
                    if strlen(&best_match) < strlen(m) {
                        best_match = m.clone();
                    }
                }
                let mut count_out: usize = 0;
                let cleaned = Preg::replace5(
                    &format!("{{,\\s*{}}}i", preg_quote(&best_match, None)),
                    "",
                    &children,
                    -1,
                    &mut count_out,
                );
                if 1 != count_out {
                    let cleaned2 = Preg::replace5(
                        &format!("{{{}\\s*,?\\s*}}i", preg_quote(&best_match, None)),
                        "",
                        &cleaned,
                        -1,
                        &mut count_out,
                    );
                    if 1 != count_out {
                        return Ok(false);
                    }
                    children_clean = Some(cleaned2);
                } else {
                    children_clean = Some(cleaned);
                }
            }
        } else {
            children_clean = Some(children.clone());
        }

        let children_clean = children_clean.ok_or_else(|| InvalidArgumentException {
            message: "JsonManipulator: $childrenClean is not defined. Please report at https://github.com/composer/composer/issues/new.".to_string(),
            code: 0,
        })?;

        // no child data left, $name was the only key in
        let mut empty_match: IndexMap<String, String> = IndexMap::new();
        if Preg::is_match_named(
            "#^\\{\\s*?(?P<content>\\S+.*?)?(?P<trailingspace>\\s*)\\}$#s",
            &children_clean,
            &mut empty_match,
        ) && empty_match.get("content").is_none()
        {
            self.contents = format!(
                "{}{{{}{}}}{}",
                node_start, self.newline, self.indent, node_end
            );

            // we have a subname, so we restore the rest of $name
            if let Some(sub) = sub_name {
                let mut cur_val = json_decode(&children, true)?;
                if let Some(arr) = cur_val.as_array_mut() {
                    if let Some(inner) = arr.get_mut(&name_owned).and_then(|v| v.as_array_mut()) {
                        inner.shift_remove(&sub);
                    }
                    let now_empty = arr
                        .get(&name_owned)
                        .and_then(|v| v.as_array())
                        .map(|a| a.is_empty())
                        .unwrap_or(false);
                    if now_empty {
                        arr.insert(name_owned.clone(), PhpMixed::Object(IndexMap::new()));
                    }
                }
                let val = cur_val
                    .as_array()
                    .and_then(|a| a.get(&name_owned))
                    .cloned()
                    .unwrap_or(PhpMixed::Null);
                self.add_sub_node(main_node, &name_owned, val, true)?;
            }

            return Ok(true);
        }

        // The node content matched here is the original `children` object; rebuild it with the
        // subkey removed when a sub_name is in play.
        let mut children_final = children_clean.clone();
        if let Some(ref sub) = sub_name {
            let mut cur_val = json_decode(&children, true).unwrap_or(PhpMixed::Null);
            if let Some(arr) = cur_val.as_array_mut() {
                if let Some(inner) = arr.get_mut(&name_owned).and_then(|v| v.as_array_mut()) {
                    inner.shift_remove(sub);
                }
                let now_empty = arr
                    .get(&name_owned)
                    .and_then(|v| v.as_array())
                    .map(|a| a.is_empty())
                    .unwrap_or(false);
                if now_empty {
                    arr.insert(name_owned.clone(), PhpMixed::Object(IndexMap::new()));
                }
            }
            children_final = self.format(&cur_val, 0, true)?;
        }

        self.contents = format!("{}{}{}", node_start, children_final, node_end);

        Ok(true)
    }

    pub fn add_list_item(
        &mut self,
        main_node: &str,
        value: PhpMixed,
        append: bool,
    ) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;

        // no main node yet
        if decoded.as_array().and_then(|a| a.get(main_node)).is_none()
            && !self.add_main_key(main_node, PhpMixed::List(vec![]))?
        {
            return Ok(false);
        }

        // main node content not match-able
        let node = match json_grammar::find_top_level_key(
            self.contents.as_bytes(),
            JsonFile::encode(main_node).as_bytes(),
            ValueKind::Array,
        ) {
            Some(node) => node,
            None => return Ok(false),
        };
        let node_start = self.contents[..node.value_pos].to_string();
        let node_end = self.contents[node.value_end..].to_string();
        let mut children = self.contents[node.value_pos..node.value_end].to_string();
        // invalid match due to un-regexable content, abort
        if json_decode(&children, false)?.as_bool() == Some(false) {
            return Ok(false);
        }

        let mut leading_match: IndexMap<String, String> = IndexMap::new();
        if Preg::is_match_named(
            "#^\\[(?P<leadingspace>\\s*?)(?P<content>\\S+.*?)?(?P<trailingspace>\\s*)\\]$#s",
            &children,
            &mut leading_match,
        ) {
            let leading_whitespace = leading_match
                .get("leadingspace")
                .cloned()
                .unwrap_or_default();
            let mut whitespace = leading_match
                .get("trailingspace")
                .cloned()
                .unwrap_or_default();
            let mut leading_item_whitespace =
                format!("{}{}{}", self.newline, self.indent, self.indent);
            let mut trailing_item_whitespace = whitespace.clone();
            let mut item_depth: i64 = 1;

            // keep oneline lists as one line
            if !str_contains(&whitespace, &self.newline) {
                leading_item_whitespace = leading_whitespace.clone();
                trailing_item_whitespace = leading_whitespace.clone();
                item_depth = 0;
            }

            if leading_match.get("content").is_some() {
                // child missing but non empty children
                if append {
                    children = Preg::replace(
                        &format!("#{}\\]$#", whitespace),
                        &addcslashes(
                            &format!(
                                ",{}{}{}]",
                                leading_item_whitespace,
                                self.format(&value, item_depth, false)?,
                                trailing_item_whitespace
                            ),
                            "\\$",
                        ),
                        &children,
                    );
                } else {
                    whitespace = leading_whitespace.clone();
                    children = Preg::replace(
                        &format!("#^\\[{}#", whitespace),
                        &addcslashes(
                            &format!(
                                "[{}{},{}",
                                whitespace,
                                self.format(&value, item_depth, false)?,
                                leading_item_whitespace
                            ),
                            "\\$",
                        ),
                        &children,
                    );
                }
            } else {
                // children present but empty
                children = format!(
                    "[{}{}{}]",
                    leading_item_whitespace,
                    self.format(&value, item_depth, false)?,
                    trailing_item_whitespace
                );
            }
        } else {
            return Err(LogicException {
                message: format!("Nothing matched above for: {}", children),
                code: 0,
            }
            .into());
        }

        self.contents = format!("{}{}{}", node_start, children, node_end);

        Ok(true)
    }

    pub fn insert_list_item(
        &mut self,
        main_node: &str,
        value: PhpMixed,
        index: i64,
    ) -> anyhow::Result<bool> {
        if index < 0 {
            return Err(InvalidArgumentException {
                message: "Index can only be positive integer".to_string(),
                code: 0,
            }
            .into());
        }

        if index == 0 {
            return self.add_list_item(main_node, value, false);
        }

        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;

        // no main node yet
        if decoded.as_array().and_then(|a| a.get(main_node)).is_none()
            && !self.add_main_key(main_node, PhpMixed::List(vec![]))?
        {
            return Ok(false);
        }

        let main_node_count = decoded
            .as_array()
            .and_then(|a| a.get(main_node))
            .and_then(|v| v.as_list())
            .map(|l| l.len() as i64)
            .unwrap_or(0);
        if main_node_count == index {
            return self.add_list_item(main_node, value, true);
        }

        // main node content not match-able
        let node = match json_grammar::find_top_level_key(
            self.contents.as_bytes(),
            JsonFile::encode(main_node).as_bytes(),
            ValueKind::Array,
        ) {
            Some(node) => node,
            None => return Ok(false),
        };
        let node_start = self.contents[..node.value_pos].to_string();
        let node_end = self.contents[node.value_end..].to_string();
        let children = self.contents[node.value_pos..node.value_end].to_string();
        // invalid match due to un-regexable content, abort
        if json_decode(&children, false)?.as_bool() == Some(false) {
            return Ok(false);
        }

        // Skip past the first `index` array items (each `value <ws> , <lazy ws>`) and insert the new
        // value before the item at `index`, preserving the whitespace that precedes it.
        let new_children = {
            let cb = children.as_bytes();
            let mut i = json_grammar::skip_ws(cb, 1); // '[' then \s*
            for _ in 0..index.max(0) {
                i = json_grammar::scan_value(cb, i)
                    .ok_or_else(|| anyhow::anyhow!("insert_list_item: malformed list"))?;
                i = json_grammar::skip_ws(cb, i);
                if cb.get(i) != Some(&b',') {
                    return Ok(false);
                }
                i += 1;
            }
            let start_end = i;
            let sbi_end = json_grammar::skip_ws(cb, i);
            let space_before_item = &children[start_end..sbi_end];
            let formatted = self.format(&value, 1, false)?;
            format!(
                "{}{}{},{}{}",
                &children[..start_end],
                space_before_item,
                formatted,
                space_before_item,
                &children[sbi_end..]
            )
        };

        self.contents = format!("{}{}{}", node_start, new_children, node_end);

        Ok(true)
    }

    pub fn remove_list_item(&mut self, main_node: &str, node_index: i64) -> anyhow::Result<bool> {
        // invalid index, that cannot be removed anyway
        if node_index < 0 {
            return Ok(true);
        }

        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;

        // no node or empty node
        let main_node_value = decoded.as_array().and_then(|a| a.get(main_node));
        if main_node_value.map(empty).unwrap_or(true) {
            return Ok(true);
        }

        // no node content match-able
        let node = match json_grammar::find_top_level_key(
            self.contents.as_bytes(),
            JsonFile::encode(main_node).as_bytes(),
            ValueKind::Array,
        ) {
            Some(node) => node,
            None => return Ok(false),
        };
        let node_start = self.contents[..node.value_pos].to_string();
        let node_end = self.contents[node.value_end..].to_string();
        let children = self.contents[node.value_pos..node.value_end].to_string();

        // invalid match due to un-regexable content, abort
        if json_decode(&children, true)?.as_bool() == Some(false) {
            return Ok(false);
        }

        // no node to remove
        let main_list = main_node_value
            .and_then(|v| v.as_list())
            .cloned()
            .unwrap_or_default();
        if main_list.get(node_index as usize).is_none() {
            return Ok(true);
        }

        // Locate the byte range `[a..b]` to drop: the item at `node_index` together with one
        // array separator (the trailing comma for the first item, otherwise the leading comma).
        let cb = children.as_bytes();
        let removal: Option<(usize, usize)> = (|| {
            let mut i = json_grammar::skip_ws(cb, 1); // '[' then \s*
            if node_index == 0 {
                let a = i;
                let mut e = json_grammar::scan_value(cb, a)?;
                e = json_grammar::skip_ws(cb, e);
                if cb.get(e) == Some(&b',') {
                    e += 1;
                }
                e = json_grammar::skip_ws(cb, e);
                Some((a, e))
            } else {
                i = json_grammar::scan_value(cb, i)?;
                i = json_grammar::skip_ws(cb, i);
                for _ in 0..(node_index - 1) {
                    if cb.get(i) != Some(&b',') {
                        return None;
                    }
                    i = json_grammar::scan_value(cb, i + 1)?;
                    i = json_grammar::skip_ws(cb, i);
                }
                let a = i;
                let mut b = json_grammar::skip_ws(cb, a);
                if cb.get(b) == Some(&b',') {
                    b += 1;
                }
                b = json_grammar::skip_ws(cb, b);
                b = json_grammar::scan_value(cb, b)?;
                Some((a, b))
            }
        })();

        if let Some((a, b)) = removal {
            self.contents = format!(
                "{}{}{}{}",
                node_start,
                &children[..a],
                &children[b..],
                node_end
            );

            return Ok(true);
        }

        Ok(false)
    }

    pub fn add_main_key(&mut self, key: &str, content: PhpMixed) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;
        let content = self.format(&content, 0, false)?;

        // key exists already
        let encoded_key = JsonFile::encode(key);
        let key_match = if decoded.as_array().and_then(|a| a.get(key)).is_some() {
            json_grammar::find_top_level_key(
                self.contents.as_bytes(),
                encoded_key.as_bytes(),
                ValueKind::Json,
            )
        } else {
            None
        };
        if let Some(m) = key_match {
            // invalid match due to un-regexable content, abort
            let key_capture = &self.contents[m.key_pos..m.value_end];
            if json_decode(&format!("{{{}}}", key_capture), false)?.is_null() {
                return Ok(false);
            }

            self.contents = format!(
                "{}{}: {}{}",
                &self.contents[..m.key_pos],
                encoded_key,
                content,
                &self.contents[m.value_end..]
            );

            return Ok(true);
        }

        // append at the end of the file and keep whitespace
        let mut tail_match: IndexMap<CaptureKey, String> = IndexMap::new();
        if Preg::is_match3("#[^{\\s](\\s*)\\}$#", &self.contents, Some(&mut tail_match)) {
            let tail_match_1 = tail_match
                .get(&CaptureKey::ByIndex(1))
                .cloned()
                .unwrap_or_default();
            self.contents = Preg::replace(
                &format!("#{}\\}}$#", tail_match_1),
                &addcslashes(
                    &format!(
                        ",{}{}{}: {}{}}}",
                        self.newline,
                        self.indent,
                        JsonFile::encode(key),
                        content,
                        self.newline
                    ),
                    "\\$",
                ),
                &self.contents,
            );

            return Ok(true);
        }

        // append at the end of the file
        self.contents = Preg::replace(
            "#\\}$#",
            &addcslashes(
                &format!(
                    "{}{}: {}{}}}",
                    self.indent,
                    JsonFile::encode(key),
                    content,
                    self.newline
                ),
                "\\$",
            ),
            &self.contents,
        );

        Ok(true)
    }

    pub fn remove_main_key(&mut self, key: &str) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;
        let decoded_arr = decoded.as_array().cloned().unwrap_or_default();

        if !array_key_exists(key, &decoded_arr) {
            return Ok(true);
        }

        // key exists already
        let encoded_key = JsonFile::encode(key);
        let key_match = json_grammar::find_top_level_key(
            self.contents.as_bytes(),
            encoded_key.as_bytes(),
            ValueKind::Json,
        );
        if let Some(m) = key_match {
            // invalid match due to un-regexable content, abort
            let removal = &self.contents[m.key_pos..m.value_end];
            if json_decode(&format!("{{{}}}", removal), false)?.is_null() {
                return Ok(false);
            }

            // The pattern consumes `\s*,?\s*` between the removed key and the rest (`end`).
            let cb = self.contents.as_bytes();
            let mut e = json_grammar::skip_ws(cb, m.value_end);
            if cb.get(e) == Some(&b',') {
                e += 1;
            }
            e = json_grammar::skip_ws(cb, e);

            // check that we are not leaving a dangling comma on the previous line if the last line was removed
            let mut start = self.contents[..m.key_pos].to_string();
            let end = self.contents[e..].to_string();
            if Preg::is_match3("#,\\s*$#", &start, None) && Preg::is_match3("#^\\}$#", &end, None) {
                start = rtrim(
                    &Preg::replace("#,(\\s*)$#", "$1", &start),
                    Some(&self.indent),
                );
            }

            self.contents = format!("{}{}", start, end);
            if Preg::is_match3("#^\\{\\s*\\}\\s*$#", &self.contents, None) {
                self.contents = "{\n}".to_string();
            }

            return Ok(true);
        }

        Ok(false)
    }

    pub fn change_empty_main_key_from_assoc_to_list(&mut self, key: &str) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;
        let decoded_arr = decoded.as_array().cloned().unwrap_or_default();

        if !array_key_exists(key, &decoded_arr) {
            return Ok(true);
        }

        // Match the key only when its value is an empty object `{ <space> }`.
        let encoded_key = JsonFile::encode(key);
        let cb = self.contents.as_bytes();
        let key_match =
            json_grammar::find_top_level_key(cb, encoded_key.as_bytes(), ValueKind::Object)
                .filter(|m| json_grammar::skip_ws(cb, m.value_pos + 1) == m.value_end - 1);
        if let Some(m) = key_match {
            // invalid match due to un-regexable content, abort
            let removal = &self.contents[m.value_pos..m.value_end];
            if json_decode(removal, false)?.as_bool() == Some(false) {
                return Ok(false);
            }

            let removal_space = &self.contents[m.value_pos + 1..m.value_end - 1];
            self.contents = format!(
                "{}[{}]{}",
                &self.contents[..m.value_pos],
                removal_space,
                &self.contents[m.value_end..]
            );

            return Ok(true);
        }

        Ok(false)
    }

    pub fn remove_main_key_if_empty(&mut self, key: &str) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;
        let decoded_arr = decoded.as_array().cloned().unwrap_or_default();

        if !array_key_exists(key, &decoded_arr) {
            return Ok(true);
        }

        let value = decoded_arr.get(key).cloned().unwrap_or(PhpMixed::Null);
        // PHP `count($val)` applies to both associative and positional arrays.
        let count = value
            .as_array()
            .map(|a| a.len())
            .or_else(|| value.as_list().map(|l| l.len()))
            .unwrap_or(0);
        if is_array(&value) && count == 0 {
            return self.remove_main_key(key);
        }

        Ok(true)
    }

    pub fn format(&self, data: &PhpMixed, depth: i64, was_object: bool) -> anyhow::Result<String> {
        let mut data = data.clone();
        let mut was_object = was_object;
        if let Some(obj) = data.as_object() {
            // PHP: (array) $data — coerce to array
            data = PhpMixed::Array(obj.clone());
            was_object = true;
        }

        if is_array(&data) {
            if data.as_array().map(|a| a.len()).unwrap_or(0) == 0
                && data.as_list().map(|l| l.len()).unwrap_or(0) == 0
            {
                return Ok(if was_object {
                    format!(
                        "{{{}{}}}",
                        self.newline,
                        str_repeat(&self.indent, (depth + 1) as usize)
                    )
                } else {
                    "[]".to_string()
                });
            }

            if let Some(list) = data.as_list().cloned() {
                let mut formatted: Vec<String> = vec![];
                for val in &list {
                    formatted.push(self.format(val, depth + 1, false)?);
                }

                return Ok(format!("[{}]", implode(", ", &formatted)));
            }

            let out = format!("{{{}", self.newline);
            let mut elems: Vec<String> = vec![];
            if let Some(arr) = data.as_array() {
                for (key, val) in arr {
                    elems.push(format!(
                        "{}{}: {}",
                        str_repeat(&self.indent, (depth + 2) as usize),
                        JsonFile::encode(key),
                        self.format(val, depth + 1, false)?
                    ));
                }
            }

            return Ok(format!(
                "{}{}{}{}}}",
                out,
                implode(&format!(",{}", self.newline), &elems),
                self.newline,
                str_repeat(&self.indent, (depth + 1) as usize)
            ));
        }

        Ok(JsonFile::encode(&data))
    }

    pub(crate) fn detect_indenting(&mut self) {
        self.indent = JsonFile::detect_indenting(Some(&self.contents));
    }
}

// Reproduces the non-anchored PHP pattern `"name"\s*:\s*(?&json)` over `children`: returns every
// substring where the raw key token `"name"` (allowing an escaped slash `\/` wherever the name has
// `/`) is followed by `\s*:\s*` and a JSON value, in left-to-right, non-overlapping order.
fn find_key_value_matches(children: &str, name: &str) -> Vec<String> {
    let cb = children.as_bytes();
    let name_bytes = name.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cb.len() {
        if let Some(end) = match_key_value(cb, i, name_bytes) {
            out.push(children[i..end].to_string());
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

fn match_key_value(cb: &[u8], start: usize, name: &[u8]) -> Option<usize> {
    if cb.get(start) != Some(&b'"') {
        return None;
    }
    let mut p = start + 1;
    for &nc in name {
        if nc == b'/' {
            if cb.get(p) == Some(&b'\\') {
                p += 1;
            }
            if cb.get(p) != Some(&b'/') {
                return None;
            }
            p += 1;
        } else {
            if cb.get(p) != Some(&nc) {
                return None;
            }
            p += 1;
        }
    }
    if cb.get(p) != Some(&b'"') {
        return None;
    }
    p = json_grammar::skip_ws(cb, p + 1);
    if cb.get(p) != Some(&b':') {
        return None;
    }
    p = json_grammar::skip_ws(cb, p + 1);
    json_grammar::scan_value(cb, p)
}

// Reproduces the PHP package-link pattern `"package"\s*:\s*(?&string)` (case-insensitive, with an
// optional escaped slash where the name has `/`) over the `links` object text. Returns the byte
// offsets `(key_quote_start, key_end, value_start, value_end)` of the first match, where `key_end`
// is just past the key's closing quote and the value is a JSON string.
fn find_package_link(links: &str, package: &str) -> Option<(usize, usize, usize, usize)> {
    let cb = links.as_bytes();
    let name = package.as_bytes();
    for i in 0..cb.len() {
        if cb[i] != b'"' {
            continue;
        }
        let Some(key_end) = match_pkg_name(cb, i, name) else {
            continue;
        };
        let mut p = json_grammar::skip_ws(cb, key_end);
        if cb.get(p) != Some(&b':') {
            continue;
        }
        p = json_grammar::skip_ws(cb, p + 1);
        if cb.get(p) != Some(&b'"') {
            continue;
        }
        let Some(value_end) = json_grammar::scan_string(cb, p) else {
            continue;
        };
        return Some((i, key_end, p, value_end));
    }
    None
}

// Matches a quoted package key (case-insensitive; a `/` in `name` matches `/` or `\/`) starting at
// `b[start] == '"'`. Returns the offset just past the closing quote.
fn match_pkg_name(b: &[u8], start: usize, name: &[u8]) -> Option<usize> {
    let mut p = start + 1;
    for &nc in name {
        if nc == b'/' {
            if b.get(p) == Some(&b'\\') {
                p += 1;
            }
            if b.get(p) != Some(&b'/') {
                return None;
            }
            p += 1;
        } else {
            match b.get(p) {
                Some(c) if c.eq_ignore_ascii_case(&nc) => p += 1,
                _ => return None,
            }
        }
    }
    if b.get(p) != Some(&b'"') {
        return None;
    }
    Some(p + 1)
}

// Lightweight clone of JsonManipulator's formatting logic, used inside Preg::replace_callback closures.
struct ManipulatorFormatter {
    newline: String,
    indent: String,
}

impl ManipulatorFormatter {
    fn format(&self, data: &PhpMixed, depth: i64, was_object: bool) -> anyhow::Result<String> {
        let mut data = data.clone();
        let mut was_object = was_object;
        if let Some(obj) = data.as_object() {
            data = PhpMixed::Array(obj.clone());
            was_object = true;
        }

        if is_array(&data) {
            if data.as_array().map(|a| a.len()).unwrap_or(0) == 0
                && data.as_list().map(|l| l.len()).unwrap_or(0) == 0
            {
                return Ok(if was_object {
                    format!(
                        "{{{}{}}}",
                        self.newline,
                        str_repeat(&self.indent, (depth + 1) as usize)
                    )
                } else {
                    "[]".to_string()
                });
            }

            if let Some(list) = data.as_list().cloned() {
                let mut formatted: Vec<String> = vec![];
                for val in &list {
                    formatted.push(self.format(val, depth + 1, false)?);
                }

                return Ok(format!("[{}]", implode(", ", &formatted)));
            }

            let out = format!("{{{}", self.newline);
            let mut elems: Vec<String> = vec![];
            if let Some(arr) = data.as_array() {
                for (key, val) in arr {
                    elems.push(format!(
                        "{}{}: {}",
                        str_repeat(&self.indent, (depth + 2) as usize),
                        JsonFile::encode(key),
                        self.format(val, depth + 1, false)?
                    ));
                }
            }

            return Ok(format!(
                "{}{}{}{}}}",
                out,
                implode(&format!(",{}", self.newline), &elems),
                self.newline,
                str_repeat(&self.indent, (depth + 1) as usize)
            ));
        }

        Ok(JsonFile::encode(&data))
    }
}
