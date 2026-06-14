//! ref: composer/src/Composer/Console/HtmlOutputFormatter.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::{CaptureKey, Preg};
use shirabe_external_packages::symfony::console::formatter::OutputFormatter;
use shirabe_external_packages::symfony::console::formatter::OutputFormatterInterface;
use shirabe_external_packages::symfony::console::formatter::OutputFormatterStyleInterface;

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

    pub fn new(styles: IndexMap<String, Box<dyn OutputFormatterStyleInterface>>) -> Self {
        Self {
            inner: OutputFormatter::new(true, styles),
        }
    }

    pub fn format(&mut self, message: Option<&str>) -> anyhow::Result<Option<String>> {
        let formatted = self.inner.format(message)?;

        let formatted = match formatted {
            Some(formatted) => formatted,
            None => return Ok(None),
        };

        let clear_escape_codes = "(?:39|49|0|22|24|25|27|28)";
        let pattern = format!(
            "{{\\033\\[([0-9;]+)m(.*?)\\033\\[(?:{};)*?{}m}}s",
            clear_escape_codes, clear_escape_codes
        );

        Ok(Some(Preg::replace_callback(
            &pattern,
            |matches| self.format_html(matches),
            &formatted,
        )))
    }

    fn format_html(&self, matches: &IndexMap<CaptureKey, String>) -> String {
        let codes_str = matches
            .get(&CaptureKey::ByIndex(1))
            .map(|s| s.as_str())
            .unwrap_or("");
        let content = matches
            .get(&CaptureKey::ByIndex(2))
            .map(|s| s.as_str())
            .unwrap_or("");
        let mut out = String::from("<span style=\"");

        for code_str in codes_str.split(';') {
            let code: i64 = code_str.parse().unwrap_or(0);
            if let Some(&(_, color)) = Self::AVAILABLE_FOREGROUND_COLORS
                .iter()
                .find(|&&(k, _)| k == code)
            {
                out.push_str(&format!("color:{};", color));
            } else if let Some(&(_, color)) = Self::AVAILABLE_BACKGROUND_COLORS
                .iter()
                .find(|&&(k, _)| k == code)
            {
                out.push_str(&format!("background-color:{};", color));
            } else if let Some(&(_, option)) =
                Self::AVAILABLE_OPTIONS.iter().find(|&&(k, _)| k == code)
            {
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
