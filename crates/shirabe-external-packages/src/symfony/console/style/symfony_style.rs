//! ref: composer/vendor/symfony/console/Style/SymfonyStyle.php

use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::runtime_exception::RuntimeException;
use crate::symfony::console::formatter::OutputFormatter;
use crate::symfony::console::formatter::OutputFormatterInterface;
use crate::symfony::console::helper::Helper;
use crate::symfony::console::helper::ProgressBar;
use crate::symfony::console::helper::SymfonyQuestionHelper;
use crate::symfony::console::helper::Table;
use crate::symfony::console::helper::TableCell;
use crate::symfony::console::helper::table::{Cell, Row};
use crate::symfony::console::input::InputInterface;
use crate::symfony::console::output::ConsoleOutputInterface;
use crate::symfony::console::output::OutputInterface;
use crate::symfony::console::output::TrimmedBufferOutput;
use crate::symfony::console::output::console_output::ConsoleOutput;
use crate::symfony::console::output::output_interface::OUTPUT_NORMAL;
use crate::symfony::console::question::ChoiceQuestion;
use crate::symfony::console::question::ConfirmationQuestion;
use crate::symfony::console::question::Question;
use crate::symfony::console::question::QuestionInterface;
use crate::symfony::console::style::output_style::OutputStyle;
use crate::symfony::console::style::style_interface::StyleInterface;
use crate::symfony::console::terminal::Terminal;
use shirabe_php_shim::PhpMixed;

/// Output decorator helpers for the Symfony Style Guide.
#[derive(Debug)]
pub struct SymfonyStyle {
    inner: OutputStyle,
    input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    question_helper: Option<SymfonyQuestionHelper>,
    progress_bar: Option<ProgressBar>,
    line_length: i64,
    buffered_output: TrimmedBufferOutput,
}

pub const MAX_LINE_LENGTH: i64 = 120;

