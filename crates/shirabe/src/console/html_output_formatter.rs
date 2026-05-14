//! ref: composer/src/Composer/Console/HtmlOutputFormatter.php

use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::console::formatter::output_formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::formatter::output_formatter_style::OutputFormatterStyle;
use indexmap::IndexMap;

#[derive(Debug)]
pub struct HtmlOutputFormatter {
    inner: OutputFormatter,
}

impl HtmlOutputFormatter {
    const AVAILABLE_FOREGROUND_COLORS: &'static [(i64, &'static str)] = &[
        (30, "black"),
        (31, "red"),
        (32, "green"),
        (33, "yellow"),
        (34, "blue"),
        (35, "magenta"),
        (36, "cyan"),
        (37, "white"),
    ];

    const AVAILABLE_BACKGROUND_COLORS: &'static [(i64, &'static str)] = &[
        (40, "black"),
        (41, "red"),
        (42, "green"),
        (43, "yellow"),
        (44, "blue"),
        (45, "magenta"),
        (46, "cyan"),
        (47, "white"),
    ];

    const AVAILABLE_OPTIONS: &'static [(i64, &'static str)] = &[
        (1, "bold"),
        (4, "underscore"),
        //5 => "blink",
        //7 => "reverse",
        //8 => "conceal"
    ];

    pub fn new(styles: IndexMap<String, OutputFormatterStyle>) -> Self {
        Self {
            inner: OutputFormatter::new(true, styles),
        }
    }

    pub fn format(&self, message: Option<&str>) -> Option<String> {
        let formatted = self.inner.format(message)?;

        let clear_escape_codes = "(?:39|49|0|22|24|25|27|28)";
        let pattern = format!(
            "{{\\033\\[([0-9;]+)m(.*?)\\033\\[(?:{};)*?{}m}}s",
            clear_escape_codes, clear_escape_codes
        );

        Preg::replace_callback(&pattern, |matches| self.format_html(matches), formatted).ok()
    }

    fn format_html(&self, matches: Vec<Option<String>>) -> String {
        let codes_str = matches[1].as_deref().unwrap_or("");
        let content = matches[2].as_deref().unwrap_or("");
        let mut out = String::from("<span style=\"");

        for code_str in codes_str.split(';') {
            let code: i64 = code_str.parse().unwrap_or(0);
            if let Some(&(_, color)) = Self::AVAILABLE_FOREGROUND_COLORS.iter().find(|&&(k, _)| k == code) {
                out.push_str(&format!("color:{};", color));
            } else if let Some(&(_, color)) = Self::AVAILABLE_BACKGROUND_COLORS.iter().find(|&&(k, _)| k == code) {
                out.push_str(&format!("background-color:{};", color));
            } else if let Some(&(_, option)) = Self::AVAILABLE_OPTIONS.iter().find(|&&(k, _)| k == code) {
                match option {
                    "bold" => out.push_str("font-weight:bold;"),
                    "underscore" => out.push_str("text-decoration:underline;"),
                    _ => {}
                }
            }
        }

        format!("{}\">{}</span>", out, content)
    }
}
