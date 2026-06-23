//! ref: composer/vendor/symfony/console/Helper/Helper.php

use crate::symfony::console::formatter::output_formatter_interface::OutputFormatterInterface;
use crate::symfony::console::helper::helper_set::HelperSet;
use crate::symfony::string::unicode_string::UnicodeString;
use std::cell::RefCell;
use std::rc::Rc;

/// Helper is the base class for all helper classes.
#[derive(Debug, Default)]
pub struct Helper {
    pub(crate) helper_set: Option<Rc<RefCell<HelperSet>>>,
}

impl Helper {
    pub fn set_helper_set(&mut self, helper_set: Option<Rc<RefCell<HelperSet>>>) {
        self.helper_set = helper_set;
    }

    pub fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        self.helper_set.clone()
    }

    /// Returns the length of a string, using mb_strwidth if it is available.
    ///
    /// @deprecated since Symfony 5.3
    pub fn strlen(string: &str) -> i64 {
        shirabe_php_shim::trigger_deprecation(
            "symfony/console",
            "5.3",
            "Method \"%s()\" is deprecated and will be removed in Symfony 6.0. Use Helper::width() or Helper::length() instead.",
            "Helper::strlen",
        );

        Self::width(string)
    }

    /// Returns the width of a string, using mb_strwidth if it is available.
    /// The width is how many characters positions the string will use.
    pub fn width(string: &str) -> i64 {
        if shirabe_php_shim::preg_match("//u", string, &mut Vec::new()) {
            return UnicodeString::new(string).width(false);
        }

        let encoding = shirabe_php_shim::mb_detect_encoding(string, None, true);
        let encoding = match encoding {
            Some(encoding) => encoding,
            None => return shirabe_php_shim::strlen(string),
        };

        shirabe_php_shim::mb_strwidth(string, Some(&encoding))
    }

    /// Returns the length of a string, using mb_strlen if it is available.
    /// The length is related to how many bytes the string will use.
    pub fn length(string: &str) -> i64 {
        if shirabe_php_shim::preg_match("//u", string, &mut Vec::new()) {
            return UnicodeString::new(string).length();
        }

        let encoding = shirabe_php_shim::mb_detect_encoding(string, None, true);
        let encoding = match encoding {
            Some(encoding) => encoding,
            None => return shirabe_php_shim::strlen(string),
        };

        shirabe_php_shim::mb_strlen(string, &encoding)
    }

    /// Returns the subset of a string, using mb_substr if it is available.
    pub fn substr(string: &str, from: i64, length: Option<i64>) -> String {
        let encoding = shirabe_php_shim::mb_detect_encoding(string, None, true);
        let encoding = match encoding {
            Some(encoding) => encoding,
            None => return shirabe_php_shim::substr(string, from, length),
        };

        shirabe_php_shim::mb_substr(string, from, length, Some(&encoding))
    }

    pub fn format_time(secs: f64) -> Option<String> {
        // [threshold, label, divisor?]
        let time_formats: [(f64, &str, Option<f64>); 9] = [
            (0.0, "< 1 sec", None),
            (1.0, "1 sec", None),
            (2.0, "secs", Some(1.0)),
            (60.0, "1 min", None),
            (120.0, "mins", Some(60.0)),
            (3600.0, "1 hr", None),
            (7200.0, "hrs", Some(3600.0)),
            (86400.0, "1 day", None),
            (172800.0, "days", Some(86400.0)),
        ];

        for (index, format) in time_formats.iter().enumerate() {
            if secs >= format.0
                && ((index + 1 < time_formats.len() && secs < time_formats[index + 1].0)
                    || index == time_formats.len() - 1)
            {
                match format.2 {
                    None => return Some(format.1.to_string()),
                    Some(divisor) => {
                        return Some(format!("{} {}", (secs / divisor).floor(), format.1));
                    }
                }
            }
        }

        None
    }

    pub fn format_memory(memory: i64) -> String {
        if memory >= 1024 * 1024 * 1024 {
            return format!("{:.1} GiB", memory as f64 / 1024.0 / 1024.0 / 1024.0);
        }

        if memory >= 1024 * 1024 {
            return format!("{:.1} MiB", memory as f64 / 1024.0 / 1024.0);
        }

        if memory >= 1024 {
            return format!("{} KiB", memory / 1024);
        }

        format!("{} B", memory)
    }

    /// @deprecated since Symfony 5.3
    pub fn strlen_without_decoration(
        formatter: &mut dyn OutputFormatterInterface,
        string: &str,
    ) -> i64 {
        shirabe_php_shim::trigger_deprecation(
            "symfony/console",
            "5.3",
            "Method \"%s()\" is deprecated and will be removed in Symfony 6.0. Use Helper::removeDecoration() instead.",
            "Helper::strlenWithoutDecoration",
        );

        Self::width(&Self::remove_decoration(formatter, string))
    }

    pub fn remove_decoration(formatter: &mut dyn OutputFormatterInterface, string: &str) -> String {
        let is_decorated = formatter.is_decorated();
        formatter.set_decorated(false);
        // remove <...> formatting
        let string = formatter.format(Some(string)).unwrap().unwrap_or_default();
        // remove already formatted characters
        let string = shirabe_php_shim::preg_replace("/\u{1b}\\[[^m]*m/", "", &string);
        // remove terminal hyperlinks
        let string =
            shirabe_php_shim::preg_replace("/\u{1b}]8;[^;]*;[^\u{1b}]*\u{1b}\\\\/", "", &string);
        formatter.set_decorated(is_decorated);

        string
    }
}
