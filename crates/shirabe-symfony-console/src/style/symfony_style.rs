//! ref: composer/vendor/symfony/console/Style/SymfonyStyle.php

use crate::formatter::OutputFormatter;
use crate::formatter::OutputFormatterInterface;
use crate::helper::Helper;
use crate::helper::QuestionHelperInterface;
use crate::helper::SymfonyQuestionHelper;
use crate::helper::Table;
use crate::helper::{Cell, Row};
use crate::input::InputInterface;
use crate::output::ConsoleOutput;
use crate::output::ConsoleOutputInterface;
use crate::output::OUTPUT_NORMAL;
use crate::output::OutputInterface;
use crate::output::TrimmedBufferOutput;
use crate::question::ConfirmationQuestion;
use crate::question::QuestionInterface;
use crate::style::output_style::OutputStyle;
use crate::style::style_interface::StyleInterface;
use crate::terminal::Terminal;
use shirabe_php_shim::PhpMixed;

/// Output decorator helpers for the Symfony Style Guide.
#[derive(Debug)]
pub struct SymfonyStyle {
    inner: OutputStyle,
    input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    question_helper: Option<SymfonyQuestionHelper>,
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
            if cfg!(windows) { 4 } else { 2 },
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
        let line_length = std::cmp::min(width - cfg!(windows) as i64, MAX_LINE_LENGTH);

        let inner = OutputStyle::new(output.clone());

        Self {
            inner,
            input,
            output,
            question_helper: None,
            line_length,
            buffered_output,
        }
    }

    /// Formats a message as a block of text.
    pub fn block(
        &mut self,
        messages: &[String],
        r#type: Option<&str>,
        style: Option<&str>,
        prefix: &str,
        padding: bool,
        escape: bool,
    ) {
        self.auto_prepend_block();
        let block = self.create_block(messages, r#type, style, prefix, padding, escape);
        self.writeln(&block, OUTPUT_NORMAL);
        self.new_line(1);
    }

    pub fn ask_question(&mut self, question: &impl QuestionInterface) -> PhpMixed {
        if self.input.borrow().is_interactive() {
            self.auto_prepend_block();
        }

        if self.question_helper.is_none() {
            self.question_helper = Some(SymfonyQuestionHelper::new());
        }

        // TODO(symfony): PHP passes `$this` as the OutputInterface, so SymfonyQuestionHelper's
        // write_error renders through SymfonyStyle::error; SymfonyStyle is not an OutputInterface
        // trait object here, so the raw output is passed instead.
        let answer = {
            let input = self.input.clone();
            let mut input = input.borrow_mut();
            self.question_helper
                .as_mut()
                .unwrap()
                .ask(&mut *input, self.output.clone(), question)
        };
        // PHP `askQuestion` returns the answer directly; exceptions propagate. The double
        // `Result` is collapsed here by panicking on either error.
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

    pub fn create_table(&mut self) -> Table {
        let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> =
            if Self::is_console_output_interface(&self.output) {
                Self::as_console_output_interface(&self.output)
                    .unwrap()
                    .section()
            } else {
                self.output.clone()
            };
        let mut style = Table::get_style_definition("symfony-style-guide".to_string())
            .expect("style definition lookup")
            .expect("undefined style definition");
        style.set_cell_header_format("<info>%s</info>".to_string());

        let mut table = Table::new(output);
        let _ = table.set_style(crate::helper::StyleName::Style(style));
        table
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

    fn write_buffer(&mut self, message: &str, new_line: bool, r#type: i64) {
        // We need to know if the last chars are PHP_EOL
        self.buffered_output
            .write(&[message.to_string()], new_line, r#type);
    }

    fn create_block(
        &mut self,
        messages: &[String],
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
        for (key, message) in messages.iter().enumerate() {
            let key = key as i64;
            let mut message = message.clone();
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

    /// PHP casts to `ConsoleOutputInterface`; `ConsoleOutput` being its only implementor, a
    /// borrow of the concrete type serves as the cast result.
    fn as_console_output_interface(
        output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> Option<std::cell::Ref<'_, ConsoleOutput>> {
        std::cell::Ref::filter_map(output.borrow(), |output| {
            output.as_any().downcast_ref::<ConsoleOutput>()
        })
        .ok()
    }

    pub fn writeln(&mut self, messages: &[String], r#type: i64) {
        for message in messages {
            self.inner.writeln(std::slice::from_ref(message), r#type);
            self.write_buffer(message, true, r#type);
        }
    }
}

impl StyleInterface for SymfonyStyle {
    fn error(&mut self, message: &[String]) {
        self.block(
            message,
            Some("ERROR"),
            Some("fg=white;bg=red"),
            " ",
            true,
            true,
        );
    }

    fn table(&mut self, headers: Vec<Cell>, rows: Vec<Row>) {
        self.create_table()
            .set_headers(headers)
            .set_rows(rows)
            .render();

        self.new_line(1);
    }

    fn confirm(&mut self, question: &str, default: bool) -> bool {
        let answer = self.ask_question(&ConfirmationQuestion::new(
            question.to_string(),
            default,
            "/^y/i".to_string(),
        ));

        shirabe_php_shim::boolval(&answer)
    }

    fn new_line(&mut self, count: i64) {
        self.inner.new_line(count);
        self.buffered_output.write(
            &[shirabe_php_shim::str_repeat("\n", count as usize)],
            false,
            OUTPUT_NORMAL,
        );
    }
}
