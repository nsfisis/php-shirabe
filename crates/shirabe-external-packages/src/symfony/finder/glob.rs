//! ref: composer/vendor/symfony/finder/Glob.php

#[derive(Debug)]
pub struct Glob;

impl Glob {
    pub fn to_regex(glob: &str, strict_leading_dot: bool, strict_wildcard_slash: bool) -> String {
        let delimiter = '#';
        let mut first_byte = true;
        let mut escaping = false;
        let mut in_curlies: i64 = 0;
        let mut regex = String::new();
        let bytes = glob.as_bytes();
        let size_glob = bytes.len();
        let mut i = 0;
        while i < size_glob {
            let mut car = (bytes[i] as char).to_string();
            if first_byte && strict_leading_dot && car != "." {
                regex.push_str("(?=[^\\.])");
            }

            first_byte = car == "/";

            if first_byte
                && strict_wildcard_slash
                && i + 2 < size_glob
                && bytes[i + 1] == b'*'
                && bytes[i + 2] == b'*'
                && (i + 3 >= size_glob || bytes[i + 3] == b'/')
            {
                let mut new_car = String::from("[^/]++/");
                if i + 3 >= size_glob {
                    new_car.push('?');
                }

                if strict_leading_dot {
                    new_car = format!("(?=[^\\.]){}", new_car);
                }

                new_car = format!("/(?:{})*", new_car);
                i += 2 + usize::from(i + 3 < size_glob);

                if delimiter == '/' {
                    new_car = new_car.replace('/', "\\/");
                }

                car = new_car;
            }

            if car == delimiter.to_string()
                || car == "."
                || car == "("
                || car == ")"
                || car == "|"
                || car == "+"
                || car == "^"
                || car == "$"
            {
                regex.push('\\');
                regex.push_str(&car);
            } else if car == "*" {
                regex.push_str(if escaping {
                    "\\*"
                } else if strict_wildcard_slash {
                    "[^/]*"
                } else {
                    ".*"
                });
            } else if car == "?" {
                regex.push_str(if escaping {
                    "\\?"
                } else if strict_wildcard_slash {
                    "[^/]"
                } else {
                    "."
                });
            } else if car == "{" {
                regex.push_str(if escaping { "\\{" } else { "(" });
                if !escaping {
                    in_curlies += 1;
                }
            } else if car == "}" && in_curlies > 0 {
                regex.push_str(if escaping { "}" } else { ")" });
                if !escaping {
                    in_curlies -= 1;
                }
            } else if car == "," && in_curlies > 0 {
                regex.push_str(if escaping { "," } else { "|" });
            } else if car == "\\" {
                if escaping {
                    regex.push_str("\\\\");
                    escaping = false;
                } else {
                    escaping = true;
                }

                i += 1;
                continue;
            } else {
                regex.push_str(&car);
            }
            escaping = false;
            i += 1;
        }

        format!("{delimiter}^{regex}${delimiter}")
    }
}
