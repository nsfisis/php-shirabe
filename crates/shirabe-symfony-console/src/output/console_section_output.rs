//! ref: composer/vendor/symfony/console/Output/ConsoleSectionOutput.php

use crate::formatter::OutputFormatterInterface;
use crate::helper::Helper;
use crate::output::{DoWrite, OutputInterface, StreamOutput};
use crate::terminal::Terminal;
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    PHP_EOL, PhpResource, array_reverse, array_splice, array_unshift, explode, str_replace,
};

type Sections =
    std::rc::Rc<std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<ConsoleSectionOutput>>>>>;

#[derive(Debug)]
pub struct ConsoleSectionOutput {
    inner: StreamOutput,
    content: std::cell::RefCell<Vec<BString>>,
    lines: std::cell::Cell<i64>,
    sections: Sections,
    terminal: Terminal,
}

impl ConsoleSectionOutput {
    /// `$sections` is shared by reference (PHP `array &$sections`); the new instance
    /// is unshifted into it.
    pub fn new(
        stream: PhpResource,
        sections: &Sections,
        verbosity: i64,
        decorated: bool,
        formatter: std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>,
    ) -> std::rc::Rc<std::cell::RefCell<Self>> {
        let inner = StreamOutput::new(stream, Some(verbosity), Some(decorated), Some(formatter))
            .expect("ConsoleSectionOutput stream operation must not fatal")
            .expect("ConsoleSectionOutput stream is valid");

        let this = std::rc::Rc::new(std::cell::RefCell::new(Self {
            inner,
            content: std::cell::RefCell::new(Vec::new()),
            lines: std::cell::Cell::new(0),
            sections: sections.clone(),
            terminal: Terminal::new(),
        }));

        array_unshift(&mut sections.borrow_mut(), this.clone());

        this
    }

    /// Clears previous output for this section.
    ///
    /// `$lines` is the number of lines to clear. If null, then the entire output
    /// of this section is cleared.
    pub fn clear(&self, lines: Option<i64>) {
        if self.content.borrow().is_empty() || !self.is_decorated() {
            return;
        }

        let lines = if let Some(lines) = lines.filter(|l| *l != 0) {
            // Multiply lines by 2 to cater for each new line added between content
            array_splice(
                &mut self.content.borrow_mut(),
                -(lines * 2),
                None,
                Vec::new(),
            );
            lines
        } else {
            let lines = self.lines.get();
            *self.content.borrow_mut() = Vec::new();
            lines
        };

        self.lines.set(self.lines.get() - lines);

        let erased = self.pop_stream_content_until_current_section(lines);
        self.inner.do_write(erased.as_bstr(), false);
    }

    /// Overwrites the previous output with a new message.
    pub fn overwrite(&self, message: &[BString]) {
        self.clear(None);
        self.writeln(message, crate::output::OUTPUT_NORMAL);
    }

    pub fn get_content(&self) -> BString {
        bstr::join("", &*self.content.borrow()).into()
    }

    pub fn add_content(&self, input: &BStr) {
        for line_content in explode(PHP_EOL, input) {
            let count = (self.get_display_length(&line_content) as f64
                / self.terminal.get_width() as f64)
                .ceil();
            self.lines
                .set(self.lines.get() + if count != 0.0 { count as i64 } else { 1 });
            self.content.borrow_mut().push(line_content);
            self.content.borrow_mut().push(PHP_EOL.into());
        }
    }

    /// At initial stage, cursor is at the end of stream output. This method makes cursor crawl upwards until it hits
    /// current section. Then it erases content it crawled through. Optionally, it erases part of current section too.
    ///
    /// `$numberOfLinesToClearFromCurrentSection` defaults to 0 in PHP.
    fn pop_stream_content_until_current_section(
        &self,
        number_of_lines_to_clear_from_current_section: i64,
    ) -> BString {
        let mut number_of_lines_to_clear = number_of_lines_to_clear_from_current_section;
        let mut erased_content: Vec<BString> = Vec::new();

        for section in self.sections.borrow().iter() {
            // PHP: `if ($section === $this) break;` — identity comparison against $this.
            // The current section is the same object stored in the shared list.
            if section.as_ptr() == (self as *const Self).cast_mut() {
                break;
            }

            let section_ref = section.borrow();
            number_of_lines_to_clear += section_ref.lines.get();
            erased_content.push(section_ref.get_content());
        }

        if number_of_lines_to_clear > 0 {
            // move cursor up n lines
            self.inner.do_write(
                format!("\x1b[{}A", number_of_lines_to_clear)
                    .as_bytes()
                    .as_bstr(),
                false,
            );
            // erase to end of screen
            self.inner.do_write("\x1b[0J".into(), false);
        }

        bstr::join("", array_reverse(&erased_content, false)).into()
    }

    fn get_display_length(&self, text: &[u8]) -> i64 {
        Helper::width(Helper::remove_decoration(
            &mut *self.get_formatter().borrow_mut(),
            str_replace("\t", "        ", text),
        ))
    }
}

impl DoWrite for ConsoleSectionOutput {
    fn do_write(&self, message: &BStr, newline: bool) {
        if !self.is_decorated() {
            self.inner.do_write(message, newline);

            return;
        }

        let erased_content = self.pop_stream_content_until_current_section(0);

        self.add_content(message);

        self.inner.do_write(message, true);
        self.inner.do_write(erased_content.as_bstr(), false);
    }
}

impl OutputInterface for ConsoleSectionOutput {
    fn write(&self, messages: &[BString], newline: bool, options: i64) {
        self.inner.inner().write(self, messages, newline, options);
    }
    fn writeln(&self, messages: &[BString], options: i64) {
        self.inner.inner().writeln(self, messages, options);
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
