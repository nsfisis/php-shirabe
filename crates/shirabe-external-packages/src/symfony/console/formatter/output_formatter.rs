use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::formatter::output_formatter_interface::OutputFormatterInterface;
use crate::symfony::console::formatter::output_formatter_style::OutputFormatterStyle;
use crate::symfony::console::formatter::output_formatter_style_interface::OutputFormatterStyleInterface;
use crate::symfony::console::formatter::output_formatter_style_stack::OutputFormatterStyleStack;
use crate::symfony::console::formatter::wrappable_output_formatter_interface::WrappableOutputFormatterInterface;
use crate::symfony::string::b;

/// Formatter class for console output.
#[derive(Debug)]
pub struct OutputFormatter {
    decorated: bool,
    styles: indexmap::IndexMap<String, Box<dyn OutputFormatterStyleInterface>>,
    style_stack: OutputFormatterStyleStack,
}

impl OutputFormatter {
    /// Escapes "<" and ">" special chars in given text.
    pub fn escape(text: &str) -> anyhow::Result<String> {
        let text = shirabe_php_shim::preg_replace("/([^\\\\]|^)([<>])/", "$1\\\\$2", text)
            .expect("preg_replace failed");

        Ok(Self::escape_trailing_backslash(&text))
    }

    /// Escapes trailing "\" in given text.
    pub fn escape_trailing_backslash(text: &str) -> String {
        let mut text = text.to_string();
        if shirabe_php_shim::str_ends_with(&text, "\\") {
            let len = shirabe_php_shim::strlen(&text);
            text = shirabe_php_shim::rtrim(&text, Some("\\"));
            text = shirabe_php_shim::str_replace("\0", "", &text);
            text.push_str(&shirabe_php_shim::str_repeat(
                "\0",
                (len - shirabe_php_shim::strlen(&text)) as usize,
            ));
        }

        text
    }

