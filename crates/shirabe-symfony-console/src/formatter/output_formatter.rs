//! ref: composer/vendor/symfony/console/Formatter/OutputFormatter.php

use crate::exception::InvalidArgumentException;
use crate::formatter::output_formatter_interface::OutputFormatterInterface;
use crate::formatter::output_formatter_style::OutputFormatterStyle;
use crate::formatter::output_formatter_style_interface::OutputFormatterStyleInterface;
use crate::formatter::output_formatter_style_stack::OutputFormatterStyleStack;
use crate::formatter::wrappable_output_formatter_interface::WrappableOutputFormatterInterface;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    PregMatches, bconcat, byte_at, explode, implode, ltrim, mb_detect_encoding, php_regex,
    preg_match, preg_match_all, preg_replace, rtrim2, str_repeat, str_replace, strlen, strtolower,
    strtr_array, substr, utf8_text,
};
use shirabe_symfony_string::b;

/// Formatter class for console output.
#[derive(Debug)]
pub struct OutputFormatter {
    decorated: bool,
    styles: indexmap::IndexMap<
        BString,
        std::rc::Rc<std::cell::RefCell<Box<dyn OutputFormatterStyleInterface>>>,
    >,
    style_stack: OutputFormatterStyleStack,
}

impl OutputFormatter {
    /// Escapes "<" and ">" special chars in given text.
    pub fn escape(text: impl AsRef<[u8]>) -> anyhow::Result<BString> {
        let text = preg_replace(php_regex!("/([^\\\\]|^)([<>])/"), "$1\\\\$2", text);

        Ok(Self::escape_trailing_backslash(text))
    }

    /// Escapes trailing "\" in given text.
    pub fn escape_trailing_backslash(text: impl AsRef<[u8]>) -> BString {
        let mut text = BString::from(text.as_ref());
        if text.ends_with(b"\\") {
            let len = strlen(&text);
            text = rtrim2(&text, "\\");
            text = str_replace("\0", "", &text);
            let padding = str_repeat("\0", (len - strlen(&text)) as usize);
            text.extend_from_slice(&padding);
        }

        text
    }

    /// Initializes console output formatter.
    ///
    /// `styles` is an array of "name => FormatterStyle" instances.
    pub fn new(
        decorated: bool,
        styles: indexmap::IndexMap<BString, Box<dyn OutputFormatterStyleInterface>>,
    ) -> Self {
        let mut this = Self {
            decorated,
            styles: indexmap::IndexMap::new(),
            style_stack: OutputFormatterStyleStack::new(None),
        };

        this.set_style(
            "error".into(),
            Box::new(OutputFormatterStyle::new(
                Some("white".into()),
                Some("red".into()),
                vec![],
            )),
        );
        this.set_style(
            "info".into(),
            Box::new(OutputFormatterStyle::new(
                Some("green".into()),
                None,
                vec![],
            )),
        );
        this.set_style(
            "comment".into(),
            Box::new(OutputFormatterStyle::new(
                Some("yellow".into()),
                None,
                vec![],
            )),
        );
        this.set_style(
            "question".into(),
            Box::new(OutputFormatterStyle::new(
                Some("black".into()),
                Some("cyan".into()),
                vec![],
            )),
        );

        for (name, style) in styles {
            this.set_style(name.as_bstr(), style);
        }

        this.style_stack = OutputFormatterStyleStack::new(None);

        this
    }

    pub fn get_style_stack(&self) -> &OutputFormatterStyleStack {
        &self.style_stack
    }

    /// Tries to create new style instance from string.
    fn create_style_from_string(
        &self,
        string: &BStr,
    ) -> anyhow::Result<Option<Box<dyn OutputFormatterStyleInterface>>> {
        if let Some(style) = self.styles.get(string) {
            return Ok(Some(style.borrow().clone_box()));
        }

        let matches: Vec<PregMatches> =
            preg_match_all(php_regex!("/([^=]+)=([^;]+)(;|$)/"), string).collect();
        if matches.is_empty() {
            return Ok(None);
        }

        let mut style = OutputFormatterStyle::new(None, None, vec![]);
        for r#match in &matches {
            let key = strtolower(
                r#match
                    .get(1)
                    .expect("every group participates whenever the pattern matches"),
            );
            let value = r#match
                .get(2)
                .expect("every group participates whenever the pattern matches");

            if key == "fg" {
                style.set_foreground(Some(strtolower(value).as_bstr()));
            } else if key == "bg" {
                style.set_background(Some(strtolower(value).as_bstr()));
            } else if key == "href" {
                let url = preg_replace(php_regex!("{\\\\([<>])}"), "$1", value);
                style.set_href(url);
            } else if key == "options" {
                let value = strtolower(value);
                let options = preg_match_all(php_regex!("([^,;]+)"), &value);
                for option in options {
                    style.set_option(
                        option
                            .get(0)
                            .expect("group 0 participates whenever the pattern matches"),
                    );
                }
            } else {
                return Ok(None);
            }
        }

