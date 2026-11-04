//! ref: composer/vendor/symfony/console/Style/SymfonyStyle.php sha1:7f214d121b04636b9508de280456273d709bab8b

use crate::formatter::{OutputFormatter, OutputFormatterInterface};
use crate::helper::{Cell, Helper, QuestionHelperInterface, Row, SymfonyQuestionHelper, Table};
use crate::input::{InputInterface, InputValue};
use crate::output::{
    ConsoleOutput, ConsoleOutputInterface, OUTPUT_NORMAL, OutputInterface, TrimmedBufferOutput,
};
use crate::question::{ConfirmationQuestion, QuestionInterface};
use crate::style::output_style::OutputStyle;
use crate::style::style_interface::StyleInterface;
use crate::terminal::Terminal;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    AsAny, PHP_EOL, array_unshift, bconcat, explode, str_repeat, str_replace, strlen, substr,
    substr_count, wordwrap,
};

/// Output decorator helpers for the Symfony Style Guide.
#[derive(Debug)]
pub struct SymfonyStyle {
    inner: OutputStyle,
    input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    question_helper: std::cell::RefCell<Option<SymfonyQuestionHelper>>,
    line_length: i64,
    buffered_output: TrimmedBufferOutput,
    /// The handle this style is built in, passed on wherever PHP passes `$this`.
    this: std::rc::Weak<std::cell::RefCell<SymfonyStyle>>,
}

pub const MAX_LINE_LENGTH: i64 = 120;

