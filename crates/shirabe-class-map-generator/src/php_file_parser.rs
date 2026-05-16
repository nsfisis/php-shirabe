//! ref: composer/vendor/composer/class-map-generator/src/PhpFileParser.php

use std::sync::OnceLock;
use anyhow::anyhow;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PHP_EOL, PHP_VERSION_ID, HHVM_VERSION,
    RuntimeException,
    error_get_last, file_exists, file_get_contents, function_exists,
    is_file, is_readable, ltrim, php_strip_whitespace, sprintf,
    str_replace_array, strrpos, substr, trim, version_compare,
};
use shirabe_external_packages::composer::pcre::preg::Preg;
use crate::php_file_cleaner::PhpFileCleaner;

pub struct PhpFileParser;

impl PhpFileParser {
    pub fn find_classes(path: &str) -> anyhow::Result<Vec<String>> {
        let extra_types = Self::get_extra_types();

        if !function_exists("php_strip_whitespace") {
            return Err(anyhow!(RuntimeException {
                message: "Classmap generation relies on the php_strip_whitespace function, but it has been disabled by the disable_functions directive.".to_string(),
                code: 0,
            }));
        }

        // Use @ here instead of Silencer to actively suppress 'unhelpful' output
        let contents = php_strip_whitespace(path);
        if contents.is_empty() {
            let message: &str;
            if !file_exists(path) {
                message = "File at \"%s\" does not exist, check your classmap definitions";
            } else if !Self::is_readable(path) {
                message = "File at \"%s\" is not readable, check its permissions";
            } else if trim(file_get_contents(path).unwrap_or_default().as_str(), None).is_empty() {
                // The input file was really empty and thus contains no classes
                return Ok(vec![]);
            } else {
                message = "File at \"%s\" could not be parsed as PHP, it may be binary or corrupted";
            }

            let error = error_get_last();
            let mut message = sprintf(message, &[shirabe_php_shim::PhpMixed::String(path.to_string())]);
            if let Some(error) = error {
                if let Some(err_msg) = error.get("message") {
                    message = format!(
                        "{}{}{}{}{}",
                        message,
                        PHP_EOL,
                        "The following message may be helpful:",
                        PHP_EOL,
                        err_msg.as_string().unwrap_or("")
                    );
                }
            }

            return Err(anyhow!(RuntimeException { message, code: 0 }));
        }

        // return early if there is no chance of matching anything in this file
        let pattern = format!("{{\\b(?:class|interface|trait{})\\s}}i", extra_types);
        let matches_0 = Preg::is_match_all_strict_groups(&pattern, &contents);
        if matches_0.as_ref().map(|m| m[0].is_empty()).unwrap_or(true) {
            return Ok(vec![]);
        }
        let max_matches = matches_0.as_ref().unwrap()[0].len();

        let mut p = PhpFileCleaner::new(contents, max_matches);
        let contents = p.clean();
        drop(p);

        let pattern2 = format!(
            r"(?ix)
            (?:
                 \b(?<![\\$:>])(?P<type>class|interface|trait{et}) \s++ (?P<name>[a-zA-Z_\x7f-\xff:][a-zA-Z0-9_\x7f-\xff:\-]*+)
               | \b(?<![\\$:>])(?P<ns>namespace) (?P<nsname>\s++[a-zA-Z_\x7f-\xff][a-zA-Z0-9_\x7f-\xff]*+(?:\s*+\\\\\s*+[a-zA-Z_\x7f-\xff][a-zA-Z0-9_\x7f-\xff]*+)*+)? \s*+ [\{{;]
            )",
            et = extra_types
        );
        let mut matches: IndexMap<String, Vec<String>> = IndexMap::new();
        Preg::match_all(&pattern2, &contents, &mut matches);

        let mut classes = vec![];
        let mut namespace = String::new();

        let len = matches.get("type").map(|v| v.len()).unwrap_or(0);
        for i in 0..len {
            let ns = matches.get("ns").and_then(|v| v.get(i)).map(|s| s.as_str()).unwrap_or("");
            if !ns.is_empty() {
                let nsname = matches.get("nsname").and_then(|v| v.get(i)).map(|s| s.as_str()).unwrap_or("");
                namespace = str_replace_array(
                    &[" ".to_string(), "\t".to_string(), "\r".to_string(), "\n".to_string()],
                    &["".to_string()],
                    nsname,
                ) + "\\";
            } else {
                let name = matches.get("name").and_then(|v| v.get(i)).map(|s| s.as_str()).unwrap_or("");
                // skip anon classes extending/implementing
                if name == "extends" {
                    continue;
                }
                if name == "implements" {
                    continue;
                }

                let name: String = if name.starts_with(':') {
                    // This is an XHP class, https://github.com/facebook/xhp
                    "xhp".to_string()
                        + &str_replace_array(
                            &["-".to_string(), ":".to_string()],
                            &["_".to_string(), "__".to_string()],
                            &name[1..],
                        )
                } else if matches
                    .get("type")
                    .and_then(|v| v.get(i))
                    .map(|s| s.to_lowercase())
                    .as_deref()
                    == Some("enum")
                {
                    // something like:
                    //   enum Foo: int { HERP = '123'; }
                    // The regex above captures the colon, which isn't part of
                    // the class name.
                    // or:
                    //   enum Foo:int { HERP = '123'; }
                    // The regex above captures the colon and type, which isn't part of
                    // the class name.
                    if let Some(colon_pos) = strrpos(name, ":") {
                        substr(name, 0, Some(colon_pos as i64))
                    } else {
                        name.to_string()
                    }
                } else {
                    name.to_string()
                };

                let class_name = ltrim(&format!("{}{}", namespace, name), Some("\\"));
                classes.push(class_name);
            }
        }

        Ok(classes)
    }

    fn get_extra_types() -> &'static str {
        static EXTRA_TYPES: OnceLock<String> = OnceLock::new();
        EXTRA_TYPES.get_or_init(|| {
            let mut extra_types = String::new();
            let mut extra_types_array: Vec<String> = vec![];
            if PHP_VERSION_ID >= 80100
                || (HHVM_VERSION.is_some()
                    && version_compare(HHVM_VERSION.unwrap(), "3.3", ">="))
            {
                extra_types += "|enum";
                extra_types_array = vec!["enum".to_string()];
            }

            let mut type_config = vec![
                "class".to_string(),
                "interface".to_string(),
                "trait".to_string(),
            ];
            type_config.extend(extra_types_array);
            PhpFileCleaner::set_type_config(type_config);

            extra_types
        })
    }

    /// Cross-platform safe version of is_readable()
    ///
    /// This will also check for readability by reading the file as is_readable can not be trusted on network-mounts
    /// and \\wsl$ paths. See https://github.com/composer/composer/issues/8231 and https://bugs.php.net/bug.php?id=68926
    fn is_readable(path: &str) -> bool {
        if is_readable(path) {
            return true;
        }

        if is_file(path) {
            return file_get_contents(path).is_some();
        }

        // assume false otherwise
        false
    }
}
