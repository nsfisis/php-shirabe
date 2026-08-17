//! ref: composer/vendor/composer/class-map-generator/src/PhpFileParser.php

use crate::php_file_cleaner::PhpFileCleaner;
use shirabe_php_shim::{
    CaptureKey, PHP_EOL, RuntimeException, file_exists, file_get_contents, function_exists,
    is_file, is_readable, ltrim, php_strip_whitespace, preg_match_all2, str_replace_array, strrpos,
    substr, trim,
};
use std::sync::OnceLock;

pub struct PhpFileParser;

impl PhpFileParser {
    pub fn find_classes(path: &str) -> anyhow::Result<Vec<String>> {
        let extra_types = Self::get_extra_types();

        if !function_exists("php_strip_whitespace") {
            return Err(RuntimeException::new("Classmap generation relies on the php_strip_whitespace function, but it has been disabled by the disable_functions directive.".to_string()).into());
        }

        // Use @ here instead of Silencer to actively suppress 'unhelpful' output
        let contents = match php_strip_whitespace(path) {
            Ok(contents) if !contents.is_empty() => contents,
            stripped => {
                let mut message: String;
                if !file_exists(path) {
                    message = format!(
                        "File at \"{}\" does not exist, check your classmap definitions",
                        path
                    );
                } else if !Self::is_readable(path) {
                    message = format!(
                        "File at \"{}\" is not readable, check its permissions",
                        path
                    );
                } else if trim(file_get_contents(path).unwrap_or_default().as_str(), None)
                    .is_empty()
                {
                    // The input file was really empty and thus contains no classes
                    return Ok(vec![]);
                } else {
                    message = format!(
                        "File at \"{}\" could not be parsed as PHP, it may be binary or corrupted",
                        path
                    );
                }

                if let Err(error) = stripped {
                    message = format!(
                        "{}{}{}{}{}",
                        message, PHP_EOL, "The following message may be helpful:", PHP_EOL, error
                    );
                }

                return Err(RuntimeException::new(message).into());
            }
        };

        // return early if there is no chance of matching anything in this file
        let pattern = format!("{{\\b(?:class|interface|trait{})\\s}}i", extra_types);
        let max_matches = preg_match_all2(&pattern, &contents).occurrence_count();
        if max_matches == 0 {
            return Ok(vec![]);
        }

        let mut p = PhpFileCleaner::new(contents, max_matches);
        let contents = p.clean();
        drop(p);

        // Regex pattern compatibility:
        // PHP uses `\b(?<![\\$:>])<keyword>` to require the keyword to start at a word boundary and
        // not be preceded by `\`, `$`, `:` or `>` (so `MyClass::class`, `$class`, `\class`,
        // `Foo->class` are skipped). The `regex` crate has no look-behind, so `\b` + the negative
        // look-behind are fused into a single consuming class `(?:^|[^...])` that excludes both the
        // identifier characters (reproducing `\b`) and the four operator characters. The consumed
        // separator lands in match group 0 only; the named groups are unaffected. The PCRE
        // possessive quantifiers (`++`, `*+`) are performance-only and become plain `+`/`*`.
        let pattern2 = format!(
            r"{{
            (?:
                 (?:^|[^\\$:>a-zA-Z0-9_\x7f-\xff])(?P<type>class|interface|trait{et}) \s+ (?P<name>[a-zA-Z_\x7f-\xff:][a-zA-Z0-9_\x7f-\xff:\-]*)
               | (?:^|[^\\$:>a-zA-Z0-9_\x7f-\xff])(?P<ns>namespace) (?P<nsname>\s+[a-zA-Z_\x7f-\xff][a-zA-Z0-9_\x7f-\xff]*(?:\s*\\\s*[a-zA-Z_\x7f-\xff][a-zA-Z0-9_\x7f-\xff]*)*)? \s* [\{{;]
            )
        }}ix",
            et = extra_types
        );
        let matches = preg_match_all2(&pattern2, &contents);

        let mut classes = vec![];
        let mut namespace = String::new();

        let len = matches
            .get(&CaptureKey::ByName("type".to_owned()))
            .map(|v| v.len())
            .unwrap_or(0);
        for i in 0..len {
            let ns = matches
                .get(&CaptureKey::ByName("ns".to_owned()))
                .and_then(|v| v.get(i))
                .and_then(|s| s.as_deref());
            if ns.is_some_and(|ns| !ns.is_empty()) {
                let nsname = matches
                    .get(&CaptureKey::ByName("nsname".to_owned()))
                    .and_then(|v| v.get(i))
                    .and_then(|s| s.as_deref())
                    .unwrap_or("");
                namespace = str_replace_array(
                    &[
                        " ".to_string(),
                        "\t".to_string(),
                        "\r".to_string(),
                        "\n".to_string(),
                    ],
                    &["".to_string()],
                    nsname,
                ) + "\\";
            } else {
                let name = matches
                    .get(&CaptureKey::ByName("name".to_owned()))
                    .and_then(|v| v.get(i))
                    .and_then(|s| s.as_deref())
                    .expect("the `name` group participates whenever `ns` does not");
                // skip anon classes extending/implementing
                if name == "extends" {
                    continue;
                }
                if name == "implements" {
                    continue;
                }

                let name: String = if let Some(stripped) = name.strip_prefix(':') {
                    // This is an XHP class, https://github.com/facebook/xhp
                    "xhp".to_string()
                        + &str_replace_array(
                            &["-".to_string(), ":".to_string()],
                            &["_".to_string(), "__".to_string()],
                            stripped,
                        )
                } else if matches
                    .get(&CaptureKey::ByName("type".to_owned()))
                    .and_then(|v| v.get(i))
                    .and_then(|s| s.as_deref())
                    .unwrap_or("")
                    .to_lowercase()
                    == "enum"
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
            // TODO(port): PHP also scans for enums on HHVM 3.3 and above
            // (`defined('HHVM_VERSION') && version_compare(HHVM_VERSION, '3.3', '>=')`).
            if shirabe_php_rpc::get_php_version().version_id >= 80100 {
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
