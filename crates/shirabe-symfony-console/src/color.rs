//! ref: composer/vendor/symfony/console/Color.php

use crate::exception::InvalidArgumentException;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{bconcat, getenv, hexdec, implode, php_str, round, strlen, substr};

const COLORS: [(&BStr, i64); 9] = [
    (php_str(b"black"), 0),
    (php_str(b"red"), 1),
    (php_str(b"green"), 2),
    (php_str(b"yellow"), 3),
    (php_str(b"blue"), 4),
    (php_str(b"magenta"), 5),
    (php_str(b"cyan"), 6),
    (php_str(b"white"), 7),
    (php_str(b"default"), 9),
];

const BRIGHT_COLORS: [(&BStr, i64); 8] = [
    (php_str(b"gray"), 0),
    (php_str(b"bright-red"), 1),
    (php_str(b"bright-green"), 2),
    (php_str(b"bright-yellow"), 3),
    (php_str(b"bright-blue"), 4),
    (php_str(b"bright-magenta"), 5),
    (php_str(b"bright-cyan"), 6),
    (php_str(b"bright-white"), 7),
];

const AVAILABLE_OPTIONS: [(&BStr, (i64, i64)); 5] = [
    (php_str(b"bold"), (1, 22)),
    (php_str(b"underscore"), (4, 24)),
    (php_str(b"blink"), (5, 25)),
    (php_str(b"reverse"), (7, 27)),
    (php_str(b"conceal"), (8, 28)),
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
    foreground: BString,
    background: BString,
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
                return Err(InvalidArgumentException::new(bconcat!(
                    "Invalid option specified: \"",
                    option,
                    "\". Expected one of (",
                    implode(
                        ", ",
                        &AVAILABLE_OPTIONS
                            .iter()
                            .map(|(k, _)| *k)
                            .collect::<Vec<_>>()
                    ),
                    ")."
                )));
            }

            this.options.insert(option.clone(), available.unwrap());
        }

        Ok(this)
    }

    pub fn apply(&self, text: &BStr) -> BString {
        bconcat!(self.set(), text, self.unset())
    }

    pub fn set(&self) -> BString {
        let mut set_codes: Vec<BString> = Vec::new();
        if !self.foreground.is_empty() {
            set_codes.push(self.foreground.clone());
        }
        if !self.background.is_empty() {
            set_codes.push(self.background.clone());
        }
        for option in self.options.values() {
            set_codes.push(bconcat!(option.0));
        }
        if set_codes.is_empty() {
            return BString::default();
        }

        bconcat!("\u{1b}[", implode(";", &set_codes), "m")
    }

    pub fn unset(&self) -> BString {
        let mut unset_codes: Vec<BString> = Vec::new();
        if !self.foreground.is_empty() {
            unset_codes.push(BString::from("39"));
        }
        if !self.background.is_empty() {
            unset_codes.push(BString::from("49"));
        }
        for option in self.options.values() {
            unset_codes.push(bconcat!(option.1));
        }
        if unset_codes.is_empty() {
            return BString::default();
        }

        bconcat!("\u{1b}[", implode(";", &unset_codes), "m")
    }

    fn parse_color(color: &BStr, background: bool) -> Result<BString, InvalidArgumentException> {
        if color.is_empty() {
            return Ok(BString::default());
        }

        if color[0] == b'#' {
            let mut color = substr(color, 1, None);

            if strlen(&color) == 3 {
                let c = color.clone();
                color = BString::from(vec![c[0], c[0], c[1], c[1], c[2], c[2]]);
            }

            if strlen(&color) != 6 {
                return Err(InvalidArgumentException::new(bconcat!(
                    "Invalid \"",
                    color,
                    "\" color."
                )));
            }

            return Ok(bconcat!(
                if background { "4" } else { "3" },
                Self::convert_hex_color_to_ansi(hexdec(&color))
            ));
        }

        if let Some(code) = colors_get(color) {
            return Ok(bconcat!(if background { "4" } else { "3" }, code));
        }

        if let Some(code) = bright_colors_get(color) {
            return Ok(bconcat!(if background { "10" } else { "9" }, code));
        }

        let mut available: Vec<&BStr> = COLORS.iter().map(|(k, _)| *k).collect();
        available.extend(BRIGHT_COLORS.iter().map(|(k, _)| *k));
        Err(InvalidArgumentException::new(bconcat!(
            "Invalid \"",
            color,
            "\" color; expected one of (",
            implode(", ", &available),
            ")."
        )))
    }

    fn convert_hex_color_to_ansi(color: i64) -> BString {
        let r = (color >> 16) & 255;
        let g = (color >> 8) & 255;
        let b = color & 255;

        // see https://github.com/termstandard/colors/ for more information about true color support
        if getenv("COLORTERM").as_deref() != Some(std::ffi::OsStr::new("truecolor")) {
            return bconcat!(Self::degrade_hex_color_to_ansi(r, g, b));
        }

        bconcat!("8;2;", r, ";", g, ";", b)
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
