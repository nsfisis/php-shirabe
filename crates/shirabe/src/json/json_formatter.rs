//! ref: composer/src/Composer/Json/JsonFormatter.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{PhpMixed, function_exists, mb_convert_encoding, pack};

pub struct JsonFormatter;

impl JsonFormatter {
    /**
     * This code is based on the function found at:
     *  http://recursive-design.com/blog/2008/03/11/format-json-with-php/
     *
     * Originally licensed under MIT by Dave Perrett <mail@recursive-design.com>
     */
    pub fn format(json: String, unescape_unicode: bool, unescape_slashes: bool) -> String {
        let mut result = String::new();
        let mut pos: usize = 0;
        let indent_str = "    ";
        let new_line = "\n";
        let mut out_of_quotes = true;
        let mut buffer = String::new();
        let mut noescape = true;

        let chars: Vec<char> = json.chars().collect();
        let str_len = chars.len();

        for i in 0..str_len {
            let char_ = chars[i];

            if char_ == '"' && noescape {
                out_of_quotes = !out_of_quotes;
            }

            if !out_of_quotes {
                buffer.push(char_);
                noescape = if char_ == '\\' { !noescape } else { true };
                continue;
            }
            if !buffer.is_empty() {
                if unescape_slashes {
                    buffer = buffer.replace("\\/", "/");
                }

                if unescape_unicode && function_exists("mb_convert_encoding") {
                    buffer = Preg::replace_callback(
                        r"/(\\+)u([0-9a-f]{4})/i",
                        |matches: &[String]| -> String {
                            let l = matches[1].len();

                            if l % 2 != 0 {
                                let code = i64::from_str_radix(&matches[2], 16).unwrap_or(0);
                                if code >= 0xD800 && code <= 0xDFFF {
                                    return matches[0].clone();
                                }

                                return "\\".repeat(l - 1)
                                    + &mb_convert_encoding(
                                        pack("H*", &[PhpMixed::String(matches[2].clone())]),
                                        "UTF-8",
                                        "UCS-2BE",
                                    );
                            }

                            matches[0].clone()
                        },
                        &buffer,
                    );
                }

                result.push_str(&buffer);
                result.push(char_);
                buffer = String::new();
                continue;
            }

            let mut char_str = char_.to_string();

            if char_ == ':' {
                char_str.push(' ');
            } else if char_ == '}' || char_ == ']' {
                pos -= 1;
                let prev_char = if i > 0 { chars[i - 1] } else { '\0' };

                if prev_char != '{' && prev_char != '[' {
                    result.push_str(new_line);
                    result.push_str(&indent_str.repeat(pos));
                } else {
                    result = result.trim_end().to_string();
                }
            }

            result.push_str(&char_str);

            if char_ == ',' || char_ == '{' || char_ == '[' {
                result.push_str(new_line);

                if char_ == '{' || char_ == '[' {
                    pos += 1;
                }

                result.push_str(&indent_str.repeat(pos));
            }
        }

        result
    }
}
