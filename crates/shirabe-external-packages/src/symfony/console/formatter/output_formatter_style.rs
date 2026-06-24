//! ref: composer/vendor/symfony/console/Formatter/OutputFormatterStyle.php

use crate::symfony::console::color::Color;
use crate::symfony::console::formatter::output_formatter_style_interface::OutputFormatterStyleInterface;

/// Formatter style class for defining styles.
#[derive(Debug, Clone)]
pub struct OutputFormatterStyle {
    color: Color,
    foreground: String,
    background: String,
    options: Vec<String>,
    href: Option<String>,
    handles_href_gracefully: Option<bool>,
}

impl OutputFormatterStyle {
    /// Initializes output formatter style.
    pub fn new(foreground: Option<&str>, background: Option<&str>, options: Vec<String>) -> Self {
        let foreground = foreground
            .filter(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();
        let background = background
            .filter(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();
        let color = Color::new(&foreground, &background, &options.clone()).unwrap();
        Self {
            color,
            foreground,
            background,
            options,
            href: None,
            handles_href_gracefully: None,
        }
    }

    pub fn set_href(&mut self, url: &str) {
        self.href = Some(url.to_string());
    }
}

impl OutputFormatterStyleInterface for OutputFormatterStyle {
    fn set_foreground(&mut self, color: Option<&str>) {
        self.foreground = color.filter(|s| !s.is_empty()).unwrap_or("").to_string();
        self.color = Color::new(&self.foreground, &self.background, &self.options.clone()).unwrap();
    }

    fn set_background(&mut self, color: Option<&str>) {
        self.background = color.filter(|s| !s.is_empty()).unwrap_or("").to_string();
        self.color = Color::new(&self.foreground, &self.background, &self.options.clone()).unwrap();
    }

    fn set_option(&mut self, option: &str) {
        self.options.push(option.to_string());
        self.color = Color::new(&self.foreground, &self.background, &self.options.clone()).unwrap();
    }

    fn unset_option(&mut self, option: &str) {
        let pos = shirabe_php_shim::array_search_in_vec(option, &self.options);
        if let Some(pos) = pos {
            self.options.remove(pos);
        }

        self.color = Color::new(&self.foreground, &self.background, &self.options.clone()).unwrap();
    }

    fn set_options(&mut self, options: Vec<String>) {
        self.options = options;
        self.color = Color::new(&self.foreground, &self.background, &self.options.clone()).unwrap();
    }

    fn apply(&mut self, text: &str) -> String {
        let mut text = text.to_string();

        if self.handles_href_gracefully.is_none() {
            self.handles_href_gracefully = Some(
                shirabe_php_shim::getenv("TERMINAL_EMULATOR").as_deref()
                    != Some(std::ffi::OsStr::new("JetBrains-JediTerm"))
                    && (shirabe_php_shim::getenv("KONSOLE_VERSION").is_none_or(|v| v.is_empty())
                        || shirabe_php_shim::getenv("KONSOLE_VERSION")
                            .map(|v| v.to_string_lossy().parse::<i64>().unwrap_or(0))
                            .unwrap_or(0)
                            > 201100)
                    && shirabe_php_shim::PHP_SERVER
                        .lock()
                        .unwrap()
                        .get("IDEA_INITIAL_DIRECTORY")
                        .is_none(),
            );
        }

        if let Some(href) = &self.href
            && self.handles_href_gracefully == Some(true)
        {
            text = format!("\x1b]8;;{href}\x1b\\{text}\x1b]8;;\x1b\\");
        }

        self.color.apply(&text)
    }

    fn clone_box(&self) -> Box<dyn OutputFormatterStyleInterface> {
        Box::new(self.clone())
    }
}