impl SymfonyStyle {
    pub fn new(
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> std::rc::Rc<std::cell::RefCell<Self>> {
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

        std::rc::Rc::new_cyclic(|this| {
            std::cell::RefCell::new(Self {
                inner,
                input,
                output,
                question_helper: std::cell::RefCell::new(None),
                line_length,
                buffered_output,
                this: this.clone(),
            })
        })
    }

    /// Formats a message as a block of text.
    pub fn block(
        &self,
        messages: &[BString],
        r#type: Option<&BStr>,
        style: Option<&BStr>,
        prefix: &BStr,
        padding: bool,
        escape: bool,
    ) {
        self.auto_prepend_block();
        let block = self.create_block(messages, r#type, style, prefix, padding, escape);
        self.writeln(&block, OUTPUT_NORMAL);
        self.new_line(1);
    }

    pub fn ask_question(&self, question: &impl QuestionInterface) -> InputValue {
        if self.input.borrow().is_interactive() {
            self.auto_prepend_block();
        }

        let this: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = self
            .this
            .upgrade()
            .expect("SymfonyStyle is only reachable through its own handle");
        let answer = {
            let mut question_helper = self.question_helper.borrow_mut();
            let question_helper = question_helper.get_or_insert_with(SymfonyQuestionHelper::new);
            let input = self.input.clone();
            let mut input = input.borrow_mut();
            question_helper.ask(&mut *input, this, question)
        };
        // PHP `askQuestion` returns the answer directly; exceptions propagate. The double
        // `Result` is collapsed here by panicking on either error.
        let answer = answer
            .expect("question helper error")
            .expect("missing input");

        if self.input.borrow().is_interactive() {
            self.new_line(1);
            self.buffered_output
                .write(&["\n".into()], false, OUTPUT_NORMAL);
        }

        answer
    }

    pub fn create_table(&self) -> Table {
        let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> =
            if Self::is_console_output_interface(&self.output) {
                Self::as_console_output_interface(&self.output)
                    .unwrap()
                    .section()
            } else {
                self.output.clone()
            };
        let mut style = Table::get_style_definition("symfony-style-guide".into())
            .expect("style definition lookup")
            .expect("undefined style definition");
        style.set_cell_header_format("<info>%s</info>".into());

        let mut table = Table::new(output);
        let _ = table.set_style(crate::helper::StyleName::Style(style));
        table
    }

    fn auto_prepend_block(&self) {
        let chars = substr(
            str_replace(PHP_EOL, "\n", self.buffered_output.fetch()),
            -2,
            None,
        );

        if chars.is_empty() {
            self.new_line(1); // empty history, so we should start with a new line.

            return;
        }
        // Prepend new line for each non LF chars (This means no blank line was output before)
        self.new_line(2 - substr_count(&chars, "\n"));
    }

    fn write_buffer(&self, message: &BStr, new_line: bool, r#type: i64) {
        // We need to know if the last chars are PHP_EOL
        self.buffered_output
            .write(&[BString::from(message)], new_line, r#type);
    }

    fn create_block(
        &self,
        messages: &[BString],
        r#type: Option<&BStr>,
        style: Option<&BStr>,
        prefix: &BStr,
        padding: bool,
        escape: bool,
    ) -> Vec<BString> {
        let mut indent_length: i64 = 0;
        let prefix_length = Helper::width(Helper::remove_decoration(
            &mut *self.get_formatter().borrow_mut(),
            prefix,
        ));
        let mut lines: Vec<BString> = Vec::new();

        let mut r#type = r#type.map(BString::from);
        let mut line_indentation = BString::default();
        if let Some(t) = &r#type {
            let formatted = bconcat!("[", t, "] ");
            indent_length = strlen(&formatted);
            line_indentation = str_repeat(" ", indent_length as usize);
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
                - Helper::width(Helper::remove_decoration(
                    &mut *self.get_formatter().borrow_mut(),
                    &message,
                ));
            let message_line_length = std::cmp::min(
                self.line_length - prefix_length - indent_length + decoration_length,
                self.line_length,
            );
            let message_lines = explode(
                PHP_EOL,
                wordwrap(&message, message_line_length, PHP_EOL, true),
            );
            for message_line in message_lines {
                lines.push(message_line);
            }

            if messages_count > 1 && key < messages_count - 1 {
                lines.push(BString::default());
            }
        }

        let mut first_line_index: i64 = 0;
        if padding && self.inner.is_decorated() {
            first_line_index = 1;
            array_unshift(&mut lines, BString::default());
            lines.push(BString::default());
        }

        for (i, line) in lines.iter_mut().enumerate() {
            let i = i as i64;
            if let Some(t) = &r#type {
                *line = if first_line_index == i {
                    bconcat!(t, &*line)
                } else {
                    bconcat!(line_indentation, &*line)
                };
            }

            *line = bconcat!(prefix, &*line);
            let padding = str_repeat(
                " ",
                (self.line_length
                    - Helper::width(Helper::remove_decoration(
                        &mut *self.output.borrow().get_formatter().borrow_mut(),
                        &line,
                    )))
                .max(0) as usize,
            );
            line.extend_from_slice(&padding);

            if let Some(style) = style {
                *line = bconcat!("<", style, ">", &*line, "</>");
            }
        }

        lines
    }

    fn is_console_output_interface(
        output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> bool {
        // ConsoleOutput is the only OutputInterface implementor that also implements
        // ConsoleOutputInterface, so `instanceof ConsoleOutputInterface` reduces to this downcast.
        AsAny::as_any(&*output.borrow())
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
}

impl OutputInterface for SymfonyStyle {
    fn write(&self, messages: &[BString], newline: bool, r#type: i64) {
        for message in messages {
            self.inner
                .write(std::slice::from_ref(message), newline, r#type);
            self.write_buffer(message.as_bstr(), newline, r#type);
        }
    }

    fn writeln(&self, messages: &[BString], r#type: i64) {
        for message in messages {
            self.inner.writeln(std::slice::from_ref(message), r#type);
            self.write_buffer(message.as_bstr(), true, r#type);
        }
    }

    fn set_verbosity(&self, level: i64) {
        self.inner.set_verbosity(level);
    }

    fn get_verbosity(&self) -> i64 {
        self.inner.get_verbosity()
    }

    fn is_quiet(&self) -> bool {
        self.inner.is_quiet()
    }

    fn is_verbose(&self) -> bool {
        self.inner.is_verbose()
    }

    fn is_very_verbose(&self) -> bool {
        self.inner.is_very_verbose()
    }

    fn is_debug(&self) -> bool {
        self.inner.is_debug()
    }

    fn set_decorated(&self, decorated: bool) {
        self.inner.set_decorated(decorated);
    }

    fn is_decorated(&self) -> bool {
        self.inner.is_decorated()
    }

    fn set_formatter(
        &self,
        formatter: std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>,
    ) {
        self.inner.set_formatter(formatter);
    }

    fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>> {
        self.inner.get_formatter()
    }
}

impl StyleInterface for SymfonyStyle {
    fn error(&self, message: &[BString]) {
        self.block(
            message,
            Some("ERROR".into()),
            Some("fg=white;bg=red".into()),
            " ".into(),
            true,
            true,
        );
    }

    fn table(&self, headers: Vec<Cell>, rows: Vec<Row>) -> anyhow::Result<()> {
        self.create_table()
            .set_headers(headers)
            .set_rows(rows)
            .render()?;

        self.new_line(1);

        Ok(())
    }

    fn confirm(&self, question: &BStr, default: bool) -> bool {
        let answer = self.ask_question(&ConfirmationQuestion::new(question, default, "/^y/i"));

        answer.to_bool()
    }

    fn new_line(&self, count: i64) {
        self.inner.new_line(count);
        self.buffered_output
            .write(&[str_repeat("\n", count as usize)], false, OUTPUT_NORMAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exception::InvalidArgumentException;
    use crate::input::{ArrayInput, StreamableInputInterface};
    use crate::output::BufferedOutput;
    use crate::question::Question;
    use shirabe_php_shim::{fopen, fwrite, rewind};

    #[test]
    fn ask_question_reports_validation_errors_through_the_style() {
        let stream = fopen("php://memory", "r+").unwrap();
        fwrite(&stream, "bad\ngood\n", None);
        rewind(&stream);
        let mut input = ArrayInput::new(vec![], None).unwrap();
        input.set_stream(stream);
        let input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(input));
        let output = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
            None, false, None,
        )));
        let output_handle: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = output.clone();

        let mut question = Question::new("Value?", InputValue::Null);
        question.set_validator(Some(Box::new(|answer: InputValue| {
            if answer.as_string() == Some(BStr::new("good")) {
                Ok(answer)
            } else {
                Err(InvalidArgumentException::new("Not good"))
            }
        })));

        let style = SymfonyStyle::new(input, output_handle);
        let answer = style.borrow().ask_question(&question);

        assert_eq!(answer.as_string(), Some(BStr::new("good")));
        assert!(output.borrow().fetch().contains_str("[ERROR] Not good"));
    }
}
