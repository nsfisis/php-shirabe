//! ref: composer/vendor/symfony/console/Helper/FormatterHelper.php sha1:9150365b39cd8bb9dcd96bf9422e1a2af5ad15a7

use crate::formatter::OutputFormatter;
use crate::helper::{Helper, HelperInterface, HelperSet};
use bstr::{BStr, BString};
use shirabe_php_shim::{bformat, implode, str_repeat};

/// The Formatter class provides helpers to format messages.
#[derive(Debug, Default)]
pub struct FormatterHelper {
    inner: Helper,
}

impl FormatterHelper {
    /// Formats a message within a section.
    pub fn format_section(&self, section: &BStr, message: &BStr, style: &BStr) -> BString {
        bformat!("<{style}>[{section}]</{style}> {message}")
    }

    /// Formats a message as a block of text.
    pub fn format_block(
        &self,
        messages: FormatBlockMessages,
        style: &BStr,
        large: bool,
    ) -> BString {
        let messages = match messages {
            FormatBlockMessages::String(message) => vec![message],
            FormatBlockMessages::Array(messages) => messages,
        };

        let mut len: i64 = 0;
        let mut lines: Vec<BString> = Vec::new();
        for message in &messages {
            let message = OutputFormatter::escape(message).unwrap();
            lines.push(if large {
                bformat!("  {message}  ")
            } else {
                bformat!(" {message} ")
            });
            len = std::cmp::max(Helper::width(&message) + (if large { 4 } else { 2 }), len);
        }

        let mut messages: Vec<BString> = if large {
            vec![str_repeat(" ", len as usize)]
        } else {
            vec![]
        };
        let mut i = 0;
        while i < lines.len() {
            messages.push(bformat!(
                "{}{}",
                lines[i],
                str_repeat(" ", (len - Helper::width(&lines[i])) as usize)
            ));
            i += 1;
        }
        if large {
            messages.push(str_repeat(" ", len as usize));
        }

        let mut i = 0;
        while i < messages.len() {
            messages[i] = bformat!("<{style}>{}</{style}>", messages[i]);
            i += 1;
        }

        implode("\n", &messages)
    }

    /// Truncates a message to the given length.
    pub fn truncate(&self, message: &BStr, length: i64, suffix: &BStr) -> BString {
        let computed_length = length - Helper::width(suffix);

        if computed_length > Helper::width(message) {
            return message.into();
        }

        bformat!("{}{suffix}", Helper::substr(message, 0, Some(length)))
    }
}

impl HelperInterface for FormatterHelper {
    fn set_helper_set(&mut self, helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> BString {
        "formatter".into()
    }
}

/// `formatBlock` accepts either a single string or an array of strings.
#[derive(Debug)]
pub enum FormatBlockMessages {
    String(BString),
    Array(Vec<BString>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::ByteSlice;

    #[test]
    fn block_messages_keep_their_bytes() {
        let block = FormatterHelper::default().format_block(
            FormatBlockMessages::String(BString::from(b"message\xE9".as_slice())),
            "error".into(),
            false,
        );

        assert!(block.contains_str(b"message\xE9"));
    }
}
