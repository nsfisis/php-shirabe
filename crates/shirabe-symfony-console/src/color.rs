//! ref: composer/vendor/symfony/console/Color.php

use crate::exception::InvalidArgumentException;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{bconcat, getenv, hexdec, round, strlen, substr};

const COLORS: [(&str, i64); 9] = [
    ("black", 0),
    ("red", 1),
    ("green", 2),
    ("yellow", 3),
    ("blue", 4),
    ("magenta", 5),
    ("cyan", 6),
    ("white", 7),
    ("default", 9),
];

const BRIGHT_COLORS: [(&str, i64); 8] = [
    ("gray", 0),
    ("bright-red", 1),
    ("bright-green", 2),
    ("bright-yellow", 3),
    ("bright-blue", 4),
    ("bright-magenta", 5),
    ("bright-cyan", 6),
    ("bright-white", 7),
];

const AVAILABLE_OPTIONS: [(&str, (i64, i64)); 5] = [
    ("bold", (1, 22)),
    ("underscore", (4, 24)),
    ("blink", (5, 25)),
    ("reverse", (7, 27)),
    ("conceal", (8, 28)),
];

fn colors_get(name: &[u8]) -> Option<i64> {
    COLORS
        .iter()
        .find(|(k, _)| k.as_bytes() == name)
        .map(|(_, v)| *v)
}

fn bright_colors_get(name: &[u8]) -> Option<i64> {
    BRIGHT_COLORS
        .iter()
        .find(|(k, _)| k.as_bytes() == name)
        .map(|(_, v)| *v)
}

fn available_options_get(name: &[u8]) -> Option<(i64, i64)> {
    AVAILABLE_OPTIONS
        .iter()
        .find(|(k, _)| k.as_bytes() == name)
        .map(|(_, v)| *v)
}

#[derive(Debug, Clone)]
pub struct Color {
    foreground: String,
    background: String,
    // option name => ['set' => i64, 'unset' => i64]
    options: IndexMap<BString, (i64, i64)>,
}

impl Color {
    pub fn new(
        foreground: &BStr,
        background: &BStr,
        options: &[BString],
    ) -> Result<Self, InvalidArgumentException> {
        let mut this = Self {
            foreground: Self::parse_color(foreground, false)?,
            background: Self::parse_color(background, true)?,
            options: IndexMap::new(),
        };

        for option in options {
            let available = available_options_get(option);
            if available.is_none() {
                return Err(InvalidArgumentException::new(format!(
                    "Invalid option specified: \"{}\". Expected one of ({}).",
                    option.display_lossy(),
                    AVAILABLE_OPTIONS
                        .iter()
                        .map(|(k, _)| k.to_string())
                        .collect::<Vec<String>>()
                        .join(", "),
                )));
            }

            this.options.insert(option.clone(), available.unwrap());
        }

        Ok(this)
    }

    pub fn apply(&self, text: &BStr) -> BString {
        bconcat!(self.set(), text, self.unset())
    }

    pub fn set(&self) -> String {
        let mut set_codes: Vec<String> = Vec::new();
        if !self.foreground.is_empty() {
            set_codes.push(self.foreground.clone());
        }
        if !self.background.is_empty() {
            set_codes.push(self.background.clone());
        }
        for option in self.options.values() {
            set_codes.push(option.0.to_string());
        }
        if set_codes.is_empty() {
            return String::new();
        }

        format!("\u{1b}[{}m", set_codes.join(";"))
    }

    pub fn unset(&self) -> String {
        let mut unset_codes: Vec<String> = Vec::new();
        if !self.foreground.is_empty() {
            unset_codes.push("39".to_string());
        }
        if !self.background.is_empty() {
            unset_codes.push("49".to_string());
        }
        for option in self.options.values() {
            unset_codes.push(option.1.to_string());
        }
        if unset_codes.is_empty() {
            return String::new();
        }

        format!("\u{1b}[{}m", unset_codes.join(";"))
    }

    fn parse_color(color: &BStr, background: bool) -> Result<String, InvalidArgumentException> {
        if color.is_empty() {
            return Ok(String::new());
        }

        if color[0] == b'#' {
            let mut color = substr(color, 1, None);

            if strlen(&color) == 3 {
                let c = color.clone();
                color = BString::from(vec![c[0], c[0], c[1], c[1], c[2], c[2]]);
            }

            if strlen(&color) != 6 {
                return Err(InvalidArgumentException::new(format!(
                    "Invalid \"{}\" color.",
                    color.display_lossy()
                )));
            }

            return Ok(format!(
                "{}{}",
                if background { "4" } else { "3" },
                Self::convert_hex_color_to_ansi(hexdec(&color))
            ));
        }

        if let Some(code) = colors_get(color) {
            return Ok(format!("{}{}", if background { "4" } else { "3" }, code));
        }

        if let Some(code) = bright_colors_get(color) {
            return Ok(format!("{}{}", if background { "10" } else { "9" }, code));
        }

        let mut available: Vec<String> = COLORS.iter().map(|(k, _)| k.to_string()).collect();
        available.extend(BRIGHT_COLORS.iter().map(|(k, _)| k.to_string()));
        Err(InvalidArgumentException::new(format!(
            "Invalid \"{}\" color; expected one of ({}).",
            color.display_lossy(),
            available.join(", "),
        )))
    }

    fn convert_hex_color_to_ansi(color: i64) -> String {
        let r = (color >> 16) & 255;
        let g = (color >> 8) & 255;
        let b = color & 255;

        // see https://github.com/termstandard/colors/ for more information about true color support
        if getenv("COLORTERM").as_deref() != Some(std::ffi::OsStr::new("truecolor")) {
            return Self::degrade_hex_color_to_ansi(r, g, b).to_string();
        }

        format!("8;2;{};{};{}", r, g, b)
    }

    fn degrade_hex_color_to_ansi(r: i64, g: i64, b: i64) -> i64 {
        if round(Self::get_saturation(r, g, b) as f64 / 50.0, 0) == 0.0 {
            return 0;
        }

        ((round(b as f64 / 255.0, 0) as i64) << 2)
            | ((round(g as f64 / 255.0, 0) as i64) << 1)
            | (round(r as f64 / 255.0, 0) as i64)
    }

    fn get_saturation(r: i64, g: i64, b: i64) -> i64 {
        let r = r as f64 / 255.0;
        let g = g as f64 / 255.0;
        let b = b as f64 / 255.0;
        let v = r.max(g).max(b);

        let diff = v - r.min(g).min(b);
        if diff == 0.0 {
            return 0;
        }

        // PHP: `(int) $diff * 100 / $v`. The `(int)` cast binds to `$diff` only (and
        // since 0 <= $diff < 1 it is always 0), `/` is float division, and the function
        // return type `int` truncates the float result.
        ((diff as i64) as f64 * 100.0 / v) as i64
    }
}
