//! ref: composer/src/Composer/Platform/Runtime.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_php_shim::{
    PhpMixed, class_exists, constant, defined, function_exists, get_loaded_extensions,
    html_entity_decode, implode, instantiate_class, ltrim, phpversion, strip_tags, trim,
};

/// Seam over the PHP runtime so PlatformRepository can be tested against mocked
/// extension/constant/function probes. PHP has no such interface (the test mocks the
/// concrete `Composer\Platform\Runtime` directly); it is introduced here to keep the
/// consumer dependent only on trait methods.
pub trait RuntimeInterface: std::fmt::Debug {
    fn has_constant(&self, constant_name: &str, class: Option<String>) -> bool;
    fn get_constant(&self, constant_name: &str, class: Option<String>) -> PhpMixed;
    /// `callable` carries the PHP callable spec (a function name string or a
    /// `[class, method]` list), matching PHP `invoke($callable, $arguments)`.
    fn invoke(&self, callable: PhpMixed, arguments: Vec<PhpMixed>) -> PhpMixed;
    fn has_class(&self, class: &str) -> bool;
    fn construct(&self, class: &str, arguments: Vec<PhpMixed>) -> Result<PhpMixed>;
    fn get_extensions(&self) -> Vec<String>;
    fn get_extension_version(&self, extension: &str) -> String;
    fn get_extension_info(&self, extension: &str) -> Result<String>;
}

#[derive(Debug)]
pub struct Runtime;

impl RuntimeInterface for Runtime {
    fn has_constant(&self, constant_name: &str, class: Option<String>) -> bool {
        defined(&ltrim(
            &format!("{}::{}", class.as_deref().unwrap_or(""), constant_name),
            Some(":"),
        ))
    }

    fn get_constant(&self, constant_name: &str, class: Option<String>) -> PhpMixed {
        constant(&ltrim(
            &format!("{}::{}", class.as_deref().unwrap_or(""), constant_name),
            Some(":"),
        ))
    }

    fn invoke(&self, callable: PhpMixed, arguments: Vec<PhpMixed>) -> PhpMixed {
        // PHP: return $callable(...$arguments);
        // Dispatching an arbitrary PHP callable needs a PHP runtime; no shim exists.
        let _ = (callable, arguments);
        todo!()
    }

    fn has_class(&self, class: &str) -> bool {
        class_exists(class)
    }

    fn construct(&self, class: &str, arguments: Vec<PhpMixed>) -> Result<PhpMixed> {
        if arguments.is_empty() {
            Ok(instantiate_class(class, vec![]))
        } else {
            Ok(instantiate_class(class, arguments))
        }
    }

    fn get_extensions(&self) -> Vec<String> {
        get_loaded_extensions()
    }

    fn get_extension_version(&self, extension: &str) -> String {
        let version = phpversion(extension);
        version.unwrap_or_else(|| "0".to_string())
    }

    fn get_extension_info(&self, extension: &str) -> Result<String> {
        // Depends on \ReflectionExtension::info() and output buffering; no shim equivalent exists.
        let _ = extension;
        todo!()
    }
}

impl Runtime {
    pub fn has_function(&self, f: &str) -> bool {
        function_exists(f)
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