impl SymfonyStyle {
    pub fn new(
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Self {
        let buffered_output = TrimmedBufferOutput::new(
            if std::path::MAIN_SEPARATOR == '\\' {
                4
            } else {
                2
            },
            Some(output.borrow().get_verbosity()),
            false,
            // TODO(plugin): clone of the formatter; PHP `clone $output->getFormatter()`.
            Some(output.borrow().get_formatter()),
        )
        .unwrap();
        // Windows cmd wraps lines as soon as the terminal width is reached, whether there are following chars or not.
        let width = {
            let w = Terminal::new().get_width();
            if w != 0 { w } else { MAX_LINE_LENGTH }
        };
        let line_length = std::cmp::min(
            width - (std::path::MAIN_SEPARATOR == '\\') as i64,
            MAX_LINE_LENGTH,
        );

        let inner = OutputStyle::new(output.clone());

        Self {
            inner,
            input,
            output,
            question_helper: None,
            progress_bar: None,
            line_length,
            buffered_output,
        }
    }

    /// Formats a message as a block of text.
    pub fn block(
        &mut self,
        messages: PhpMixed,
        r#type: Option<&str>,
        style: Option<&str>,
        prefix: &str,
        padding: bool,
        escape: bool,
    ) {
        let messages: Vec<PhpMixed> = if shirabe_php_shim::is_array(&messages) {
            match messages {
                PhpMixed::Array(entries) => entries.into_values().collect(),
                PhpMixed::List(items) => items,
                _ => unreachable!("value is an array past the is_array guard"),
            }
        } else {
            vec![messages]
        };

        self.auto_prepend_block();
        let block = self.create_block(messages, r#type, style, prefix, padding, escape);
        self.writeln(
            PhpMixed::List(block.into_iter().map(PhpMixed::String).collect()),
            OUTPUT_NORMAL,
        );
        self.new_line(1);
    }

    /// Formats a command comment.
    pub fn comment(&mut self, message: PhpMixed) {
        self.block(
            message,
            None,
            None,
            "<fg=default;bg=default> // </>",
            false,
            false,
        );
    }

    /// Formats an info message.
    pub fn info(&mut self, message: PhpMixed) {
        self.block(message, Some("INFO"), Some("fg=green"), " ", true, true);
    }

    /// Formats a horizontal table.
    pub fn horizontal_table(&mut self, headers: Vec<PhpMixed>, rows: Vec<PhpMixed>) {
        self.create_table()
            .set_horizontal(true)
            .set_headers(headers.into_iter().map(Cell::from).collect())
            .set_rows(rows.into_iter().map(Row::from).collect())
            .render();

        self.new_line(1);
    }

    /// Formats a list of key/value horizontally.
    ///
    /// Each row can be one of:
    /// * 'A title'
    /// * ['key' => 'value']
    /// * new TableSeparator()
    pub fn definition_list(&mut self, list: Vec<PhpMixed>) {
        let mut headers: Vec<PhpMixed> = Vec::new();
        let mut row: Vec<PhpMixed> = Vec::new();
        for value in list {
            if Self::is_table_separator(&value) {
                headers.push(value.clone());
                row.push(value);
                continue;
            }
            if shirabe_php_shim::is_string(&value) {
                // TODO: store a `TableCell` (with colspan => 2) into the mixed array.
                let _table_cell = TableCell::new(&Self::php_string(&value), {
                    let mut options = indexmap::IndexMap::new();
                    options.insert(
                        "colspan".to_string(),
                        crate::symfony::console::helper::table_cell::TableCellOption::Int(2),
                    );
                    options
                });
                let _ = _table_cell;
                todo!();
            }
            if !shirabe_php_shim::is_array(&value) {
                // TODO(plugin): recoverable error path.
                let _ = InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: "Value should be an array, string, or an instance of TableSeparator."
                        .to_string(),
                    code: 0,
                });
                todo!()
            }
            // $headers[] = key($value); $row[] = current($value);
            let (first_key, first_value) = match &value {
                PhpMixed::Array(entries) => (
                    entries
                        .keys()
                        .next()
                        .map(|k| PhpMixed::String(k.clone()))
                        .unwrap_or(PhpMixed::Null),
                    entries
                        .values()
                        .next()
                        .cloned()
                        .unwrap_or(PhpMixed::Bool(false)),
                ),
                PhpMixed::List(items) => (
                    if items.is_empty() {
                        PhpMixed::Null
                    } else {
                        PhpMixed::Int(0)
                    },
                    items.first().cloned().unwrap_or(PhpMixed::Bool(false)),
                ),
                _ => unreachable!("value is an array past the is_array guard"),
            };
            headers.push(first_key);
            row.push(first_value);
        }

        self.horizontal_table(headers, vec![PhpMixed::List(row.into_iter().collect())]);
    }

    pub fn progress_iterate(
        &mut self,
        _iterable: Vec<PhpMixed>,
        _max: Option<i64>,
    ) -> Vec<PhpMixed> {
        // TODO(phase-c/d): PHP uses `yield from`; porting the generator semantics of
        // ProgressBar::iterate() requires a streaming design not yet in place.
        todo!()
    }

    pub fn ask_question(&mut self, question: &impl QuestionInterface) -> PhpMixed {
        if self.input.borrow().is_interactive() {
            self.auto_prepend_block();
        }

        if self.question_helper.is_none() {
            self.question_helper = Some(SymfonyQuestionHelper::new());
        }

        // TODO(plugin): pass `self` as the OutputInterface to the question helper.
        let answer = {
            let input = self.input.clone();
            let mut input = input.borrow_mut();
            self.question_helper
                .as_mut()
                .unwrap()
                .ask(&mut *input, self.output.clone(), question)
        };
        // PHP `askQuestion` returns the answer directly; exceptions propagate. Phase B
        // collapses the double `Result` by panicking on either error.
        let answer = answer
            .expect("question helper error")
            .expect("missing input");

        if self.input.borrow().is_interactive() {
            self.new_line(1);
            self.buffered_output
                .write(&["\n".to_string()], false, OUTPUT_NORMAL);
        }

        answer
    }

