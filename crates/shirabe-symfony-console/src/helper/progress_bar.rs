//! ref: composer/vendor/symfony/console/Helper/ProgressBar.php sha1:240e6cdb125cbeb0137ca60ccf4cc236d5d335c3

use crate::cursor::Cursor;
use crate::exception::LogicException;
use crate::helper::Helper;
use crate::output::{
    ConsoleOutput, ConsoleOutputInterface, ConsoleSectionOutput, OutputInterface, output_interface,
};
use crate::terminal::Terminal;
use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe_php_shim::{
    AsAny, PHP_EOL, PhpMixed, PregMatches, STR_PAD_LEFT, bconcat, explode, microtime, php_str,
    php_to_string, preg_replace_callback, round, rtrim_chars, sprintf, str_pad, str_repeat,
    substr_count, time,
};

pub const FORMAT_VERBOSE: &BStr = php_str(b"verbose");
pub const FORMAT_VERY_VERBOSE: &BStr = php_str(b"very_verbose");
pub const FORMAT_DEBUG: &BStr = php_str(b"debug");
pub const FORMAT_NORMAL: &BStr = php_str(b"normal");

const FORMAT_VERBOSE_NOMAX: &BStr = php_str(b"verbose_nomax");
const FORMAT_VERY_VERBOSE_NOMAX: &BStr = php_str(b"very_verbose_nomax");
const FORMAT_DEBUG_NOMAX: &BStr = php_str(b"debug_nomax");
const FORMAT_NORMAL_NOMAX: &BStr = php_str(b"normal_nomax");

