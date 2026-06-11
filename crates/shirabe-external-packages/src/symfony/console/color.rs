use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use indexmap::IndexMap;

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

fn colors_get(name: &str) -> Option<i64> {
    COLORS.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn bright_colors_get(name: &str) -> Option<i64> {
    BRIGHT_COLORS
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
}

fn available_options_get(name: &str) -> Option<(i64, i64)> {
    AVAILABLE_OPTIONS
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| *v)
}

#[derive(Debug)]
pub struct Color {
    foreground: String,
    background: String,
    // option name => ['set' => i64, 'unset' => i64]
    options: IndexMap<String, (i64, i64)>,
}

impl Color {
    pub fn new(
        foreground: &str,
        background: &str,
        options: &[String],
    ) -> Result<Self, InvalidArgumentException> {
        let mut this = Self {
            foreground: Self::parse_color(foreground, false)?,
            background: Self::parse_color(background, true)?,
            options: IndexMap::new(),
        };

        for option in options {
            let available = available_options_get(option);
            if available.is_none() {
                return Err(InvalidArgumentException(
                    shirabe_php_shim::InvalidArgumentException {
                        message: shirabe_php_shim::sprintf(
                            "Invalid option specified: \"%s\". Expected one of (%s).",
                            &[
                                option.clone().into(),
                                shirabe_php_shim::implode(
                                    ", ",
                                    &AVAILABLE_OPTIONS
                                        .iter()
                                        .map(|(k, _)| k.to_string())
                                        .collect::<Vec<String>>(),
                                )
                                .into(),
                            ],
                        ),
                        code: 0,
                    },
                ));
            }

            this.options.insert(option.clone(), available.unwrap());
        }

        Ok(this)
    }

    pub fn apply(&self, text: &str) -> String {
        format!("{}{}{}", self.set(), text, self.unset())
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

        shirabe_php_shim::sprintf(
            "\u{1b}[%sm",
            &[shirabe_php_shim::implode(";", &set_codes).into()],
        )
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

        shirabe_php_shim::sprintf(
            "\u{1b}[%sm",
            &[shirabe_php_shim::implode(";", &unset_codes).into()],
        )
    }

    fn parse_color(color: &str, background: bool) -> Result<String, InvalidArgumentException> {
        if color.is_empty() {
            return Ok(String::new());
        }

        if &color[0..1] == "#" {
            let mut color = shirabe_php_shim::substr(color, 1, None);

            if shirabe_php_shim::strlen(&color) == 3 {
                let c: Vec<char> = color.chars().collect();
                color = format!("{}{}{}{}{}{}", c[0], c[0], c[1], c[1], c[2], c[2]);
            }

            if shirabe_php_shim::strlen(&color) != 6 {
                return Err(InvalidArgumentException(
                    shirabe_php_shim::InvalidArgumentException {
                        message: shirabe_php_shim::sprintf(
                            "Invalid \"%s\" color.",
                            &[color.clone().into()],
                        ),
                        code: 0,
                    },
                ));
            }

            return Ok(format!(
                "{}{}",
                if background { "4" } else { "3" },
                Self::convert_hex_color_to_ansi(shirabe_php_shim::hexdec(&color))
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
        Err(InvalidArgumentException(
            shirabe_php_shim::InvalidArgumentException {
                message: shirabe_php_shim::sprintf(
                    "Invalid \"%s\" color; expected one of (%s).",
                    &[
                        color.into(),
                        shirabe_php_shim::implode(", ", &available).into(),
                    ],
                ),
                code: 0,
            },
        ))
    }

    fn convert_hex_color_to_ansi(color: i64) -> String {
        let r = (color >> 16) & 255;
        let g = (color >> 8) & 255;
        let b = color & 255;

        // see https://github.com/termstandard/colors/ for more information about true color support
        if shirabe_php_shim::getenv("COLORTERM").as_deref() != Some("truecolor") {
            return Self::degrade_hex_color_to_ansi(r, g, b).to_string();
        }

        shirabe_php_shim::sprintf("8;2;%d;%d;%d", &[r.into(), g.into(), b.into()])
    }

    fn degrade_hex_color_to_ansi(r: i64, g: i64, b: i64) -> i64 {
        if shirabe_php_shim::round(Self::get_saturation(r, g, b) as f64 / 50.0, 0) == 0.0 {
            return 0;
        }

        ((shirabe_php_shim::round(b as f64 / 255.0, 0) as i64) << 2)
            | ((shirabe_php_shim::round(g as f64 / 255.0, 0) as i64) << 1)
            | (shirabe_php_shim::round(r as f64 / 255.0, 0) as i64)
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