    /// Returns a new instance which makes use of stderr if available.
    pub fn get_error_style(&self) -> Self {
        Self::new(self.input.clone(), self.inner.get_error_output())
    }

    pub fn create_table(&mut self) -> Table {
        // TODO(plugin): ConsoleOutputInterface::section() requires runtime type info.
        let output = if Self::is_console_output_interface(&self.output) {
            Self::as_console_output_interface(&self.output)
                .unwrap()
                .borrow()
                .section() as std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>
        } else {
            self.output.clone()
        };
        let mut style = Table::get_style_definition("symfony-style-guide".to_string())
            .expect("style definition lookup")
            .expect("undefined style definition");
        style.set_cell_header_format("<info>%s</info>".to_string());

        let mut table = Table::new(output);
        let _ = table.set_style(crate::symfony::console::helper::table::StyleName::Style(
            style,
        ));
        table
    }

    pub fn create_progress_bar(&self, max: i64) -> ProgressBar {
        let mut progress_bar = self.inner.create_progress_bar(max);

        if std::path::MAIN_SEPARATOR != '\\'
            || shirabe_php_shim::getenv("TERM_PROGRAM").as_deref()
                == Some(std::ffi::OsStr::new("Hyper"))
        {
            progress_bar.set_empty_bar_character("░"); // light shade character ░
            progress_bar.set_progress_character("");
            progress_bar.set_bar_character("▓"); // dark shade character ▓
        }

        progress_bar
    }

    fn get_progress_bar(&mut self) -> &mut ProgressBar {
        if self.progress_bar.is_none() {
            // TODO(plugin): recoverable error path.
            let _ = RuntimeException(shirabe_php_shim::RuntimeException {
                message: "The ProgressBar is not started.".to_string(),
                code: 0,
            });
            todo!()
        }

        self.progress_bar.as_mut().unwrap()
    }

    fn auto_prepend_block(&mut self) {
        let chars = shirabe_php_shim::substr(
            &shirabe_php_shim::str_replace(
                shirabe_php_shim::PHP_EOL,
                "\n",
                &self.buffered_output.fetch(),
            ),
            -2,
            None,
        );

        if chars.is_empty() {
            self.new_line(1); // empty history, so we should start with a new line.

            return;
        }
        // Prepend new line for each non LF chars (This means no blank line was output before)
        self.new_line(2 - shirabe_php_shim::substr_count(&chars, "\n"));
    }

    fn auto_prepend_text(&mut self) {
        let fetched = self.buffered_output.fetch();
        // Prepend new line if last char isn't EOL:
        if !shirabe_php_shim::str_ends_with(&fetched, "\n") {
            self.new_line(1);
        }
    }

