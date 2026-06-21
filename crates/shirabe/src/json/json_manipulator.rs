//! ref: composer/src/Composer/Json/JsonManipulator.php

use indexmap::IndexMap;

use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{
    InvalidArgumentException, LogicException, PhpMixed, addcslashes, array_key_exists, array_keys,
    array_reverse, count, empty, explode, implode, in_array, is_array, is_int, is_numeric,
    json_decode, preg_quote, rtrim, str_contains, str_repeat, str_replace, strlen, strnatcmp,
    strpos, substr, trim, uksort,
};

use crate::json::JsonFile;
use crate::repository::PlatformRepository;

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

        let regex = format!(
            "{{{}^(?P<start>\\s*\\{{\\s*(?:(?&string)\\s*:\\s*(?&json)\\s*,\\s*)*?)(?P<property>{}\\s*:\\s*)(?P<value>(?&json))(?P<end>.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(r#type), None),
        );
        let mut matches: IndexMap<String, String> = IndexMap::new();
        if !Preg::is_match_named(&regex, &self.contents, &mut matches) {
            return Ok(false);
        }
        let start = matches.get("start").cloned().unwrap_or_default();
        let property = matches.get("property").cloned().unwrap_or_default();
        let end = matches.get("end").cloned().unwrap_or_default();

        let mut links = matches.get("value").cloned().unwrap_or_default();

        // try to find existing link
        let package_regex = str_replace("/", "\\\\?/", &preg_quote(package, None));
        let regex = format!(
            "{{{}\"(?P<package>{})\"(\\s*:\\s*)(?&string)}}ix",
            Self::DEFINES,
            package_regex
        );
        let mut package_matches: IndexMap<String, String> = IndexMap::new();
        if Preg::is_match_named(&regex, &links, &mut package_matches) {
            // update existing link
            let existing_package = package_matches.get("package").cloned().unwrap_or_default();
            let package_regex = str_replace("/", "\\\\?/", &preg_quote(&existing_package, None));
            let constraint_owned = constraint.to_string();
            let existing_owned = existing_package.clone();
            links = Preg::replace_callback(
                &format!(
                    "{{{}\"{}\"(?P<separator>\\s*:\\s*)(?&string)}}ix",
                    Self::DEFINES,
                    package_regex
                ),
                move |m: &IndexMap<CaptureKey, String>| -> String {
                    format!(
                        "{}{}\"{}\"",
                        JsonFile::encode(&str_replace("\\/", "/", &existing_owned)),
                        m.get(&CaptureKey::ByName("separator".to_string()))
                            .cloned()
                            .unwrap_or_default(),
                        constraint_owned
                    )
                },
                &links,
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
                    let repo: IndexMap<String, PhpMixed> =
                        repository.as_array().cloned().unwrap_or_default();
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

        let repos = decoded
            .as_array()
            .and_then(|a| a.get("repositories"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for (index, repository) in &repos {
            if name == index.as_str() {
                repository_index = Some(PhpMixed::String(index.clone()));
                break;
            }

            let repo_name = repository
                .as_array()
                .and_then(|a| a.get("name"))
                .and_then(|v| v.as_string());
            if Some(name) == repo_name {
                repository_index = Some(PhpMixed::String(index.clone()));
                break;
            }
        }

        let repository_index = match repository_index {
            Some(r) => r,
            None => return Ok(false),
        };

        let mut list_regex: Option<String> = None;

        if is_int(&repository_index) {
            let i_val = repository_index.as_int().unwrap_or(0);
            list_regex = Some(format!(
                "{{{}^(?P<start>\\s*\\{{\\s*(?:(?&string)\\s*:\\s*(?&json)\\s*,\\s*)*?\"repositories\"\\s*:\\s*\\[\\s*((?&json)\\s*+,\\s*+){{{}}})(?P<repository>(?&object))(?P<end>.*)}}sx",
                Self::DEFINES,
                i_val.max(0)
            ));
        }

        let object_regex = format!(
            "{{{}^(?P<start>\\s*\\{{\\s*(?:(?&string)\\s*:\\s*(?&json)\\s*,\\s*)*?\"repositories\"\\s*:\\s*\\{{\\s*(?:(?&string)\\s*:\\s*(?&json)\\s*,\\s*)*?{}\\s*:\\s*)(?P<repository>(?&object))(?P<end>.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(&repository_index), None)
        );
        let mut matches: IndexMap<String, String> = IndexMap::new();

        let list_match = list_regex
            .as_ref()
            .is_some_and(|r| Preg::is_match_named(r, &self.contents, &mut matches));
        if list_match || Preg::is_match_named(&object_regex, &self.contents, &mut matches) {
            // invalid match due to un-regexable content, abort
            let raw_repo = matches.get("repository").cloned().unwrap_or_default();
            if json_decode(&raw_repo, false)?.as_bool() == Some(false) {
                return Ok(false);
            }

            let repository_regex = format!(
                "{{{}^(?P<start>\\s*\\{{\\s*(?:(?&string)\\s*:\\s*(?&json)\\s*,\\s*)*?\"url\"\\s*:\\s*)(?P<url>(?&string))(?P<end>.*)}}sx",
                Self::DEFINES
            );

            let url_owned = url.to_string();
            self.contents = format!(
                "{}{}{}",
                matches.get("start").cloned().unwrap_or_default(),
                Preg::replace_callback(
                    &repository_regex,
                    move |repository_matches: &IndexMap<CaptureKey, String>| -> String {
                        format!(
                            "{}{}{}",
                            repository_matches
                                .get(&CaptureKey::ByName("start".to_string()))
                                .cloned()
                                .unwrap_or_default(),
                            JsonFile::encode(&url_owned),
                            repository_matches
                                .get(&CaptureKey::ByName("end".to_string()))
                                .cloned()
                                .unwrap_or_default()
                        )
                    },
                    &raw_repo,
                ),
                matches.get("end").cloned().unwrap_or_default()
            );

            return Ok(true);
        }

        Ok(false)
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

        let repos: IndexMap<String, PhpMixed> = repositories_value
            .as_ref()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();

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
                let repository_as_array: IndexMap<String, PhpMixed> =
                    repository.as_array().cloned().unwrap_or_default();

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
        let node_regex = format!(
            "{{{}^(?P<start> \\s* \\{{ \\s* (?: (?&string) \\s* : (?&json) \\s* , \\s* )*?{}\\s*:\\s*)(?P<content>(?&object))(?P<end>.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(main_node), None)
        );

        let mut match_map: IndexMap<String, String> = IndexMap::new();
        if !Preg::is_match_named(&node_regex, &self.contents, &mut match_map) {
            return Ok(false);
        }

        let mut children = match_map.get("content").cloned().unwrap_or_default();
        // invalid match due to un-regexable content, abort
        if json_decode(&children, false)?.is_null()
            || json_decode(&children, false)?.as_bool() == Some(false)
        {
            return Ok(false);
        }

        // child exists
        let child_regex = format!(
            "{{{}(?P<start>\"{}\"\\s*:\\s*)(?P<content>(?&json))(?P<end>,?)}}x",
            Self::DEFINES,
            preg_quote(&name_owned, None)
        );
        let mut child_match_map: IndexMap<String, String> = IndexMap::new();
        if Preg::is_match_named(&child_regex, &children, &mut child_match_map) {
            let value_capture = value.clone();
            let sub_name_capture = sub_name.clone();
            let formatter = ManipulatorFormatter {
                newline: self.newline.clone(),
                indent: self.indent.clone(),
            };
            children = Preg::replace_callback(
                &child_regex,
                move |matches: &IndexMap<CaptureKey, String>| -> String {
                    let content_key = CaptureKey::ByName("content".to_string());
                    let start_key = CaptureKey::ByName("start".to_string());
                    let end_key = CaptureKey::ByName("end".to_string());
                    let mut value_local = value_capture.clone();
                    if sub_name_capture.is_some() && matches.get(&content_key).is_some() {
                        let mut cur_val = json_decode(matches.get(&content_key).unwrap(), true)
                            .unwrap_or(PhpMixed::Null);
                        if !is_array(&cur_val) {
                            cur_val = PhpMixed::Array(IndexMap::new());
                        }
                        if let Some(arr) = cur_val.as_array_mut() {
                            arr.insert(sub_name_capture.clone().unwrap(), value_local.clone());
                        }
                        value_local = cur_val;
                    }

                    format!(
                        "{}{}{}",
                        matches.get(&start_key).cloned().unwrap_or_default(),
                        formatter.format(&value_local, 1, false).unwrap_or_default(),
                        matches.get(&end_key).cloned().unwrap_or_default()
                    )
                },
                &children,
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

        let children_owned = children;
        self.contents = Preg::replace_callback(
            &node_regex,
            move |m: &IndexMap<CaptureKey, String>| -> String {
                format!(
                    "{}{}{}",
                    m.get(&CaptureKey::ByName("start".to_string()))
                        .cloned()
                        .unwrap_or_default(),
                    children_owned,
                    m.get(&CaptureKey::ByName("end".to_string()))
                        .cloned()
                        .unwrap_or_default()
                )
            },
            &self.contents,
        );

        Ok(true)
    }

    pub fn remove_sub_node(&mut self, main_node: &str, name: &str) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;

        // no node or empty node
        let main_node_value = decoded.as_array().and_then(|a| a.get(main_node));
        if main_node_value.map(|v| empty(v)).unwrap_or(true) {
            return Ok(true);
        }

        // no node content match-able
        let node_regex = format!(
            "{{{}^(?P<start> \\s* \\{{ \\s* (?: (?&string) \\s* : (?&json) \\s* , \\s* )*?{}\\s*:\\s*)(?P<content>(?&object))(?P<end>.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(main_node), None)
        );
        let mut match_map: IndexMap<String, String> = IndexMap::new();
        if !Preg::is_match_named(&node_regex, &self.contents, &mut match_map) {
            return Ok(false);
        }

        let children = match_map.get("content").cloned().unwrap_or_default();

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
            // find best match for the value of "name"
            let mut all_matches: IndexMap<CaptureKey, Vec<String>> = IndexMap::new();
            if Preg::is_match_all3(
                &format!(
                    "{{{}\"{}\"\\s*:\\s*(?:(?&json))}}x",
                    Self::DEFINES,
                    key_regex
                ),
                &children,
                Some(&mut all_matches),
            ) {
                let mut best_match: String = String::new();
                let first_group = all_matches
                    .get(&CaptureKey::ByIndex(0))
                    .cloned()
                    .unwrap_or_default();
                for m in &first_group {
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
            let newline = self.newline.clone();
            let indent = self.indent.clone();

            self.contents = Preg::replace_callback(
                &node_regex,
                move |matches: &IndexMap<CaptureKey, String>| -> String {
                    format!(
                        "{}{{{}{}}}{}",
                        matches
                            .get(&CaptureKey::ByName("start".to_string()))
                            .cloned()
                            .unwrap_or_default(),
                        newline,
                        indent,
                        matches
                            .get(&CaptureKey::ByName("end".to_string()))
                            .cloned()
                            .unwrap_or_default()
                    )
                },
                &self.contents,
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

        let name_capture = name_owned.clone();
        let sub_name_capture = sub_name.clone();
        let children_clean_capture = children_clean.clone();
        let formatter = ManipulatorFormatter {
            newline: self.newline.clone(),
            indent: self.indent.clone(),
        };
        self.contents = Preg::replace_callback(
            &node_regex,
            move |matches: &IndexMap<CaptureKey, String>| -> String {
                let content_key = CaptureKey::ByName("content".to_string());
                let start_key = CaptureKey::ByName("start".to_string());
                let end_key = CaptureKey::ByName("end".to_string());
                let mut children_clean = children_clean_capture.clone();
                if let Some(ref sub) = sub_name_capture {
                    let mut cur_val = json_decode(
                        matches.get(&content_key).map(|s| s.as_str()).unwrap_or(""),
                        true,
                    )
                    .unwrap_or(PhpMixed::Null);
                    if let Some(arr) = cur_val.as_array_mut() {
                        if let Some(inner) =
                            arr.get_mut(&name_capture).and_then(|v| v.as_array_mut())
                        {
                            inner.shift_remove(sub);
                        }
                        let now_empty = arr
                            .get(&name_capture)
                            .and_then(|v| v.as_array())
                            .map(|a| a.is_empty())
                            .unwrap_or(false);
                        if now_empty {
                            arr.insert(name_capture.clone(), PhpMixed::Object(IndexMap::new()));
                        }
                    }
                    children_clean = formatter.format(&cur_val, 0, true).unwrap_or_default();
                }

                format!(
                    "{}{}{}",
                    matches.get(&start_key).cloned().unwrap_or_default(),
                    children_clean,
                    matches.get(&end_key).cloned().unwrap_or_default()
                )
            },
            &self.contents,
        );

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
        let node_regex = format!(
            "{{{}^(?P<start> \\s* \\{{ \\s* (?: (?&string) \\s* : (?&json) \\s* , \\s* )*?{}\\s*:\\s*)(?P<content>(?&array))(?P<end>.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(main_node), None)
        );

        let mut match_map: IndexMap<String, String> = IndexMap::new();
        if !Preg::is_match_named(&node_regex, &self.contents, &mut match_map) {
            return Ok(false);
        }

        let mut children = match_map.get("content").cloned().unwrap_or_default();
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

        let children_owned = children;
        self.contents = Preg::replace_callback(
            &node_regex,
            move |m: &IndexMap<CaptureKey, String>| -> String {
                format!(
                    "{}{}{}",
                    m.get(&CaptureKey::ByName("start".to_string()))
                        .cloned()
                        .unwrap_or_default(),
                    children_owned,
                    m.get(&CaptureKey::ByName("end".to_string()))
                        .cloned()
                        .unwrap_or_default()
                )
            },
            &self.contents,
        );

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
        let node_regex = format!(
            "{{{}^(?P<start> \\s* \\{{ \\s* (?: (?&string) \\s* : (?&json) \\s* , \\s* )*?{}\\s*:\\s*)(?P<content>(?&array))(?P<end>.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(main_node), None)
        );

        let mut match_map: IndexMap<String, String> = IndexMap::new();
        if !Preg::is_match_named(&node_regex, &self.contents, &mut match_map) {
            return Ok(false);
        }

        let mut children = match_map.get("content").cloned().unwrap_or_default();
        // invalid match due to un-regexable content, abort
        if json_decode(&children, false)?.as_bool() == Some(false) {
            return Ok(false);
        }

        let list_skip_to_item_regex = format!(
            "{{{}^(?P<start>\\[\\s*((?&json)\\s*+,\\s*?){{{}}})(?P<space_before_item>(\\s*))(?P<end>.*)}}sx",
            Self::DEFINES,
            index.max(0)
        );

        let value_capture = value.clone();
        let formatter = ManipulatorFormatter {
            newline: self.newline.clone(),
            indent: self.indent.clone(),
        };
        children = Preg::replace_callback(
            &list_skip_to_item_regex,
            move |m: &IndexMap<CaptureKey, String>| -> String {
                format!(
                    "{}{}{},{}{}",
                    m.get(&CaptureKey::ByName("start".to_string()))
                        .cloned()
                        .unwrap_or_default(),
                    m.get(&CaptureKey::ByName("space_before_item".to_string()))
                        .cloned()
                        .unwrap_or_default(),
                    formatter
                        .format(&value_capture, 1, false)
                        .unwrap_or_default(),
                    m.get(&CaptureKey::ByName("space_before_item".to_string()))
                        .cloned()
                        .unwrap_or_default(),
                    m.get(&CaptureKey::ByName("end".to_string()))
                        .cloned()
                        .unwrap_or_default()
                )
            },
            &children,
        );

        let children_owned = children;
        self.contents = Preg::replace_callback(
            &node_regex,
            move |m: &IndexMap<CaptureKey, String>| -> String {
                format!(
                    "{}{}{}",
                    m.get(&CaptureKey::ByName("start".to_string()))
                        .cloned()
                        .unwrap_or_default(),
                    children_owned,
                    m.get(&CaptureKey::ByName("end".to_string()))
                        .cloned()
                        .unwrap_or_default()
                )
            },
            &self.contents,
        );

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
        if main_node_value.map(|v| empty(v)).unwrap_or(true) {
            return Ok(true);
        }

        // no node content match-able
        let node_regex = format!(
            "{{{}^(?P<start> \\s* \\{{ \\s* (?: (?&string) \\s* : (?&json) \\s* , \\s* )*?{}\\s*:\\s*)(?P<content>(?&array))(?P<end>.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(main_node), None)
        );
        let mut match_map: IndexMap<String, String> = IndexMap::new();
        if !Preg::is_match_named(&node_regex, &self.contents, &mut match_map) {
            return Ok(false);
        }

        let children = match_map.get("content").cloned().unwrap_or_default();

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

        let mut content_regex = "(?&json)".to_string();
        let start_regex: String;
        let end_regex: String;

        if node_index > 1 {
            start_regex = format!("(?&json)\\s*+(?:,(?&json)\\s*+){{{}}}", node_index - 1);
            // remove leading array separator in case we might remove the last
            content_regex = format!("\\s*+,?\\s*+{}", content_regex);
            end_regex = "(?:(\\s*+,\\s*+(?&json))*(?:\\s*+(?&json))?)\\s*+".to_string();
        } else if node_index > 0 {
            start_regex = "(?&json)\\s*+".to_string();
            // remove leading array separator in case we might remove the last
            content_regex = format!("\\s*+,?\\s*+{}", content_regex);
            end_regex = "(?:(\\s*+,\\s*+(?&json))*(?:\\s*+(?&json))?)\\s*+".to_string();
        } else {
            start_regex = "\\s*+".to_string();
            // remove trailing array separator when we delete first
            content_regex = format!("{}\\s*+,?\\s*+", content_regex);
            end_regex = "(?:((?&json)\\s*+,\\s*+)*(?:\\s*+(?&json))?)\\s*+".to_string();
        }

        let mut child_match: IndexMap<String, String> = IndexMap::new();
        if Preg::is_match_named(
            &format!(
                "{{{}(?P<start>\\[{})(?P<content>{})(?P<end>{}\\])}}sx",
                Self::DEFINES,
                start_regex,
                content_regex,
                end_regex
            ),
            &children,
            &mut child_match,
        ) {
            self.contents = format!(
                "{}{}{}{}",
                match_map.get("start").cloned().unwrap_or_default(),
                child_match.get("start").cloned().unwrap_or_default(),
                child_match.get("end").cloned().unwrap_or_default(),
                match_map.get("end").cloned().unwrap_or_default()
            );

            return Ok(true);
        }

        Ok(false)
    }

    pub fn add_main_key(&mut self, key: &str, content: PhpMixed) -> anyhow::Result<bool> {
        let decoded = JsonFile::parse_json(Some(&self.contents), Some("composer.json"))?;
        let content = self.format(&content, 0, false)?;

        // key exists already
        let regex = format!(
            "{{{}^(?P<start>\\s*\\{{\\s*(?:(?&string)\\s*:\\s*(?&json)\\s*,\\s*)*?)(?P<key>{}\\s*:\\s*(?&json))(?P<end>.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(key), None)
        );
        let mut matches: IndexMap<String, String> = IndexMap::new();
        if decoded.as_array().and_then(|a| a.get(key)).is_some()
            && Preg::is_match_named(&regex, &self.contents, &mut matches)
        {
            // invalid match due to un-regexable content, abort
            let key_match = matches.get("key").cloned().unwrap_or_default();
            if json_decode(&format!("{{{}}}", key_match), false)?.is_null() {
                return Ok(false);
            }

            self.contents = format!(
                "{}{}: {}{}",
                matches.get("start").cloned().unwrap_or_default(),
                JsonFile::encode(key),
                content,
                matches.get("end").cloned().unwrap_or_default()
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
        let regex = format!(
            "{{{}^(?P<start>\\s*\\{{\\s*(?:(?&string)\\s*:\\s*(?&json)\\s*,\\s*)*?)(?P<removal>{}\\s*:\\s*(?&json))\\s*,?\\s*(?P<end>.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(key), None)
        );
        let mut matches: IndexMap<String, String> = IndexMap::new();
        if Preg::is_match_named(&regex, &self.contents, &mut matches) {
            // invalid match due to un-regexable content, abort
            let removal = matches.get("removal").cloned().unwrap_or_default();
            if json_decode(&format!("{{{}}}", removal), false)?.is_null() {
                return Ok(false);
            }

            // check that we are not leaving a dangling comma on the previous line if the last line was removed
            let mut start = matches.get("start").cloned().unwrap_or_default();
            let end = matches.get("end").cloned().unwrap_or_default();
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

        let regex = format!(
            "{{{}^(?P<start>\\s*\\{{\\s*(?:(?&string)\\s*:\\s*(?&json)\\s*,\\s*)*?{}\\s*:\\s*)(?P<removal>\\{{(?P<removal_space>\\s*+)\\}})(?P<end>\\s*,?\\s*.*)}}sx",
            Self::DEFINES,
            preg_quote(&JsonFile::encode(key), None)
        );
        let mut matches: IndexMap<String, String> = IndexMap::new();
        if Preg::is_match_named(&regex, &self.contents, &mut matches) {
            // invalid match due to un-regexable content, abort
            let removal = matches.get("removal").cloned().unwrap_or_default();
            if json_decode(&removal, false)?.as_bool() == Some(false) {
                return Ok(false);
            }

            self.contents = format!(
                "{}[{}]{}",
                matches.get("start").cloned().unwrap_or_default(),
                matches.get("removal_space").cloned().unwrap_or_default(),
                matches.get("end").cloned().unwrap_or_default()
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
        if is_array(&value) && value.as_array().map(|a| a.len()).unwrap_or(0) == 0 {
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
