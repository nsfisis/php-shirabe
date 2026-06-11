use crate::symfony::console::formatter::OutputFormatterInterface;
use crate::symfony::console::helper::Helper;
use crate::symfony::console::output::OutputInterface;
use crate::symfony::console::output::output::DoWrite;
use crate::symfony::console::output::stream_output::StreamOutput;
use crate::symfony::console::terminal::Terminal;

type Sections =
    std::rc::Rc<std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<ConsoleSectionOutput>>>>>;

#[derive(Debug)]
pub struct ConsoleSectionOutput {
    inner: StreamOutput,
    content: std::cell::RefCell<Vec<String>>,
    lines: std::cell::Cell<i64>,
    sections: Sections,
    terminal: Terminal,
}

impl ConsoleSectionOutput {
    /// `$sections` is shared by reference (PHP `array &$sections`); the new instance
    /// is unshifted into it.
    pub fn new(
        stream: shirabe_php_shim::PhpResource,
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

        shirabe_php_shim::array_unshift(&mut sections.borrow_mut(), this.clone());

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
            shirabe_php_shim::array_splice(
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
        self.inner.do_write(&erased, false);
    }

    /// Overwrites the previous output with a new message.
    pub fn overwrite(&self, message: &[String]) {
        self.clear(None);
        self.writeln(
            message,
            crate::symfony::console::output::output_interface::OUTPUT_NORMAL,
        );
    }

    pub fn get_content(&self) -> String {
        self.content.borrow().join("")
    }

    /// @internal
    pub fn add_content(&self, input: &str) {
        for line_content in shirabe_php_shim::explode(shirabe_php_shim::PHP_EOL, input) {
            let count = shirabe_php_shim::ceil(
                self.get_display_length(&line_content) as f64 / self.terminal.get_width() as f64,
            );
            self.lines
                .set(self.lines.get() + if count != 0.0 { count as i64 } else { 1 });
            self.content.borrow_mut().push(line_content);
            self.content
                .borrow_mut()
                .push(shirabe_php_shim::PHP_EOL.to_string());
        }
    }

    /// At initial stage, cursor is at the end of stream output. This method makes cursor crawl upwards until it hits
    /// current section. Then it erases content it crawled through. Optionally, it erases part of current section too.
    ///
    /// `$numberOfLinesToClearFromCurrentSection` defaults to 0 in PHP.
    fn pop_stream_content_until_current_section(
        &self,
        number_of_lines_to_clear_from_current_section: i64,
    ) -> String {
        let mut number_of_lines_to_clear = number_of_lines_to_clear_from_current_section;
        let mut erased_content: Vec<String> = Vec::new();

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
                &shirabe_php_shim::sprintf(
                    "\x1b[%dA",
                    &[shirabe_php_shim::PhpMixed::Int(number_of_lines_to_clear)],
                ),
                false,
            );
            // erase to end of screen
            self.inner.do_write("\x1b[0J", false);
        }

        shirabe_php_shim::array_reverse(&erased_content, false).join("")
    }

    fn get_display_length(&self, text: &str) -> i64 {
        Helper::width(&Helper::remove_decoration(
            &mut *self.get_formatter().borrow_mut(),
            &shirabe_php_shim::str_replace("\t", "        ", text),
        ))
    }
}

impl DoWrite for ConsoleSectionOutput {
    fn do_write(&self, message: &str, newline: bool) {
        if !self.is_decorated() {
            self.inner.do_write(message, newline);

            return;
        }

        let erased_content = self.pop_stream_content_until_current_section(0);

        self.add_content(message);

        self.inner.do_write(message, true);
        self.inner.do_write(&erased_content, false);
    }
}

impl OutputInterface for ConsoleSectionOutput {
    fn write(&self, messages: &[String], newline: bool, options: i64) {
        self.inner.inner().write(self, messages, newline, options);
    }
    fn writeln(&self, messages: &[String], options: i64) {
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