    fn write_buffer(&mut self, message: &str, new_line: bool, r#type: i64) {
        // We need to know if the last chars are PHP_EOL
        self.buffered_output
            .write(&[message.to_string()], new_line, r#type);
    }

    fn create_block(
        &mut self,
        messages: Vec<PhpMixed>,
        r#type: Option<&str>,
        style: Option<&str>,
        prefix: &str,
        padding: bool,
        escape: bool,
    ) -> Vec<String> {
        let mut indent_length: i64 = 0;
        let prefix_length = Helper::width(&Helper::remove_decoration(
            &mut *self.get_formatter().borrow_mut(),
            prefix,
        ));
        let mut lines: Vec<String> = Vec::new();

        let mut r#type = r#type.map(|t| t.to_string());
        let mut line_indentation = String::new();
        if let Some(t) = &r#type {
            let formatted = format!("[{}] ", t.clone());
            indent_length = shirabe_php_shim::strlen(&formatted);
            line_indentation = shirabe_php_shim::str_repeat(" ", indent_length as usize);
            r#type = Some(formatted);
        }

        let messages_count = messages.len() as i64;
        // wrap and add newlines for each element
        for (key, message) in messages.into_iter().enumerate() {
            let key = key as i64;
            let mut message = Self::php_string(&message);
            if escape {
                message = OutputFormatter::escape(&message).unwrap();
            }

            let decoration_length = Helper::width(&message)
                - Helper::width(&Helper::remove_decoration(
                    &mut *self.get_formatter().borrow_mut(),
                    &message,
                ));
            let message_line_length = std::cmp::min(
                self.line_length - prefix_length - indent_length + decoration_length,
                self.line_length,
            );
            let message_lines = shirabe_php_shim::explode(
                shirabe_php_shim::PHP_EOL,
                &shirabe_php_shim::wordwrap(
                    &message,
                    message_line_length,
                    shirabe_php_shim::PHP_EOL,
                    true,
                ),
            );
            for message_line in message_lines {
                lines.push(message_line);
            }

            if messages_count > 1 && key < messages_count - 1 {
                lines.push(String::new());
            }
        }

        let mut first_line_index: i64 = 0;
        if padding && self.inner.is_decorated() {
            first_line_index = 1;
            shirabe_php_shim::array_unshift(&mut lines, String::new());
            lines.push(String::new());
        }

        for (i, line) in lines.iter_mut().enumerate() {
            let i = i as i64;
            if let Some(t) = &r#type {
                *line = if first_line_index == i {
                    format!("{}{}", t, line)
                } else {
                    format!("{}{}", line_indentation, line)
                };
            }

            *line = format!("{}{}", prefix, line);
            line.push_str(&shirabe_php_shim::str_repeat(
                " ",
                (self.line_length
                    - Helper::width(&Helper::remove_decoration(
                        &mut *self.output.borrow().get_formatter().borrow_mut(),
                        line,
                    )))
                .max(0) as usize,
            ));

            if let Some(style) = style {
                *line = format!("<{}>{}</>", style, line.clone());
            }
        }

        lines
    }

    fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>> {
        self.output.borrow().get_formatter()
    }

    fn is_console_output_interface(
        output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> bool {
        // ConsoleOutput is the only OutputInterface implementor that also implements
        // ConsoleOutputInterface, so `instanceof ConsoleOutputInterface` reduces to this downcast.
        shirabe_php_shim::AsAny::as_any(&*output.borrow())
            .downcast_ref::<ConsoleOutput>()
            .is_some()
    }

    fn as_console_output_interface(
        _output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Option<std::rc::Rc<std::cell::RefCell<dyn ConsoleOutputInterface>>> {
        todo!()
    }

    // TODO(phase-c/d): `PhpMixed` cannot carry a `TableSeparator` object, so the
    // `$value instanceof TableSeparator` check has no faithful representation yet.
    fn is_table_separator(_value: &PhpMixed) -> bool {
        todo!()
    }

    fn php_string(value: &PhpMixed) -> String {
        shirabe_php_shim::strval(value)
    }

    /// Bridges the `StyleInterface` validator (which yields `anyhow::Error`) to the
    /// `Question::set_validator` validator (which yields `InvalidArgumentException`) by
    /// converting any error into an `InvalidArgumentException` carrying its message.
    #[allow(clippy::type_complexity)]
    fn adapt_validator(
        validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) -> Option<Box<dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>>> {
        validator.map(|validator| {
            Box::new(move |value: Option<PhpMixed>| {
                validator(value).map_err(|e| {
                    InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                        message: e.to_string(),
                        code: 0,
                    })
                })
            })
                as Box<dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>>
        })
    }
}

