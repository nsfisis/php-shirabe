//! ref: composer/vendor/symfony/console/Helper/FormatterHelper.php

use crate::symfony::console::formatter::output_formatter::OutputFormatter;
use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::helper::helper_interface::HelperInterface;
use crate::symfony::console::helper::helper_set::HelperSet;
use std::cell::RefCell;
use std::rc::Rc;

/// The Formatter class provides helpers to format messages.
#[derive(Debug, Default)]
pub struct FormatterHelper {
    inner: Helper,
}

impl FormatterHelper {
    /// Formats a message within a section.
    pub fn format_section(&self, section: &str, message: &str, style: &str) -> String {
        format!("<{}>[{}]</{}> {}", style, section, style, message,)
    }

    /// Formats a message as a block of text.
    ///
    /// @param string|array $messages The message to write in the block
    pub fn format_block(&self, messages: FormatBlockMessages, style: &str, large: bool) -> String {
        let messages = match messages {
            FormatBlockMessages::String(message) => vec![message],
            FormatBlockMessages::Array(messages) => messages,
        };

        let mut len: i64 = 0;
        let mut lines: Vec<String> = Vec::new();
        for message in &messages {
            let message = OutputFormatter::escape(message).unwrap();
            lines.push(if large {
                format!("  {}  ", message)
            } else {
                format!(" {} ", message)
            });
            len = std::cmp::max(Helper::width(&message) + (if large { 4 } else { 2 }), len);
        }

        let mut messages: Vec<String> = if large {
            vec![shirabe_php_shim::str_repeat(" ", len as usize)]
        } else {
            vec![]
        };
        let mut i = 0;
        while i < lines.len() {
            messages.push(format!(
                "{}{}",
                lines[i],
                shirabe_php_shim::str_repeat(" ", (len - Helper::width(&lines[i])) as usize)
            ));
            i += 1;
        }
        if large {
            messages.push(shirabe_php_shim::str_repeat(" ", len as usize));
        }

        let mut i = 0;
        while i < messages.len() {
            messages[i] = format!("<{}>{}</{}>", style, messages[i].clone(), style,);
            i += 1;
        }

        messages.join("\n")
    }

    /// Truncates a message to the given length.
    pub fn truncate(&self, message: &str, length: i64, suffix: &str) -> String {
        let computed_length = length - Helper::width(suffix);

        if computed_length > Helper::width(message) {
            return message.to_string();
        }

        format!("{}{}", Helper::substr(message, 0, Some(length)), suffix)
    }
}

impl HelperInterface for FormatterHelper {
    fn set_helper_set(&mut self, helper_set: Option<Rc<RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> String {
        "formatter".to_string()
    }
}

/// `formatBlock` accepts either a single string or an array of strings.
#[derive(Debug)]
pub enum FormatBlockMessages {
    String(String),
    Array(Vec<String>),
}