        Ok(Some(Box::new(style)))
    }

    /// Applies current style from stack to text, if must be applied.
    fn apply_current_style(
        &mut self,
        text: &BStr,
        current: &BStr,
        width: i64,
        current_line_length: &mut i64,
    ) -> BString {
        if text.is_empty() {
            return BString::default();
        }

        if width == 0 {
            return if self.is_decorated() {
                self.style_stack.get_current_mut().apply(text)
            } else {
                BString::from(text)
            };
        }

        let mut text = BString::from(text);

        if *current_line_length == 0 && !current.is_empty() {
            text = ltrim(&text);
        }

        let prefix;
        if *current_line_length != 0 {
            let i = width - *current_line_length;
            prefix = bconcat!(substr(&text, 0, Some(i)), "\n");
            text = substr(&text, i, None);
        } else {
            prefix = BString::default();
        }

        let trailing = BString::from(
            preg_match(php_regex!("~(\\n)$~"), &text)
                .and_then(|matches| matches.get(1))
                .unwrap_or_default(),
        );
        text = bconcat!(prefix, self.add_line_breaks(text.as_bstr(), width));
        text = bconcat!(rtrim2(&text, "\n"), trailing);

        if *current_line_length == 0 && !current.is_empty() && substr(current, -1, None) != "\n" {
            text = bconcat!("\n", text);
        }

        let mut lines = explode("\n", &text);

        for line in &lines {
            *current_line_length += strlen(line);
            if width <= *current_line_length {
                *current_line_length = 0;
            }
        }

        if self.is_decorated() {
            for line in lines.iter_mut() {
                *line = self.style_stack.get_current_mut().apply(line.as_bstr());
            }
        }

        implode("\n", &lines)
    }

    fn add_line_breaks(&self, text: &BStr, width: i64) -> BString {
        let encoding = mb_detect_encoding(text, None, true)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| BString::from("UTF-8"));
        let encoding =
            utf8_text(&encoding).expect("mb_detect_encoding() returns an ASCII encoding name");

        b(text)
            .to_code_point_string(encoding)
            .wordwrap(width, "\n", true)
            .to_byte_string(encoding)
    }
}

impl OutputFormatterInterface for OutputFormatter {
    fn set_decorated(&mut self, decorated: bool) {
        self.decorated = decorated;
    }

    fn is_decorated(&self) -> bool {
        self.decorated
    }

    fn set_style(&mut self, name: &BStr, style: Box<dyn OutputFormatterStyleInterface>) {
        self.styles.insert(
            strtolower(name),
            std::rc::Rc::new(std::cell::RefCell::new(style)),
        );
    }

    fn has_style(&self, name: &BStr) -> bool {
        self.styles.contains_key(strtolower(name).as_bstr())
    }

    fn get_style(
        &self,
        name: &BStr,
    ) -> anyhow::Result<std::rc::Rc<std::cell::RefCell<Box<dyn OutputFormatterStyleInterface>>>>
    {
        if !self.has_style(name) {
            return Err(InvalidArgumentException::new(format!(
                "Undefined style: \"{}\".",
                name.display_lossy()
            ))
            .into());
        }

        Ok(std::rc::Rc::clone(&self.styles[strtolower(name).as_bstr()]))
    }

    fn format(&mut self, message: Option<&BStr>) -> anyhow::Result<Option<BString>> {
        self.format_and_wrap(message, 0)
    }
}

impl WrappableOutputFormatterInterface for OutputFormatter {
    fn format_and_wrap(
        &mut self,
        message: Option<&BStr>,
        width: i64,
    ) -> anyhow::Result<Option<BString>> {
        let message = match message {
            None => return Ok(Some(BString::default())),
            Some(message) => message,
        };

        let mut offset: i64 = 0;
        let mut output = BString::default();
        let open_tag_regex = "[a-z](?:[^\\\\<>]* | \\\\.)*";
        let close_tag_regex = "[a-z][^<>]*";
        let mut current_line_length: i64 = 0;
        let matches = preg_match_all(
            format!("#<(({open_tag_regex}) | /({close_tag_regex})?)>#ix"),
            message,
        );
        for match_ in matches {
            let pos = match_
                .get_offset(0)
                .expect("group 0 participates whenever the pattern matches")
                as i64;
            let text = match_
                .get(0)
                .expect("group 0 participates whenever the pattern matches");

            if pos != 0 && byte_at(message, (pos - 1) as usize) == b'\\' {
                continue;
            }

            // add the text up to the next tag
            let segment = substr(message, offset, Some(pos - offset));
            let applied = self.apply_current_style(
                segment.as_bstr(),
                output.as_bstr(),
                width,
                &mut current_line_length,
            );
            output.extend_from_slice(&applied);
            offset = pos + strlen(text);

            // opening tag?
            let open = byte_at(text, 1) != b'/';
            let tag = if open {
                match_
                    .get(1)
                    .expect("group 1 participates whenever the pattern matches")
            } else {
                match_.get(3).unwrap_or_default()
            };

            if !open && tag.is_empty() {
                // </>
                self.style_stack.pop(None)?.ok();
            } else if let Some(style) = self.create_style_from_string(tag)? {
                if open {
                    self.style_stack.push(style);
                } else {
                    self.style_stack.pop(Some(style))?.ok();
                }
            } else {
                let applied = self.apply_current_style(
                    text,
                    output.as_bstr(),
                    width,
                    &mut current_line_length,
                );
                output.extend_from_slice(&applied);
            }
        }

        let segment = substr(message, offset, None);
        let applied = self.apply_current_style(
            segment.as_bstr(),
            output.as_bstr(),
            width,
            &mut current_line_length,
        );
        output.extend_from_slice(&applied);

        let mut pairs = indexmap::IndexMap::new();
        pairs.insert("\0", "\\");
        pairs.insert("\\<", "<");
        pairs.insert("\\>", ">");
        Ok(Some(strtr_array(&output, &pairs)))
    }
}