    /// Initializes console output formatter.
    ///
    /// `styles` is an array of "name => FormatterStyle" instances.
    pub fn new(
        decorated: bool,
        styles: indexmap::IndexMap<String, Box<dyn OutputFormatterStyleInterface>>,
    ) -> Self {
        let mut this = Self {
            decorated,
            styles: indexmap::IndexMap::new(),
            style_stack: OutputFormatterStyleStack::new(None),
        };

        this.set_style(
            "error",
            Box::new(OutputFormatterStyle::new(
                Some("white"),
                Some("red"),
                vec![],
            )),
        );
        this.set_style(
            "info",
            Box::new(OutputFormatterStyle::new(Some("green"), None, vec![])),
        );
        this.set_style(
            "comment",
            Box::new(OutputFormatterStyle::new(Some("yellow"), None, vec![])),
        );
        this.set_style(
            "question",
            Box::new(OutputFormatterStyle::new(
                Some("black"),
                Some("cyan"),
                vec![],
            )),
        );

        for (name, style) in styles {
            this.set_style(&name, style);
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
        string: &str,
    ) -> anyhow::Result<Option<Box<dyn OutputFormatterStyleInterface>>> {
        if let Some(style) = self.styles.get(string) {
            return Ok(Some(style.clone_box()));
        }

        let mut matches: Vec<Vec<String>> = vec![];
        if shirabe_php_shim::preg_match_all_set_order(
            "/([^=]+)=([^;]+)(;|$)/",
            string,
            &mut matches,
        )? == 0
        {
            return Ok(None);
        }

        let mut style = OutputFormatterStyle::new(None, None, vec![]);
        for r#match in &matches {
            let mut r#match: Vec<String> = r#match.clone();
            shirabe_php_shim::array_shift(&mut r#match);
            r#match[0] = shirabe_php_shim::strtolower(&r#match[0]);

            if r#match[0] == "fg" {
                style.set_foreground(Some(&shirabe_php_shim::strtolower(&r#match[1])));
            } else if r#match[0] == "bg" {
                style.set_background(Some(&shirabe_php_shim::strtolower(&r#match[1])));
            } else if r#match[0] == "href" {
                let url = shirabe_php_shim::preg_replace("{\\\\([<>])}", "$1", &r#match[1])
                    .expect("preg_replace failed");
                style.set_href(&url);
            } else if r#match[0] == "options" {
                let mut options: Vec<Vec<String>> = vec![];
                shirabe_php_shim::preg_match_all_simple(
                    "([^,;]+)",
                    &shirabe_php_shim::strtolower(&r#match[1]),
                    &mut options,
                )?;
                let options = shirabe_php_shim::array_shift(&mut options).unwrap_or_default();
                for option in &options {
                    style.set_option(option);
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
        text: &str,
        current: &str,
        width: i64,
        current_line_length: &mut i64,
    ) -> String {
        if text.is_empty() {
            return String::new();
        }

        if width == 0 {
            return if self.is_decorated() {
                self.style_stack.get_current_mut().apply(text)
            } else {
                text.to_string()
            };
        }

        let mut text = text.to_string();

        if *current_line_length == 0 && !current.is_empty() {
            text = shirabe_php_shim::ltrim(&text, None);
        }

        let prefix;
        if *current_line_length != 0 {
            let i = width - *current_line_length;
            prefix = format!("{}\n", shirabe_php_shim::substr(&text, 0, Some(i)));
            text = shirabe_php_shim::substr(&text, i, None);
        } else {
            prefix = String::new();
        }

        let mut matches: Vec<Option<String>> = vec![];
        shirabe_php_shim::preg_match("~(\\n)$~", &text, &mut matches);
        text = format!("{}{}", prefix, self.add_line_breaks(&text, width));
        let trailing = matches.get(1).and_then(|m| m.clone()).unwrap_or_default();
        text = format!("{}{}", shirabe_php_shim::rtrim(&text, Some("\n")), trailing);

        if *current_line_length == 0
            && !current.is_empty()
            && shirabe_php_shim::substr(current, -1, None) != "\n"
        {
            text = format!("\n{text}");
        }

        let mut lines = shirabe_php_shim::explode("\n", &text);

        for line in &lines {
            *current_line_length += shirabe_php_shim::strlen(line);
            if width <= *current_line_length {
                *current_line_length = 0;
            }
        }

        if self.is_decorated() {
            for line in lines.iter_mut() {
                *line = self.style_stack.get_current_mut().apply(line);
            }
        }

        shirabe_php_shim::implode("\n", &lines)
    }

    fn add_line_breaks(&self, text: &str, width: i64) -> String {
        let encoding = shirabe_php_shim::mb_detect_encoding(text, None, true)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "UTF-8".to_string());

        b(text)
            .to_code_point_string(&encoding)
            .wordwrap(width, "\n", true)
            .to_byte_string(&encoding)
    }
}

impl OutputFormatterInterface for OutputFormatter {
    fn set_decorated(&mut self, decorated: bool) {
        self.decorated = decorated;
    }

    fn is_decorated(&self) -> bool {
        self.decorated
    }

    fn set_style(&mut self, name: &str, style: Box<dyn OutputFormatterStyleInterface>) {
        self.styles
            .insert(shirabe_php_shim::strtolower(name), style);
    }

    fn has_style(&self, name: &str) -> bool {
        self.styles
            .contains_key(&shirabe_php_shim::strtolower(name))
    }

    fn get_style(&self, name: &str) -> anyhow::Result<Box<dyn OutputFormatterStyleInterface>> {
        if !self.has_style(name) {
            return Err(anyhow::anyhow!(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: format!(
                        "Undefined style: \"{}\".",
                        shirabe_php_shim::PhpMixed::String(name.to_string()),
                    ),
                    code: 0,
                },
            )));
        }

        // PHP returns the shared style instance; ownership cannot be expressed without Clone on
        // the trait object.
        // TODO(human-review): returning a shared style here needs an Rc/Clone strategy in Phase C.
        let _ = &self.styles[&shirabe_php_shim::strtolower(name)];
        todo!()
    }

    fn format(&mut self, message: Option<&str>) -> anyhow::Result<Option<String>> {
        self.format_and_wrap(message, 0)
    }
}

impl WrappableOutputFormatterInterface for OutputFormatter {
    fn format_and_wrap(
        &mut self,
        message: Option<&str>,
        width: i64,
    ) -> anyhow::Result<Option<String>> {
        let message = match message {
            None => return Ok(Some(String::new())),
            Some(message) => message,
        };

        let mut offset: i64 = 0;
        let mut output = String::new();
        // Accurate PCRE patterns (possessive quantifiers `*+`), unsupported by the
        // `regex` crate:
        //   let open_tag_regex = "[a-z](?:[^\\\\<>]*+ | \\\\.)*";
        //   let close_tag_regex = "[a-z][^<>]*+";
        // TODO(phase-c): restore the possessive quantifiers once a PCRE-compatible
        // engine is available; greedy quantifiers match the same tags here but may
        // differ in pathological backtracking cases.
        let open_tag_regex = "[a-z](?:[^\\\\<>]* | \\\\.)*";
        let close_tag_regex = "[a-z][^<>]*";
        let mut current_line_length: i64 = 0;
        let mut matches: shirabe_php_shim::PregOffsetCaptureMatches = Default::default();
        shirabe_php_shim::preg_match_all_offset_capture(
            &format!("#<(({open_tag_regex}) | /({close_tag_regex})?)>#ix"),
            message,
            &mut matches,
        )?;
        let count = matches.group(0).len();
        for i in 0..count {
            let (text, pos) = matches.group(0)[i].clone();
            let pos = pos as i64;

            if pos != 0 && shirabe_php_shim::byte_at(message, (pos - 1) as usize) == b'\\' {
                continue;
            }

            // add the text up to the next tag
            let segment = shirabe_php_shim::substr(message, offset, Some(pos - offset));
            let applied =
                self.apply_current_style(&segment, &output, width, &mut current_line_length);
            output.push_str(&applied);
            offset = pos + shirabe_php_shim::strlen(&text);

            // opening tag?
            let open = shirabe_php_shim::byte_at(&text, 1) != b'/';
            let tag = if open {
                matches.group(1)[i].0.clone()
            } else {
                matches
                    .group(3)
                    .get(i)
                    .map(|m| m.0.clone())
                    .unwrap_or_default()
            };

            if !open && tag.is_empty() {
                // </>
                self.style_stack.pop(None)?.ok();
            } else if let Some(style) = self.create_style_from_string(&tag)? {
                if open {
                    self.style_stack.push(style);
                } else {
                    self.style_stack.pop(Some(style))?.ok();
                }
            } else {
                let applied =
                    self.apply_current_style(&text, &output, width, &mut current_line_length);
                output.push_str(&applied);
            }
        }

        let segment = shirabe_php_shim::substr(message, offset, None);
        let applied = self.apply_current_style(&segment, &output, width, &mut current_line_length);
        output.push_str(&applied);

        let mut pairs = indexmap::IndexMap::new();
        pairs.insert("\0".to_string(), "\\".to_string());
        pairs.insert("\\<".to_string(), "<".to_string());
        pairs.insert("\\>".to_string(), ">".to_string());
        Ok(Some(shirabe_php_shim::strtr_array(&output, &pairs)))
    }
}
