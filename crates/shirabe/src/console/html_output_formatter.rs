//! ref: composer/src/Composer/Console/HtmlOutputFormatter.php

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{PhpMixed, PregMatches, bconcat, explode, intval, preg_replace_callback};
use shirabe_symfony_console::formatter::{
    OutputFormatter, OutputFormatterInterface, OutputFormatterStyleInterface,
};

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

    pub fn new(styles: IndexMap<BString, Box<dyn OutputFormatterStyleInterface>>) -> Self {
        Self {
            inner: OutputFormatter::new(true, styles),
        }
    }

    pub fn format(&mut self, message: Option<&BStr>) -> anyhow::Result<Option<BString>> {
        let formatted = self.inner.format(message)?;

        let formatted = match formatted {
            Some(formatted) => formatted,
            None => return Ok(None),
        };

        let clear_escape_codes = "(?:39|49|0|22|24|25|27|28)";
        // Regex pattern compatibility:
        // The PHP pattern writes the ESC byte as the octal escape `\033`; the `regex` crate reads
        // `\0` as a backreference, so spell ESC as `\x1b` instead.
        let pattern = format!(
            "{{\\x1b\\[([0-9;]+)m(.*?)\\x1b\\[(?:{};)*?{}m}}s",
            clear_escape_codes, clear_escape_codes
        );

        Ok(Some(
            preg_replace_callback(
                &pattern,
                |matches| Ok(self.format_html(matches)),
                &formatted,
            )
            .expect("the replacement callback cannot fail"),
        ))
    }

    fn format_html(&self, matches: &PregMatches) -> BString {
        let codes_str = matches.get(1).unwrap_or_default();
        let content = matches.get(2).unwrap_or_default();
        let mut out = String::from("<span style=\"");

        for code_str in explode(";", codes_str) {
            let code = intval(&PhpMixed::string(code_str));
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

        bconcat!(out, "\">", content, "</span>")
    }
}