/// A placeholder formatter callable, receiving the bar and the output.
pub type PlaceholderFormatter = Box<
    dyn Fn(
        &ProgressBar,
        &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<Result<PhpMixed, LogicException>>,
>;

/// The ProgressBar provides helpers to display progress output.
#[derive(Debug)]
pub struct ProgressBar {
    bar_width: i64,
    bar_char: Option<BString>,
    empty_bar_char: BString,
    progress_char: BString,
    format: Option<BString>,
    internal_format: Option<BString>,
    redraw_freq: Option<i64>,
    write_count: i64,
    last_write_time: f64,
    min_seconds_between_redraws: f64,
    max_seconds_between_redraws: f64,
    output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    step: i64,
    max: i64,
    start_time: i64,
    step_width: i64,
    percent: f64,
    messages: IndexMap<BString, BString>,
    overwrite: bool,
    terminal: Terminal,
    previous_message: Option<BString>,
    cursor: Cursor,
}

thread_local! {
    static FORMATTERS: std::cell::RefCell<Option<IndexMap<BString, PlaceholderFormatter>>> = const { std::cell::RefCell::new(None) };
    static FORMATS: std::cell::RefCell<Option<IndexMap<BString, BString>>> = const { std::cell::RefCell::new(None) };
}

impl ProgressBar {
    /// `$max` Maximum steps (0 if unknown)
    pub fn new(
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        max: i64,
        min_seconds_between_redraws: f64,
    ) -> Self {
        // PHP: `if ($output instanceof ConsoleOutputInterface) { $output =
        // $output->getErrorOutput(); }`. ConsoleOutput is the only OutputInterface
        // implementor that also implements ConsoleOutputInterface, so the check
        // reduces to a downcast to the concrete type.
        let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> = {
            let redirected = AsAny::as_any(&*output.borrow())
                .downcast_ref::<ConsoleOutput>()
                .map(|console| console.get_error_output());
            redirected.unwrap_or(output)
        };

        let mut this = Self {
            bar_width: 28,
            bar_char: None,
            empty_bar_char: "-".into(),
            progress_char: ">".into(),
            format: None,
            internal_format: None,
            redraw_freq: Some(1),
            write_count: 0,
            last_write_time: 0.0,
            min_seconds_between_redraws: 0.0,
            max_seconds_between_redraws: 1.0,
            output: output.clone(),
            step: 0,
            max: 0,
            start_time: 0,
            step_width: 0,
            percent: 0.0,
            messages: IndexMap::new(),
            overwrite: true,
            terminal: Terminal::new(),
            previous_message: None,
            cursor: Cursor::new(output.clone(), None),
        };

        this.set_max_steps(max);

        if 0.0 < min_seconds_between_redraws {
            this.redraw_freq = None;
            this.min_seconds_between_redraws = min_seconds_between_redraws;
        }

        if !this.output.borrow().is_decorated() {
            // disable overwrite when output does not support ANSI codes.
            this.overwrite = false;

            // set a reasonable redraw frequency so output isn't flooded
            this.redraw_freq = None;
        }

        this.start_time = time();

        this
    }

    /// Sets a placeholder formatter for a given name.
    ///
    /// This method also allow you to override an existing placeholder.
    ///
    /// `$name` The placeholder name (including the delimiter char like %)
    /// `$callable` A PHP callable
    pub fn set_placeholder_formatter_definition(name: &BStr, callable: PlaceholderFormatter) {
        FORMATTERS.with(|formatters| {
            let mut formatters = formatters.borrow_mut();
            if formatters.is_none() {
                *formatters = Some(Self::init_placeholder_formatters());
            }

            formatters
                .as_mut()
                .unwrap()
                .insert(name.to_owned(), callable);
        });
    }

    /// Gets the placeholder formatter for a given name.
    ///
    /// `$name` The placeholder name (including the delimiter char like %)
    pub fn get_placeholder_formatter_definition(name: &BStr) -> Option<()> {
        // Note: the returned callable cannot be cloned out of the thread-local
        // map; call sites invoke the formatter via the map directly.
        FORMATTERS.with(|formatters| {
            let mut formatters = formatters.borrow_mut();
            if formatters.is_none() {
                *formatters = Some(Self::init_placeholder_formatters());
            }

            formatters.as_ref().unwrap().get(name).map(|_| ())
        })
    }

    /// Sets a format for a given name.
    ///
    /// This method also allow you to override an existing format.
    ///
    /// `$name` The format name
    /// `$format` A format string
    pub fn set_format_definition(name: &BStr, format: &BStr) {
        FORMATS.with(|formats| {
            let mut formats = formats.borrow_mut();
            if formats.is_none() {
                *formats = Some(Self::init_formats());
            }

            formats
                .as_mut()
                .unwrap()
                .insert(name.to_owned(), format.to_owned());
        });
    }

    /// Gets the format for a given name.
    ///
    /// `$name` The format name
    pub fn get_format_definition(name: &BStr) -> Option<BString> {
        FORMATS.with(|formats| {
            let mut formats = formats.borrow_mut();
            if formats.is_none() {
                *formats = Some(Self::init_formats());
            }

            formats.as_ref().unwrap().get(name).cloned()
        })
    }

    /// Associates a text with a named placeholder.
    ///
    /// The text is displayed when the progress bar is rendered but only
    /// when the corresponding placeholder is part of the custom format line
    /// (by wrapping the name with %).
    ///
    /// `$message` The text to associate with the placeholder
    /// `$name` The name of the placeholder
    pub fn set_message(&mut self, message: &BStr, name: &BStr) {
        self.messages.insert(name.to_owned(), message.to_owned());
    }

    pub fn get_message(&self, name: &BStr) -> Option<BString> {
        self.messages.get(name).cloned()
    }

    pub fn get_start_time(&self) -> i64 {
        self.start_time
    }

    pub fn get_max_steps(&self) -> i64 {
        self.max
    }

    pub fn get_progress(&self) -> i64 {
        self.step
    }

    fn get_step_width(&self) -> i64 {
        self.step_width
    }

    pub fn get_progress_percent(&self) -> f64 {
        self.percent
    }

    pub fn get_bar_offset(&self) -> f64 {
        f64::floor(if self.max != 0 {
            self.percent * self.bar_width as f64
        } else if self.redraw_freq.is_none() {
            (((self.bar_width / 15).min(5) * self.write_count) % self.bar_width) as f64
        } else {
            (self.step % self.bar_width) as f64
        })
    }

    pub fn get_estimated(&self) -> f64 {
        if self.step == 0 {
            return 0.0;
        }

        round(
            (time() - self.start_time) as f64 / self.step as f64 * self.max as f64,
            0,
        )
    }

    pub fn get_remaining(&self) -> f64 {
        if self.step == 0 {
            return 0.0;
        }

        round(
            (time() - self.start_time) as f64 / self.step as f64 * (self.max - self.step) as f64,
            0,
        )
    }

    pub fn set_bar_width(&mut self, size: i64) {
        self.bar_width = size.max(1);
    }

    pub fn get_bar_width(&self) -> i64 {
        self.bar_width
    }

    pub fn set_bar_character(&mut self, char: &BStr) {
        self.bar_char = Some(char.to_owned());
    }

    pub fn get_bar_character(&self) -> BString {
        match &self.bar_char {
            Some(bar_char) => bar_char.clone(),
            None => {
                if self.max != 0 {
                    "=".into()
                } else {
                    self.empty_bar_char.clone()
                }
            }
        }
    }

    pub fn set_empty_bar_character(&mut self, char: &BStr) {
        self.empty_bar_char = char.to_owned();
    }

    pub fn get_empty_bar_character(&self) -> BString {
        self.empty_bar_char.clone()
    }

    pub fn set_progress_character(&mut self, char: &BStr) {
        self.progress_char = char.to_owned();
    }

    pub fn get_progress_character(&self) -> BString {
        self.progress_char.clone()
    }

    pub fn set_format(&mut self, format: &BStr) {
        self.format = None;
        self.internal_format = Some(format.to_owned());
    }

    /// Sets the redraw frequency.
    ///
    /// `$freq` The frequency in steps
    pub fn set_redraw_frequency(&mut self, freq: Option<i64>) {
        self.redraw_freq = freq.map(|freq| freq.max(1));
    }

    pub fn min_seconds_between_redraws(&mut self, seconds: f64) {
        self.min_seconds_between_redraws = seconds;
    }

    pub fn max_seconds_between_redraws(&mut self, seconds: f64) {
        self.max_seconds_between_redraws = seconds;
    }

    /// Returns an iterator that will automatically update the progress bar when iterated.
    ///
    /// `$max` Number of steps to complete the bar (0 if indeterminate), if null it will be
    /// inferred from `$iterable`
    pub fn iterate(
        &mut self,
        iterable: Vec<(PhpMixed, PhpMixed)>,
        max: Option<i64>,
    ) -> anyhow::Result<Vec<(PhpMixed, PhpMixed)>> {
        self.start(Some(max.unwrap_or({
            // is_countable($iterable) ? \count($iterable) : 0
            iterable.len() as i64
        })))?;

        let mut yielded = Vec::new();
        for (key, value) in iterable {
            yielded.push((key, value));

            self.advance(1)?;
        }

        self.finish()?;

        Ok(yielded)
    }

    /// Starts the progress output.
    ///
    /// `$max` Number of steps to complete the bar (0 if indeterminate), null to leave unchanged
    pub fn start(&mut self, max: Option<i64>) -> anyhow::Result<()> {
        self.start_time = time();
        self.step = 0;
        self.percent = 0.0;

        if let Some(max) = max {
            self.set_max_steps(max);
        }

        self.display()
    }

    /// Advances the progress output X steps.
    ///
    /// `$step` Number of steps to advance
    pub fn advance(&mut self, step: i64) -> anyhow::Result<()> {
        self.set_progress(self.step + step)
    }

    /// Sets whether to overwrite the progressbar, false for new line.
    pub fn set_overwrite(&mut self, overwrite: bool) {
        self.overwrite = overwrite;
    }

    pub fn set_progress(&mut self, mut step: i64) -> anyhow::Result<()> {
        if self.max != 0 && step > self.max {
            self.max = step;
        } else if step < 0 {
            step = 0;
        }

        let redraw_freq = match self.redraw_freq {
            Some(redraw_freq) => redraw_freq as f64,
            None => (if self.max != 0 { self.max } else { 10 }) as f64 / 10.0,
        };
        let prev_period = (self.step as f64 / redraw_freq) as i64;
        let curr_period = (step as f64 / redraw_freq) as i64;
        self.step = step;
        self.percent = if self.max != 0 {
            self.step as f64 / self.max as f64
        } else {
            0.0
        };
        let time_interval = microtime() - self.last_write_time;

        // Draw regardless of other limits
        if self.max == step {
            self.display()?;

            return Ok(());
        }

        // Throttling
        if time_interval < self.min_seconds_between_redraws {
            return Ok(());
        }

        // Draw each step period, but not too late
        if prev_period != curr_period || time_interval >= self.max_seconds_between_redraws {
            self.display()?;
        }

        Ok(())
    }

    pub fn set_max_steps(&mut self, max: i64) {
        self.format = None;
        self.max = max.max(0);
        self.step_width = if self.max != 0 {
            Helper::width(bconcat!(self.max))
        } else {
            4
        };
    }

    /// Finishes the progress output.
    pub fn finish(&mut self) -> anyhow::Result<()> {
        if self.max == 0 {
            self.max = self.step;
        }

        if self.step == self.max && !self.overwrite {
            // prevent double 100% output
            return Ok(());
        }

        self.set_progress(self.max)
    }

    /// Outputs the current progress string.
    pub fn display(&mut self) -> anyhow::Result<()> {
        if output_interface::VERBOSITY_QUIET == self.output.borrow().get_verbosity() {
            return Ok(());
        }

        if self.format.is_none() {
            let format = match &self.internal_format {
                Some(internal_format) if !internal_format.is_empty() => internal_format.clone(),
                _ => self.determine_best_format().into(),
            };
            self.set_real_format(format.as_ref());
        }

        let line = self.build_line()?;
        self.overwrite(&line);

        Ok(())
    }

    /// Removes the progress bar from the current line.
    ///
    /// This is useful if you wish to write some output
    /// while a progress bar is running.
    /// Call display() to show the progress bar again.
    pub fn clear(&mut self) -> anyhow::Result<()> {
        if !self.overwrite {
            return Ok(());
        }

        if self.format.is_none() {
            let format = match &self.internal_format {
                Some(internal_format) if !internal_format.is_empty() => internal_format.clone(),
                _ => self.determine_best_format().into(),
            };
            self.set_real_format(format.as_ref());
        }

        self.overwrite("");

        Ok(())
    }

    fn set_real_format(&mut self, format: &BStr) {
        let format_nomax = bconcat!(format, "_nomax");
        // try to use the _nomax variant if available
        if self.max == 0 && Self::get_format_definition(format_nomax.as_ref()).is_some() {
            self.format = Self::get_format_definition(format_nomax.as_ref());
        } else if Self::get_format_definition(format).is_some() {
            self.format = Self::get_format_definition(format);
        } else {
            self.format = Some(format.to_owned());
        }
    }

    /// Overwrites a previous message to the output.
    fn overwrite(&mut self, message: impl AsRef<[u8]>) {
        let message = message.as_ref();
        if self.previous_message.as_ref().map(|m| m.as_slice()) == Some(message) {
            return;
        }

        let original_message = BString::from(message);
        let mut message = BString::from(message);

        if self.overwrite {
            if let Some(previous_message) = self.previous_message.clone() {
                // PHP: `$this->output instanceof ConsoleSectionOutput`. Downcast the
                // shared output handle to the concrete section type.
                let output_ref = self.output.borrow();
                if let Some(section) =
                    AsAny::as_any(&*output_ref).downcast_ref::<ConsoleSectionOutput>()
                {
                    let message_lines = explode("\n", &previous_message);
                    let mut line_count = message_lines.len() as i64;
                    for message_line in &message_lines {
                        let formatter = section.get_formatter();
                        let message_line_length = Helper::width(Helper::remove_decoration(
                            &mut *formatter.borrow_mut(),
                            message_line,
                        ));
                        if message_line_length > self.terminal.get_width() {
                            line_count += (message_line_length as f64
                                / self.terminal.get_width() as f64)
                                .floor() as i64;
                        }
                    }
                    section.clear(Some(line_count));
                } else {
                    drop(output_ref);
                    let line_count = substr_count(&previous_message, "\n");
                    for _i in 0..line_count {
                        self.cursor.move_to_column(1);
                        self.cursor.clear_line();
                        self.cursor.move_up(1);
                    }

                    self.cursor.move_to_column(1);
                    self.cursor.clear_line();
                }
            }
        } else if self.step > 0 {
            message = bconcat!(PHP_EOL, message);
        }

        self.previous_message = Some(original_message);
        self.last_write_time = microtime();

        self.output
            .borrow()
            .write(&[message], false, output_interface::OUTPUT_NORMAL);
        self.write_count += 1;
    }

    fn determine_best_format(&self) -> &'static BStr {
        match self.output.borrow().get_verbosity() {
            // OutputInterface::VERBOSITY_QUIET: display is disabled anyway
            output_interface::VERBOSITY_VERBOSE => {
                if self.max != 0 {
                    FORMAT_VERBOSE
                } else {
                    FORMAT_VERBOSE_NOMAX
                }
            }
            output_interface::VERBOSITY_VERY_VERBOSE => {
                if self.max != 0 {
                    FORMAT_VERY_VERBOSE
                } else {
                    FORMAT_VERY_VERBOSE_NOMAX
                }
            }
            output_interface::VERBOSITY_DEBUG => {
                if self.max != 0 {
                    FORMAT_DEBUG
                } else {
                    FORMAT_DEBUG_NOMAX
                }
            }
            _ => {
                if self.max != 0 {
                    FORMAT_NORMAL
                } else {
                    FORMAT_NORMAL_NOMAX
                }
            }
        }
    }

    fn init_placeholder_formatters() -> IndexMap<BString, PlaceholderFormatter> {
        let mut formatters: IndexMap<BString, PlaceholderFormatter> = IndexMap::new();

        formatters.insert(
            "bar".into(),
            Box::new(
                |bar: &ProgressBar,
                 output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                    let complete_bars = bar.get_bar_offset();
                    let mut display = str_repeat(bar.get_bar_character(), complete_bars as usize);
                    if complete_bars < bar.get_bar_width() as f64 {
                        let empty_bars = bar.get_bar_width() as f64
                            - complete_bars
                            - Helper::length(Helper::remove_decoration(
                                &mut *output.borrow().get_formatter().borrow_mut(),
                                bar.get_progress_character(),
                            )) as f64;
                        display.extend_from_slice(&bar.get_progress_character());
                        display.extend_from_slice(&str_repeat(
                            bar.get_empty_bar_character(),
                            empty_bars as usize,
                        ));
                    }

                    Ok(Ok(PhpMixed::string(display)))
                },
            ),
        );

        formatters.insert(
            "elapsed".into(),
            Box::new(
                |bar: &ProgressBar,
                 _output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                    Ok(Ok(PhpMixed::string(
                        Helper::format_time((time() - bar.get_start_time()) as f64)
                            .unwrap_or_default(),
                    )))
                },
            ),
        );

        formatters.insert(
            "remaining".into(),
            Box::new(|bar: &ProgressBar, _output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                if bar.get_max_steps() == 0 {
                    return Ok(Err(LogicException::new("Unable to display the remaining time if the maximum number of steps is not set.")));
                }

                Ok(Ok(PhpMixed::string(Helper::format_time(bar.get_remaining()).unwrap_or_default())))
            }),
        );

        formatters.insert(
            "estimated".into(),
            Box::new(|bar: &ProgressBar, _output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                if bar.get_max_steps() == 0 {
                    return Ok(Err(LogicException::new("Unable to display the estimated time if the maximum number of steps is not set.")));
                }

                Ok(Ok(PhpMixed::string(Helper::format_time(bar.get_estimated()).unwrap_or_default())))
            }),
        );

        formatters.insert(
            "memory".into(),
            Box::new(
                |_bar: &ProgressBar,
                 _output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                    Ok(Ok(PhpMixed::string(
                        match shirabe_php_rpc::memory_get_usage(true) {
                            Ok(memory) => Helper::format_memory(memory),
                            Err(_) => BString::from("<unknown>"),
                        },
                    )))
                },
            ),
        );

        formatters.insert(
            "current".into(),
            Box::new(
                |bar: &ProgressBar,
                 _output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                    Ok(Ok(PhpMixed::string(str_pad(
                        bconcat!(bar.get_progress()),
                        bar.get_step_width() as usize,
                        " ",
                        STR_PAD_LEFT,
                    ))))
                },
            ),
        );

        formatters.insert(
            "max".into(),
            Box::new(
                |bar: &ProgressBar,
                 _output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                    Ok(Ok(PhpMixed::Int(bar.get_max_steps())))
                },
            ),
        );

        formatters.insert(
            "percent".into(),
            Box::new(
                |bar: &ProgressBar,
                 _output: &std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>| {
                    Ok(Ok(PhpMixed::Float(
                        (bar.get_progress_percent() * 100.0).floor(),
                    )))
                },
            ),
        );

        formatters
    }

    fn init_formats() -> IndexMap<BString, BString> {
        let mut formats: IndexMap<BString, BString> = IndexMap::new();

        formats.insert(
            FORMAT_NORMAL.into(),
            " %current%/%max% [%bar%] %percent:3s%%".into(),
        );
        formats.insert(FORMAT_NORMAL_NOMAX.into(), " %current% [%bar%]".into());

        formats.insert(
            FORMAT_VERBOSE.into(),
            " %current%/%max% [%bar%] %percent:3s%% %elapsed:6s%".into(),
        );
        formats.insert(
            FORMAT_VERBOSE_NOMAX.into(),
            " %current% [%bar%] %elapsed:6s%".into(),
        );

        formats.insert(
            FORMAT_VERY_VERBOSE.into(),
            " %current%/%max% [%bar%] %percent:3s%% %elapsed:6s%/%estimated:-6s%".into(),
        );
        formats.insert(
            FORMAT_VERY_VERBOSE_NOMAX.into(),
            " %current% [%bar%] %elapsed:6s%".into(),
        );

        formats.insert(
            FORMAT_DEBUG.into(),
            " %current%/%max% [%bar%] %percent:3s%% %elapsed:6s%/%estimated:-6s% %memory:6s%"
                .into(),
        );
        formats.insert(
            FORMAT_DEBUG_NOMAX.into(),
            " %current% [%bar%] %elapsed:6s% %memory:6s%".into(),
        );

        formats
    }

    fn build_line(&mut self) -> anyhow::Result<BString> {
        let regex = BStr::new("{%([a-z\\-_]+)(?:\\:([^%]+))?%}i");

        // The callback resolves a placeholder match into its replacement text.
        // It is invoked by preg_replace_callback over $this->format.
        let line = self.build_line_apply(regex)?;

        // gets string length for each sub line with multiline format
        let lines_length: Vec<i64> = explode("\n", &line)
            .iter()
            .map(|sub_line| {
                Helper::width(Helper::remove_decoration(
                    &mut *self.output.borrow().get_formatter().borrow_mut(),
                    rtrim_chars(sub_line, "\r"),
                ))
            })
            .collect();

        let lines_width = *lines_length.iter().max().unwrap();

        let terminal_width = self.terminal.get_width();
        if lines_width <= terminal_width {
            return Ok(line);
        }

        self.set_bar_width(self.bar_width - lines_width + terminal_width);

        self.build_line_apply(regex)
    }

    /// Applies the placeholder-resolving callback over `$this->format`, mirroring
    /// the `preg_replace_callback` invocation in PHP's `buildLine()`.
    fn build_line_apply(&self, regex: &BStr) -> anyhow::Result<BString> {
        let format = self.format.clone().unwrap_or_default();

        // $callback in PHP, expressed as a closure over $this and the matches.
        let callback = |matches: &PregMatches| -> anyhow::Result<BString> {
            let name = matches.get(1).unwrap_or_default();

            let text: PhpMixed = if Self::get_placeholder_formatter_definition(name).is_some() {
                // $text = $formatter($this, $this->output);
                let formatter_result = FORMATTERS.with(|formatters| {
                    let formatters = formatters.borrow();
                    let formatter = formatters.as_ref().unwrap().get(name).unwrap();
                    formatter(self, &self.output)
                });
                formatter_result??
            } else if let Some(message) = self.messages.get(name) {
                PhpMixed::string(message.clone())
            } else {
                return Ok(matches.get(0).unwrap_or_default().into());
            };

            if let Some(modifier) = matches.get(2) {
                return Ok(sprintf(bconcat!("%", modifier), &[text]));
            }

            // PHP implicitly casts the formatter result to string here.
            Ok(php_to_string(&text))
        };

        preg_replace_callback(regex, callback, &format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::BufferedOutput;
    use bstr::ByteSlice;

    #[test]
    fn format_message_and_characters_keep_their_bytes() {
        let output = std::rc::Rc::new(std::cell::RefCell::new(BufferedOutput::new(
            None, false, None,
        )));
        let mut bar = ProgressBar::new(output.clone(), 2, 0.0);
        bar.set_format(BStr::new(b"f\xE9 %m% %bar%"));
        bar.set_message(BStr::new(b"m\xE9"), "m".into());
        bar.set_bar_character(BStr::new(b"\xE9"));
        bar.set_bar_width(2);
        bar.start(None).unwrap();
        bar.advance(2).unwrap();

        assert!(
            output
                .borrow()
                .fetch()
                .contains_str(b"f\xE9 m\xE9 \xE9\xE9")
        );
    }
}
