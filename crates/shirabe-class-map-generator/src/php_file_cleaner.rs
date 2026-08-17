//! ref: composer/vendor/composer/class-map-generator/src/PhpFileCleaner.php

use indexmap::IndexMap;
use shirabe_php_shim::{PregMatches, preg_match};
use std::sync::Mutex;

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
                    // Regex pattern compatibility:
                    // PHP uses `.\b(?<![$:>])<type>` anchored (`A`): it consumes the single char
                    // before the keyword (`.`), requires a word boundary there (`\b`) and forbids
                    // that char being `$`, `:` or `>`. The `regex` crate has no look-behind, so the
                    // consumed char plus both guards collapse into one negated class
                    // `[^a-zA-Z0-9_$:>]` (the `\w` set reproducing `\b`, plus the three operators).
                    // The possessive quantifiers (`++`, `*+`) are performance-only and become plain
                    // `+`/`*`. The `A` (anchored) modifier becomes a leading `^` over the sub-slice
                    // that begins at the search offset.
                    pattern: format!(
                        "{{^[^a-zA-Z0-9_$:>]{}\\s+[a-zA-Z_\\x7f-\\xff:][a-zA-Z0-9_\\x7f-\\xff:\\-]*}}is",
                        r#type
                    ),
                },
            );
        }

        let keys: String = type_config.keys().collect();
        let rest_pattern = format!("{{^[^?\"'</{}]+}}", keys);

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
                    // Regex pattern compatibility:
                    // PHP matches `<<<`, an optional quote, the identifier, then requires the
                    // closing quote to be the exact same character via `\1`. The `regex` crate has
                    // no backreferences, so the three quote states (none, `'`, `"`) are expanded
                    // into separate alternatives, each capturing the identifier in its own group.
                    if let Some(r#match) = self.r#match(
                        r#"{^<<<[ \t]*(?:"([a-zA-Z_\x80-\xff][a-zA-Z0-9_\x80-\xff]*)"|'([a-zA-Z_\x80-\xff][a-zA-Z0-9_\x80-\xff]*)'|([a-zA-Z_\x80-\xff][a-zA-Z0-9_\x80-\xff]*))(?:\r\n|\n|\r)}"#,
                    ) {
                        let matched_len = r#match
                            .get(0)
                            .map(|s| s.len())
                            .unwrap_or(0);
                        let delimiter = [1, 2, 3]
                            .iter()
                            .find_map(|i| {
                                r#match
                                    .get(*i)
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_string)
                            })
                            .unwrap_or_default();
                        self.index += matched_len;
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
                        if end <= self.len && self.contents[self.index..end] == entry.name {
                            let offset = if self.index > 0 { self.index - 1 } else { 0 };
                            if let Some(r#match) =
                                preg_match(&entry.pattern, &self.contents[offset..])
                            {
                                return clean + r#match.get(0).unwrap_or("");
                            }
                        }
                    }
                }

                self.index += 1;
                let rest_pattern = REST_PATTERN.lock().unwrap().clone();
                if let Some(rest_pattern) = rest_pattern {
                    if let Some(r#match) = self.r#match(&rest_pattern) {
                        let m0 = r#match.get(0).unwrap_or_default().to_string();
                        clean.push(char);
                        clean.push_str(&m0);
                        self.index += m0.len();
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
                    if end <= self.len && &self.contents[self.index..end] == delimiter {
                        // Regex pattern compatibility:
                        // PHP follows the delimiter with a negative lookahead
                        // `(?![a-zA-Z0-9_\x80-\xff])` to ensure it isn't a prefix of a longer
                        // identifier. The `regex` crate has no look-around, so the boundary is
                        // checked directly on the next byte instead.
                        let next_is_identifier_byte = self
                            .contents
                            .as_bytes()
                            .get(end)
                            .map(|&b| b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80)
                            .unwrap_or(false);
                        if !next_is_identifier_byte {
                            self.index += delimiter_length;
                            return;
                        }
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

    // Regex pattern compatibility:
    // PHP runs `$regex` anchored (`A`) at `$this->index`, so it must match starting exactly there.
    // The `regex` crate anchors only at the head of the haystack, so the search runs over the part
    // of the contents that begins at the index and the patterns carry a leading `^` instead of the
    // `A` modifier.
    fn r#match(&self, regex: &str) -> Option<PregMatches<'_>> {
        preg_match(regex, &self.contents[self.index..])
    }
}
