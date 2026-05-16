//! ref: composer/vendor/composer/class-map-generator/src/PhpFileCleaner.php

use std::sync::Mutex;
use indexmap::IndexMap;
use shirabe_php_shim::preg_quote;
use shirabe_external_packages::composer::pcre::preg::Preg;

#[derive(Debug, Clone)]
struct TypeConfigEntry {
    name: String,
    length: usize,
    pattern: String,
}

static TYPE_CONFIG: Mutex<Option<IndexMap<char, TypeConfigEntry>>> = Mutex::new(None);
static REST_PATTERN: Mutex<Option<String>> = Mutex::new(None);

#[derive(Debug)]
pub struct PhpFileCleaner {
    contents: String,
    len: usize,
    max_matches: usize,
    index: usize,
}

impl PhpFileCleaner {
    pub fn set_type_config(types: Vec<String>) {
        let mut type_config: IndexMap<char, TypeConfigEntry> = IndexMap::new();

        for r#type in &types {
            let first_char = r#type.chars().next().unwrap();
            type_config.insert(
                first_char,
                TypeConfigEntry {
                    name: r#type.clone(),
                    length: r#type.len(),
                    pattern: format!(
                        "{{.\\b(?<![$:>]){}\\s++[a-zA-Z_\\x7f-\\xff:][a-zA-Z0-9_\\x7f-\\xff:\\-]*+}}Ais",
                        r#type
                    ),
                },
            );
        }

        let keys: String = type_config.keys().collect();
        let rest_pattern = format!("{{[^?\"'</{}]+}}A", keys);

        *REST_PATTERN.lock().unwrap() = Some(rest_pattern);
        *TYPE_CONFIG.lock().unwrap() = Some(type_config);
    }

    pub fn new(contents: String, max_matches: usize) -> Self {
        let len = contents.len();
        PhpFileCleaner {
            contents,
            len,
            max_matches,
            index: 0,
        }
    }

    pub fn clean(&mut self) -> String {
        let mut clean = String::new();

        'outer: while self.index < self.len {
            self.skip_to_php();
            clean.push_str("<?");

            while self.index < self.len {
                let char = self.contents.as_bytes()[self.index] as char;

                if char == '?' && self.peek('>') {
                    clean.push_str("?>");
                    self.index += 2;
                    continue 'outer;
                }

                if char == '"' {
                    self.skip_string('"');
                    clean.push_str("null");
                    continue;
                }

                if char == '\'' {
                    self.skip_string('\'');
                    clean.push_str("null");
                    continue;
                }

                if char == '<' && self.peek('<') {
                    let mut r#match: Vec<String> = vec![];
                    if self.r#match(
                        r"{<<<[ \t]*+(['\"]?)([a-zA-Z_\x80-\xff][a-zA-Z0-9_\x80-\xff]*+)\1(?:\r\n|\n|\r)}A",
                        Some(&mut r#match),
                    ) {
                        self.index += r#match[0].len();
                        let delimiter = r#match[2].clone();
                        self.skip_heredoc(&delimiter);
                        clean.push_str("null");
                        continue;
                    }
                }

                if char == '/' {
                    if self.peek('/') {
                        self.skip_to_newline();
                        continue;
                    }

                    if self.peek('*') {
                        self.skip_comment();
                        continue;
                    }
                }

                if self.max_matches == 1 {
                    let type_entry = {
                        let guard = TYPE_CONFIG.lock().unwrap();
                        guard.as_ref().and_then(|tc| tc.get(&char)).cloned()
                    };
                    if let Some(entry) = type_entry {
                        let end = self.index + entry.length;
                        if end <= self.len
                            && &self.contents[self.index..end] == entry.name
                        {
                            let offset = if self.index > 0 { self.index - 1 } else { 0 };
                            let mut r#match: Vec<String> = vec![];
                            if Preg::is_match_at(
                                &entry.pattern,
                                &self.contents,
                                &mut r#match,
                                0,
                                offset,
                            ) {
                                return clean + &r#match[0];
                            }
                        }
                    }
                }

                self.index += 1;
                let rest_pattern = REST_PATTERN.lock().unwrap().clone();
                if let Some(rest_pattern) = rest_pattern {
                    let mut r#match: Vec<String> = vec![];
                    if self.r#match(&rest_pattern, Some(&mut r#match)) {
                        clean.push(char);
                        clean.push_str(&r#match[0]);
                        self.index += r#match[0].len();
                    } else {
                        clean.push(char);
                    }
                } else {
                    clean.push(char);
                }
            }
        }

        clean
    }

    fn skip_to_php(&mut self) {
        while self.index < self.len {
            if self.contents.as_bytes()[self.index] as char == '<' && self.peek('?') {
                self.index += 2;
                break;
            }

            self.index += 1;
        }
    }

    fn skip_string(&mut self, delimiter: char) {
        self.index += 1;
        while self.index < self.len {
            let c = self.contents.as_bytes()[self.index] as char;
            if c == '\\' && (self.peek('\\') || self.peek(delimiter)) {
                self.index += 2;
                continue;
            }

            if c == delimiter {
                self.index += 1;
                break;
            }

            self.index += 1;
        }
    }

    fn skip_comment(&mut self) {
        self.index += 2;
        while self.index < self.len {
            if self.contents.as_bytes()[self.index] as char == '*' && self.peek('/') {
                self.index += 2;
                break;
            }

            self.index += 1;
        }
    }

    fn skip_to_newline(&mut self) {
        while self.index < self.len {
            let c = self.contents.as_bytes()[self.index] as char;
            if c == '\r' || c == '\n' {
                return;
            }

            self.index += 1;
        }
    }

    fn skip_heredoc(&mut self, delimiter: &str) {
        let first_delimiter_char = delimiter.chars().next().unwrap();
        let delimiter_length = delimiter.len();
        let delimiter_pattern = format!(
            "{{{}(?![a-zA-Z0-9_\\x80-\\xff])}}A",
            preg_quote(delimiter, None)
        );

        while self.index < self.len {
            let c = self.contents.as_bytes()[self.index] as char;

            // check if we find the delimiter after some spaces/tabs
            match c {
                '\t' | ' ' => {
                    self.index += 1;
                    continue;
                }
                _ if c == first_delimiter_char => {
                    let end = self.index + delimiter_length;
                    if end <= self.len
                        && &self.contents[self.index..end] == delimiter
                        && self.r#match(&delimiter_pattern, None)
                    {
                        self.index += delimiter_length;
                        return;
                    }
                }
                _ => {}
            }

            // skip the rest of the line
            self.skip_to_newline();

            // skip newlines
            while self.index < self.len {
                let c = self.contents.as_bytes()[self.index] as char;
                if c == '\r' || c == '\n' {
                    self.index += 1;
                } else {
                    break;
                }
            }
        }
    }

    fn peek(&self, char: char) -> bool {
        self.index + 1 < self.len && self.contents.as_bytes()[self.index + 1] as char == char
    }

    fn r#match(&self, regex: &str, r#match: Option<&mut Vec<String>>) -> bool {
        Preg::is_match_strict_groups_at(regex, &self.contents, r#match, 0, self.index)
    }
}
