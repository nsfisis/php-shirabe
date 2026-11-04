//! ref: composer/vendor/symfony/console/Helper/Helper.php sha1:842cbf486c0917ae3262d8ec4da53ac0dd09b0cb

use crate::formatter::OutputFormatterInterface;
use crate::helper::HelperSet;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    PhpMixed, bconcat, mb_detect_encoding, mb_strlen, mb_strwidth, mb_substr, php_regex,
    php_to_string, preg_is_match, preg_replace, sprintf, strlen, substr, trigger_deprecation,
    utf8_text,
};
use shirabe_symfony_string::unicode_string::UnicodeString;

/// Helper is the base class for all helper classes.
#[derive(Debug, Default)]
pub struct Helper {
    helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>,
}

impl Helper {
    pub fn set_helper_set(
        &mut self,
        helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>,
    ) {
        self.helper_set = helper_set;
    }

    pub fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>> {
        self.helper_set.clone()
    }

    /// Returns the length of a string, using mb_strwidth if it is available.
    pub fn strlen(string: impl AsRef<[u8]>) -> i64 {
        trigger_deprecation(
            "symfony/console",
            "5.3",
            "Method \"%s()\" is deprecated and will be removed in Symfony 6.0. Use Helper::width() or Helper::length() instead.",
            "Helper::strlen",
        );

        Self::width(string)
    }

    /// Returns the width of a string, using mb_strwidth if it is available.
    /// The width is how many characters positions the string will use.
    pub fn width(string: impl AsRef<[u8]>) -> i64 {
        let string = string.as_ref();
        if preg_is_match(php_regex!("//u"), string) {
            let string = utf8_text(string).expect("a string matching //u is valid UTF-8");
            return UnicodeString::new(string).width(false);
        }

        let encoding = mb_detect_encoding(string, None, true);
        let encoding = match encoding {
            Some(encoding) => encoding,
            None => return strlen(string),
        };

        mb_strwidth(string, Some(encoding.as_bstr()))
    }

    /// Returns the length of a string, using mb_strlen if it is available.
    /// The length is related to how many bytes the string will use.
    pub fn length(string: impl AsRef<[u8]>) -> i64 {
        let string = string.as_ref();
        if preg_is_match(php_regex!("//u"), string) {
            let string = utf8_text(string).expect("a string matching //u is valid UTF-8");
            return UnicodeString::new(string).length();
        }

        let encoding = mb_detect_encoding(string, None, true);
        let encoding = match encoding {
            Some(encoding) => encoding,
            None => return strlen(string),
        };

        mb_strlen(string, &encoding)
    }

    /// Returns the subset of a string, using mb_substr if it is available.
    pub fn substr(string: impl AsRef<[u8]>, from: i64, length: Option<i64>) -> BString {
        let string = string.as_ref();
        let encoding = mb_detect_encoding(string, None, true);
        let encoding = match encoding {
            Some(encoding) => encoding,
            None => return substr(string, from, length),
        };

        mb_substr(string, from, length, Some(encoding.as_bstr()))
    }

    pub fn format_time(secs: f64) -> Option<BString> {
        // [threshold, label, divisor?]
        let time_formats: [(f64, &BStr, Option<f64>); 9] = [
            (0.0, BStr::new("< 1 sec"), None),
            (1.0, BStr::new("1 sec"), None),
            (2.0, BStr::new("secs"), Some(1.0)),
            (60.0, BStr::new("1 min"), None),
            (120.0, BStr::new("mins"), Some(60.0)),
            (3600.0, BStr::new("1 hr"), None),
            (7200.0, BStr::new("hrs"), Some(3600.0)),
            (86400.0, BStr::new("1 day"), None),
            (172800.0, BStr::new("days"), Some(86400.0)),
        ];

        for (index, format) in time_formats.iter().enumerate() {
            if secs >= format.0
                && ((index + 1 < time_formats.len() && secs < time_formats[index + 1].0)
                    || index == time_formats.len() - 1)
            {
                match format.2 {
                    None => return Some(format.1.to_owned()),
                    Some(divisor) => {
                        return Some(bconcat!(
                            php_to_string(&PhpMixed::Float((secs / divisor).floor())),
                            " ",
                            format.1
                        ));
                    }
                }
            }
        }

        None
    }

    pub fn format_memory(memory: i64) -> BString {
        if memory >= 1024 * 1024 * 1024 {
            return sprintf(
                "%.1f GiB",
                &[PhpMixed::Float(memory as f64 / 1024.0 / 1024.0 / 1024.0)],
            );
        }

        if memory >= 1024 * 1024 {
            return sprintf(
                "%.1f MiB",
                &[PhpMixed::Float(memory as f64 / 1024.0 / 1024.0)],
            );
        }

        if memory >= 1024 {
            return sprintf("%d KiB", &[PhpMixed::Int(memory / 1024)]);
        }

        sprintf("%d B", &[PhpMixed::Int(memory)])
    }

    pub fn strlen_without_decoration(
        formatter: &mut dyn OutputFormatterInterface,
        string: impl AsRef<[u8]>,
    ) -> i64 {
        trigger_deprecation(
            "symfony/console",
            "5.3",
            "Method \"%s()\" is deprecated and will be removed in Symfony 6.0. Use Helper::removeDecoration() instead.",
            "Helper::strlenWithoutDecoration",
        );

        Self::width(Self::remove_decoration(formatter, string))
    }

    pub fn remove_decoration(
        formatter: &mut dyn OutputFormatterInterface,
        string: impl AsRef<[u8]>,
    ) -> BString {
        let is_decorated = formatter.is_decorated();
        formatter.set_decorated(false);
        // remove <...> formatting
        let string = formatter
            .format(Some(string.as_ref().as_bstr()))
            .unwrap()
            .unwrap_or_default();
        // remove already formatted characters
        let string = preg_replace(php_regex!("/\u{1b}\\[[^m]*m/"), "", &string);
        // remove terminal hyperlinks
        let string = preg_replace(
            php_regex!("/\u{1b}]8;[^;]*;[^\u{1b}]*\u{1b}\\\\/"),
            "",
            &string,
        );
        formatter.set_decorated(is_decorated);

        string
    }
}
