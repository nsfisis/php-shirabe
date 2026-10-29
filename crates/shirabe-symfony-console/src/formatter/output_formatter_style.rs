//! ref: composer/vendor/symfony/console/Formatter/OutputFormatterStyle.php

use crate::color::Color;
use crate::formatter::output_formatter_style_interface::OutputFormatterStyleInterface;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{PHP_SERVER, array_search_in_vec, bconcat, getenv};

/// Formatter style class for defining styles.
#[derive(Debug, Clone)]
pub struct OutputFormatterStyle {
    color: Color,
    foreground: BString,
    background: BString,
    options: Vec<BString>,
    href: Option<BString>,
    handles_href_gracefully: Option<bool>,
}

impl OutputFormatterStyle {
    /// Initializes output formatter style.
    pub fn new(
        foreground: Option<&BStr>,
        background: Option<&BStr>,
        options: Vec<BString>,
    ) -> Self {
        let foreground = BString::from(foreground.unwrap_or_default());
        let background = BString::from(background.unwrap_or_default());
        let color = Color::new(foreground.as_bstr(), background.as_bstr(), &options).unwrap();
        Self {
            color,
            foreground,
            background,
            options,
            href: None,
            handles_href_gracefully: None,
        }
    }

    pub fn set_href(&mut self, url: impl Into<BString>) {
        self.href = Some(url.into());
    }

    fn update_color(&mut self) {
        self.color = Color::new(
            self.foreground.as_bstr(),
            self.background.as_bstr(),
            &self.options,
        )
        .unwrap();
    }
}

impl OutputFormatterStyleInterface for OutputFormatterStyle {
    fn set_foreground(&mut self, color: Option<&BStr>) {
        self.foreground = BString::from(color.unwrap_or_default());
        self.update_color();
    }

    fn set_background(&mut self, color: Option<&BStr>) {
        self.background = BString::from(color.unwrap_or_default());
        self.update_color();
    }

    fn set_option(&mut self, option: &BStr) {
        self.options.push(BString::from(option));
        self.update_color();
    }

    fn unset_option(&mut self, option: &BStr) {
        let pos = array_search_in_vec(option, &self.options);
        if let Some(pos) = pos {
            self.options.remove(pos);
        }

        self.update_color();
    }

    fn set_options(&mut self, options: Vec<BString>) {
        self.options = options;
        self.update_color();
    }

    fn apply(&mut self, text: &BStr) -> BString {
        let mut text = BString::from(text);

        if self.handles_href_gracefully.is_none() {
            self.handles_href_gracefully = Some(
                getenv("TERMINAL_EMULATOR").as_deref()
                    != Some(std::ffi::OsStr::new("JetBrains-JediTerm"))
                    && (getenv("KONSOLE_VERSION").is_none_or(|v| v.is_empty())
                        || getenv("KONSOLE_VERSION")
                            // TODO(bytes): the version is parsed as a &str.
                            .map(|v| {
                                v.into_string()
                                    .unwrap_or_else(|v| {
                                        panic!("KONSOLE_VERSION is not valid UTF-8: {v:?}")
                                    })
                                    .parse::<i64>()
                                    .unwrap_or(0)
                            })
                            .unwrap_or(0)
                            > 201100)
                    && PHP_SERVER
                        .lock()
                        .unwrap()
                        .get("IDEA_INITIAL_DIRECTORY")
                        .is_none(),
            );
        }

        if let Some(href) = &self.href
            && self.handles_href_gracefully == Some(true)
        {
            text = bconcat!("\x1b]8;;", href, "\x1b\\", text, "\x1b]8;;\x1b\\");
        }

        self.color.apply(text.as_bstr())
    }

    fn clone_box(&self) -> Box<dyn OutputFormatterStyleInterface> {
        Box::new(self.clone())
    }
}