impl SymfonyStyle {
    /// {@inheritdoc}
    pub fn writeln(&mut self, messages: PhpMixed, r#type: i64) {
        let messages: Vec<PhpMixed> = if !shirabe_php_shim::is_iterable(&messages) {
            vec![messages]
        } else {
            match messages {
                PhpMixed::Array(entries) => entries.into_values().collect(),
                PhpMixed::List(items) => items,
                _ => unreachable!("value is iterable past the is_iterable guard"),
            }
        };

        for message in messages {
            let message = Self::php_string(&message);
            self.inner.writeln(std::slice::from_ref(&message), r#type);
            self.write_buffer(&message, true, r#type);
        }
    }

    /// {@inheritdoc}
    pub fn write(&mut self, messages: PhpMixed, newline: bool, r#type: i64) {
        let messages: Vec<PhpMixed> = if !shirabe_php_shim::is_iterable(&messages) {
            vec![messages]
        } else {
            match messages {
                PhpMixed::Array(entries) => entries.into_values().collect(),
                PhpMixed::List(items) => items,
                _ => unreachable!("value is iterable past the is_iterable guard"),
            }
        };

        for message in messages {
            let message = Self::php_string(&message);
            self.inner
                .write(std::slice::from_ref(&message), newline, r#type);
            self.write_buffer(&message, newline, r#type);
        }
    }
}

impl StyleInterface for SymfonyStyle {
    /// {@inheritdoc}
    fn title(&mut self, message: &str) {
        self.auto_prepend_block();
        self.writeln(
            PhpMixed::List(vec![
                PhpMixed::String(format!(
                    "<comment>{}</>",
                    OutputFormatter::escape_trailing_backslash(message),
                )),
                PhpMixed::String(format!(
                    "<comment>{}</>",
                    shirabe_php_shim::str_repeat(
                        "=",
                        Helper::width(&Helper::remove_decoration(
                            &mut *self.get_formatter().borrow_mut(),
                            message,
                        )) as usize,
                    ),
                )),
            ]),
            OUTPUT_NORMAL,
        );
        self.new_line(1);
    }

    /// {@inheritdoc}
    fn section(&mut self, message: &str) {
        self.auto_prepend_block();
        self.writeln(
            PhpMixed::List(vec![
                PhpMixed::String(format!(
                    "<comment>{}</>",
                    OutputFormatter::escape_trailing_backslash(message),
                )),
                PhpMixed::String(format!(
                    "<comment>{}</>",
                    shirabe_php_shim::str_repeat(
                        "-",
                        Helper::width(&Helper::remove_decoration(
                            &mut *self.get_formatter().borrow_mut(),
                            message,
                        )) as usize,
                    ),
                )),
            ]),
            OUTPUT_NORMAL,
        );
        self.new_line(1);
    }

    /// {@inheritdoc}
    fn listing(&mut self, elements: Vec<PhpMixed>) {
        self.auto_prepend_text();
        let elements: Vec<PhpMixed> = shirabe_php_shim::array_map(
            |element: &PhpMixed| PhpMixed::String(format!(" * {}", element.clone())),
            &elements,
        );

        self.writeln(
            PhpMixed::List(elements.into_iter().collect()),
            OUTPUT_NORMAL,
        );
        self.new_line(1);
    }

    /// {@inheritdoc}
    fn text(&mut self, message: PhpMixed) {
        self.auto_prepend_text();

        let messages: Vec<PhpMixed> = if shirabe_php_shim::is_array(&message) {
            match message {
                PhpMixed::Array(entries) => entries.into_values().collect(),
                PhpMixed::List(items) => items,
                _ => unreachable!("value is an array past the is_array guard"),
            }
        } else {
            vec![message]
        };
        for message in messages {
            self.writeln(PhpMixed::String(format!(" {}", message)), OUTPUT_NORMAL);
        }
    }

    /// {@inheritdoc}
    fn success(&mut self, message: PhpMixed) {
        self.block(
            message,
            Some("OK"),
            Some("fg=black;bg=green"),
            " ",
            true,
            true,
        );
    }

    /// {@inheritdoc}
    fn error(&mut self, message: PhpMixed) {
        self.block(
            message,
            Some("ERROR"),
            Some("fg=white;bg=red"),
            " ",
            true,
            true,
        );
    }

    /// {@inheritdoc}
    fn warning(&mut self, message: PhpMixed) {
        self.block(
            message,
            Some("WARNING"),
            Some("fg=black;bg=yellow"),
            " ",
            true,
            true,
        );
    }

    /// {@inheritdoc}
    fn note(&mut self, message: PhpMixed) {
        self.block(message, Some("NOTE"), Some("fg=yellow"), " ! ", false, true);
    }

    /// {@inheritdoc}
    fn caution(&mut self, message: PhpMixed) {
        self.block(
            message,
            Some("CAUTION"),
            Some("fg=white;bg=red"),
            " ! ",
            true,
            true,
        );
    }

    /// {@inheritdoc}
    fn table(&mut self, headers: Vec<PhpMixed>, rows: Vec<PhpMixed>) {
        self.create_table()
            .set_headers(headers.into_iter().map(Cell::from).collect())
            .set_rows(rows.into_iter().map(Row::from).collect())
            .render();

        self.new_line(1);
    }

    /// {@inheritdoc}
    fn ask(
        &mut self,
        question: &str,
        default: Option<&str>,
        validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) -> PhpMixed {
        let mut question = Question::new(
            question.to_string(),
            default.map(|d| PhpMixed::String(d.to_string())),
        );
        question.set_validator(Self::adapt_validator(validator));

        self.ask_question(&question)
    }

    /// {@inheritdoc}
    fn ask_hidden(
        &mut self,
        question: &str,
        validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) -> PhpMixed {
        let mut question = Question::new(question.to_string(), None);

        question.set_hidden(true);
        question.set_validator(Self::adapt_validator(validator));

        self.ask_question(&question)
    }

    /// {@inheritdoc}
    fn confirm(&mut self, question: &str, default: bool) -> bool {
        let answer = self.ask_question(&ConfirmationQuestion::new(
            question.to_string(),
            default,
            "/^y/i".to_string(),
        ));

        shirabe_php_shim::boolval(&answer)
    }

    /// {@inheritdoc}
    fn choice(
        &mut self,
        question: &str,
        choices: Vec<PhpMixed>,
        default: Option<PhpMixed>,
    ) -> PhpMixed {
        let default = if let Some(default) = default {
            let values = shirabe_php_shim::array_flip(&PhpMixed::List(choices.to_vec()));
            // $default = $values[$default] ?? $default;
            let resolved = match &values {
                PhpMixed::Array(map) => map.get(&default.to_string()).cloned(),
                _ => None,
            };
            Some(resolved.unwrap_or(default))
        } else {
            None
        };

        // PHP: return $this->askQuestion(new ChoiceQuestion($question, $choices, $default));
        let choices_map: indexmap::IndexMap<String, PhpMixed> = choices
            .into_iter()
            .enumerate()
            .map(|(i, c)| (i.to_string(), c))
            .collect();
        let choice_question = ChoiceQuestion::new(question.to_string(), choices_map, default)
            .expect("choice() always provides at least one choice");
        self.ask_question(&choice_question)
    }

    /// {@inheritdoc}
    fn new_line(&mut self, count: i64) {
        self.inner.new_line(count);
        self.buffered_output.write(
            &[shirabe_php_shim::str_repeat("\n", count as usize)],
            false,
            OUTPUT_NORMAL,
        );
    }

    /// {@inheritdoc}
    fn progress_start(&mut self, max: i64) {
        let mut progress_bar = self.create_progress_bar(max);
        progress_bar.start(None);
        self.progress_bar = Some(progress_bar);
    }

    /// {@inheritdoc}
    fn progress_advance(&mut self, step: i64) {
        self.get_progress_bar().advance(step);
    }

    /// {@inheritdoc}
    fn progress_finish(&mut self) {
        self.get_progress_bar().finish();
        self.new_line(2);
        self.progress_bar = None;
    }
}
