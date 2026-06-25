//! ref: composer/src/Composer/Platform/Runtime.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{
    PhpMixed, class_exists, constant, defined, function_exists, get_loaded_extensions,
    html_entity_decode, implode, instantiate_class, ltrim, phpversion, strip_tags, trim,
};

#[derive(Debug)]
pub struct Runtime;

impl Runtime {
    pub fn has_constant(&self, constant_name: &str, class: Option<&str>) -> bool {
        defined(&ltrim(
            &format!("{}::{}", class.unwrap_or(""), constant_name),
            Some(":"),
        ))
    }

    pub fn get_constant(&self, constant_name: &str, class: Option<&str>) -> PhpMixed {
        constant(&ltrim(
            &format!("{}::{}", class.unwrap_or(""), constant_name),
            Some(":"),
        ))
    }

    pub fn has_function(&self, f: &str) -> bool {
        function_exists(f)
    }

    pub fn invoke(
        &self,
        callable: Box<dyn Fn(Vec<PhpMixed>) -> PhpMixed>,
        arguments: Vec<PhpMixed>,
    ) -> PhpMixed {
        callable(arguments)
    }

    pub fn has_class(&self, class: &str) -> bool {
        class_exists(class)
    }

    pub fn construct(&self, class: &str, arguments: Vec<PhpMixed>) -> Result<PhpMixed> {
        if arguments.is_empty() {
            Ok(instantiate_class(class, vec![]))
        } else {
            Ok(instantiate_class(class, arguments))
        }
    }

    pub fn get_extensions(&self) -> Vec<String> {
        get_loaded_extensions()
    }

    pub fn get_extension_version(&self, extension: &str) -> String {
        let version = phpversion(extension);
        version.unwrap_or_else(|| "0".to_string())
    }

    pub fn get_extension_info(&self, extension: &str) -> Result<String> {
        // Depends on \ReflectionExtension::info() and output buffering; no shim equivalent exists.
        let _ = extension;
        todo!()
    }

    pub fn parse_html_extension_info(html: &str) -> String {
        let mut result: Vec<String> = vec![];

        let mut matches: IndexMap<CaptureKey, String> = IndexMap::new();
        if Preg::match3(
            r"~<h2>\s*<a[^>]*>([^<]+)</a>\s*</h2>~i",
            html,
            Some(&mut matches),
        ) {
            result.push(trim(
                &html_entity_decode(
                    matches
                        .get(&CaptureKey::ByIndex(1))
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                ),
                None,
            ));
            result.push(String::new());
        }

        let mut matches: IndexMap<CaptureKey, Vec<String>> = IndexMap::new();
        if Preg::match_all3(
            r#"~<tr>\s*<td class="e">\s*(.*?)\s*</td>\s*<td class="v">\s*(.*?)\s*</td>\s*</tr>~is"#,
            html,
            Some(&mut matches),
        ) > 0
        {
            let group1 = matches
                .get(&CaptureKey::ByIndex(1))
                .cloned()
                .unwrap_or_default();
            let group2 = matches
                .get(&CaptureKey::ByIndex(2))
                .cloned()
                .unwrap_or_default();
            let count = std::cmp::min(group1.len(), group2.len());

            for i in 0..count {
                let key = trim(&html_entity_decode(&strip_tags(&group1[i])), None);
                let value = trim(&html_entity_decode(&strip_tags(&group2[i])), None);
                result.push(format!("{} => {}", key, value));
            }
        }

        implode("\n", &result)
    }
}
