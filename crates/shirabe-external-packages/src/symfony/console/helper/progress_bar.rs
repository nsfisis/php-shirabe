//! ref: composer/vendor/symfony/console/Helper/ProgressBar.php

use crate::symfony::console::cursor::Cursor;
use crate::symfony::console::exception::logic_exception::LogicException;
use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::output::OutputInterface;
use crate::symfony::console::output::output_interface;
use crate::symfony::console::terminal::Terminal;
use indexmap::IndexMap;
use std::cell::RefCell;
use std::rc::Rc;

pub const FORMAT_VERBOSE: &str = "verbose";
pub const FORMAT_VERY_VERBOSE: &str = "very_verbose";
pub const FORMAT_DEBUG: &str = "debug";
pub const FORMAT_NORMAL: &str = "normal";

const FORMAT_VERBOSE_NOMAX: &str = "verbose_nomax";
const FORMAT_VERY_VERBOSE_NOMAX: &str = "very_verbose_nomax";
const FORMAT_DEBUG_NOMAX: &str = "debug_nomax";
const FORMAT_NORMAL_NOMAX: &str = "normal_nomax";

/// A placeholder formatter callable, receiving the bar and the output.
pub type PlaceholderFormatter = Box<
    dyn Fn(
        &ProgressBar,
        &Rc<RefCell<dyn OutputInterface>>,
    ) -> anyhow::Result<Result<shirabe_php_shim::PhpMixed, LogicException>>,
>;

/// The ProgressBar provides helpers to display progress output.
#[derive(Debug)]
pub struct ProgressBar {
    bar_width: i64,
    bar_char: Option<String>,
    empty_bar_char: String,
    progress_char: String,
    format: Option<String>,
    internal_format: Option<String>,
    redraw_freq: Option<i64>,
    write_count: i64,
    last_write_time: f64,
    min_seconds_between_redraws: f64,
    max_seconds_between_redraws: f64,
    output: Rc<RefCell<dyn OutputInterface>>,
    step: i64,
    max: i64,
    start_time: i64,
    step_width: i64,
    percent: f64,
    messages: IndexMap<String, String>,
    overwrite: bool,
    terminal: Terminal,
    previous_message: Option<String>,
    cursor: Cursor,
}

thread_local! {
    static FORMATTERS: RefCell<Option<IndexMap<String, PlaceholderFormatter>>> = const { RefCell::new(None) };
    static FORMATS: RefCell<Option<IndexMap<String, String>>> = const { RefCell::new(None) };
}

impl ProgressBar {
    /// `$max` Maximum steps (0 if unknown)
    pub fn new(
        output: Rc<RefCell<dyn OutputInterface>>,
        max: i64,
        min_seconds_between_redraws: f64,
    ) -> Self {
        let output = if todo!("$output instanceof ConsoleOutputInterface") {
            todo!("$output = $output->getErrorOutput();")
        } else {
            output
        };

        let mut this = Self {
            bar_width: 28,
            bar_char: None,
            empty_bar_char: "-".to_string(),
            progress_char: ">".to_string(),
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

        this.start_time = shirabe_php_shim::time();

        this
    }

    /// Sets a placeholder formatter for a given name.
    ///
    /// This method also allow you to override an existing placeholder.
    ///
    /// `$name` The placeholder name (including the delimiter char like %)
    /// `$callable` A PHP callable
    pub fn set_placeholder_formatter_definition(name: &str, callable: PlaceholderFormatter) {
        FORMATTERS.with(|formatters| {
            let mut formatters = formatters.borrow_mut();
            if formatters.is_none() {
                *formatters = Some(Self::init_placeholder_formatters());
            }

            formatters
                .as_mut()
                .unwrap()
                .insert(name.to_string(), callable);
        });
    }

    /// Gets the placeholder formatter for a given name.
    ///
    /// `$name` The placeholder name (including the delimiter char like %)
    pub fn get_placeholder_formatter_definition(name: &str) -> Option<()> {
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
    pub fn set_format_definition(name: &str, format: &str) {
        FORMATS.with(|formats| {
            let mut formats = formats.borrow_mut();
            if formats.is_none() {
                *formats = Some(Self::init_formats());
            }

            formats
                .as_mut()
                .unwrap()
                .insert(name.to_string(), format.to_string());
        });
    }

    /// Gets the format for a given name.
    ///
    /// `$name` The format name
    pub fn get_format_definition(name: &str) -> Option<String> {
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
    pub fn set_message(&mut self, message: &str, name: &str) {
        self.messages.insert(name.to_string(), message.to_string());
    }

    pub fn get_message(&self, name: &str) -> Option<String> {
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

        shirabe_php_shim::round(
            (shirabe_php_shim::time() - self.start_time) as f64 / self.step as f64
                * self.max as f64,
            0,
        )
    }

    pub fn get_remaining(&self) -> f64 {
        if self.step == 0 {
            return 0.0;
        }

        shirabe_php_shim::round(
            (shirabe_php_shim::time() - self.start_time) as f64 / self.step as f64
                * (self.max - self.step) as f64,
            0,
        )
    }

    pub fn set_bar_width(&mut self, size: i64) {
        self.bar_width = size.max(1);
    }

    pub fn get_bar_width(&self) -> i64 {
        self.bar_width
    }

    pub fn set_bar_character(&mut self, char: &str) {
        self.bar_char = Some(char.to_string());
    }

    pub fn get_bar_character(&self) -> String {
        match &self.bar_char {
            Some(bar_char) => bar_char.clone(),
            None => {
                if self.max != 0 {
                    "=".to_string()
                } else {
                    self.empty_bar_char.clone()
                }
            }
        }
    }

    pub fn set_empty_bar_character(&mut self, char: &str) {
        self.empty_bar_char = char.to_string();
    }

    pub fn get_empty_bar_character(&self) -> String {
        self.empty_bar_char.clone()
    }

    pub fn set_progress_character(&mut self, char: &str) {
        self.progress_char = char.to_string();
    }

    pub fn get_progress_character(&self) -> String {
        self.progress_char.clone()
    }

    pub fn set_format(&mut self, format: &str) {
        self.format = None;
        self.internal_format = Some(format.to_string());
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
        iterable: Vec<(shirabe_php_shim::PhpMixed, shirabe_php_shim::PhpMixed)>,
        max: Option<i64>,
    ) -> anyhow::Result<Vec<(shirabe_php_shim::PhpMixed, shirabe_php_shim::PhpMixed)>> {
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
        self.start_time = shirabe_php_shim::time();
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
        let time_interval = shirabe_php_shim::microtime(true) - self.last_write_time;

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
            Helper::width(&self.max.to_string())
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
                _ => self.determine_best_format().to_string(),
            };
            self.set_real_format(&format);
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
                _ => self.determine_best_format().to_string(),
            };
            self.set_real_format(&format);
        }

        self.overwrite("");

        Ok(())
    }

    fn set_real_format(&mut self, format: &str) {
        // try to use the _nomax variant if available
        if self.max == 0 && Self::get_format_definition(&format!("{format}_nomax")).is_some() {
            self.format = Self::get_format_definition(&format!("{format}_nomax"));
        } else if Self::get_format_definition(format).is_some() {
            self.format = Self::get_format_definition(format);
        } else {
            self.format = Some(format.to_string());
        }
    }

    /// Overwrites a previous message to the output.
    fn overwrite(&mut self, message: &str) {
        if self.previous_message.as_deref() == Some(message) {
            return;
        }

        let original_message = message.to_string();
        let mut message = message.to_string();

        if self.overwrite {
            if let Some(previous_message) = self.previous_message.clone() {
                if todo!("$this->output instanceof ConsoleSectionOutput") {
                    let message_lines = shirabe_php_shim::explode("\n", &previous_message);
                    let mut line_count = message_lines.len() as i64;
                    for message_line in &message_lines {
                        let message_line_length = Helper::width(&Helper::remove_decoration(
                            todo!("$this->output->getFormatter()"),
                            message_line,
                        ));
                        if message_line_length > self.terminal.get_width() {
                            line_count += (message_line_length as f64
                                / self.terminal.get_width() as f64)
                                .floor() as i64;
                        }
                    }
                    todo!("$this->output->clear($lineCount); (ConsoleSectionOutput)");
                } else {
                    let line_count = shirabe_php_shim::substr_count(&previous_message, "\n");
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
            message = format!("{}{}", shirabe_php_shim::PHP_EOL, message);
        }

        self.previous_message = Some(original_message);
        self.last_write_time = shirabe_php_shim::microtime(true);

        self.output
            .borrow()
            .write(&[message], false, output_interface::OUTPUT_NORMAL);
        self.write_count += 1;
    }

    fn determine_best_format(&self) -> &'static str {
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

    fn init_placeholder_formatters() -> IndexMap<String, PlaceholderFormatter> {
        let mut formatters: IndexMap<String, PlaceholderFormatter> = IndexMap::new();

        formatters.insert(
            "bar".to_string(),
            Box::new(
                |bar: &ProgressBar, output: &Rc<RefCell<dyn OutputInterface>>| {
                    let complete_bars = bar.get_bar_offset();
                    let mut display = shirabe_php_shim::str_repeat(
                        &bar.get_bar_character(),
                        complete_bars as usize,
                    );
                    if complete_bars < bar.get_bar_width() as f64 {
                        let empty_bars = bar.get_bar_width() as f64
                            - complete_bars
                            - Helper::length(&Helper::remove_decoration(
                                todo!("$output->getFormatter()"),
                                &bar.get_progress_character(),
                            )) as f64;
                        display.push_str(&format!(
                            "{}{}",
                            bar.get_progress_character(),
                            shirabe_php_shim::str_repeat(
                                &bar.get_empty_bar_character(),
                                empty_bars as usize
                            )
                        ));
                        let _ = output;
                    }

                    Ok(Ok(shirabe_php_shim::PhpMixed::String(display)))
                },
            ),
        );

        formatters.insert(
            "elapsed".to_string(),
            Box::new(
                |bar: &ProgressBar, _output: &Rc<RefCell<dyn OutputInterface>>| {
                    Ok(Ok(shirabe_php_shim::PhpMixed::String(
                        Helper::format_time(
                            (shirabe_php_shim::time() - bar.get_start_time()) as f64,
                        )
                        .unwrap_or_default(),
                    )))
                },
            ),
        );

        formatters.insert(
            "remaining".to_string(),
            Box::new(|bar: &ProgressBar, _output: &Rc<RefCell<dyn OutputInterface>>| {
                if bar.get_max_steps() == 0 {
                    return Ok(Err(LogicException(shirabe_php_shim::LogicException {
                        message: "Unable to display the remaining time if the maximum number of steps is not set.".to_string(),
                        code: 0,
                    })));
                }

                Ok(Ok(shirabe_php_shim::PhpMixed::String(
                    Helper::format_time(bar.get_remaining()).unwrap_or_default(),
                )))
            }),
        );

        formatters.insert(
            "estimated".to_string(),
            Box::new(|bar: &ProgressBar, _output: &Rc<RefCell<dyn OutputInterface>>| {
                if bar.get_max_steps() == 0 {
                    return Ok(Err(LogicException(shirabe_php_shim::LogicException {
                        message: "Unable to display the estimated time if the maximum number of steps is not set.".to_string(),
                        code: 0,
                    })));
                }

                Ok(Ok(shirabe_php_shim::PhpMixed::String(
                    Helper::format_time(bar.get_estimated()).unwrap_or_default(),
                )))
            }),
        );

        formatters.insert(
            "memory".to_string(),
            Box::new(
                |_bar: &ProgressBar, _output: &Rc<RefCell<dyn OutputInterface>>| {
                    Ok(Ok(shirabe_php_shim::PhpMixed::String(
                        Helper::format_memory(shirabe_php_shim::memory_get_usage()),
                    )))
                },
            ),
        );

        formatters.insert(
            "current".to_string(),
            Box::new(
                |bar: &ProgressBar, _output: &Rc<RefCell<dyn OutputInterface>>| {
                    Ok(Ok(shirabe_php_shim::PhpMixed::String(
                        shirabe_php_shim::str_pad(
                            &bar.get_progress().to_string(),
                            bar.get_step_width() as usize,
                            " ",
                            shirabe_php_shim::STR_PAD_LEFT,
                        ),
                    )))
                },
            ),
        );

        formatters.insert(
            "max".to_string(),
            Box::new(
                |bar: &ProgressBar, _output: &Rc<RefCell<dyn OutputInterface>>| {
                    Ok(Ok(shirabe_php_shim::PhpMixed::Int(bar.get_max_steps())))
                },
            ),
        );

        formatters.insert(
            "percent".to_string(),
            Box::new(
                |bar: &ProgressBar, _output: &Rc<RefCell<dyn OutputInterface>>| {
                    Ok(Ok(shirabe_php_shim::PhpMixed::Float(
                        (bar.get_progress_percent() * 100.0).floor(),
                    )))
                },
            ),
        );

        formatters
    }

    fn init_formats() -> IndexMap<String, String> {
        let mut formats: IndexMap<String, String> = IndexMap::new();

        formats.insert(
            FORMAT_NORMAL.to_string(),
            " %current%/%max% [%bar%] %percent:3s%%".to_string(),
        );
        formats.insert(
            FORMAT_NORMAL_NOMAX.to_string(),
            " %current% [%bar%]".to_string(),
        );

        formats.insert(
            FORMAT_VERBOSE.to_string(),
            " %current%/%max% [%bar%] %percent:3s%% %elapsed:6s%".to_string(),
        );
        formats.insert(
            FORMAT_VERBOSE_NOMAX.to_string(),
            " %current% [%bar%] %elapsed:6s%".to_string(),
        );

        formats.insert(
            FORMAT_VERY_VERBOSE.to_string(),
            " %current%/%max% [%bar%] %percent:3s%% %elapsed:6s%/%estimated:-6s%".to_string(),
        );
        formats.insert(
            FORMAT_VERY_VERBOSE_NOMAX.to_string(),
            " %current% [%bar%] %elapsed:6s%".to_string(),
        );

        formats.insert(
            FORMAT_DEBUG.to_string(),
            " %current%/%max% [%bar%] %percent:3s%% %elapsed:6s%/%estimated:-6s% %memory:6s%"
                .to_string(),
        );
        formats.insert(
            FORMAT_DEBUG_NOMAX.to_string(),
            " %current% [%bar%] %elapsed:6s% %memory:6s%".to_string(),
        );

        formats
    }

    fn build_line(&mut self) -> anyhow::Result<String> {
        let regex = "{%([a-z\\-_]+)(?:\\:([^%]+))?%}i";

        // The callback resolves a placeholder match into its replacement text.
        // It is invoked by preg_replace_callback over $this->format.
        let line = self.build_line_apply(regex)?;

        // gets string length for each sub line with multiline format
        let lines_length: Vec<i64> = shirabe_php_shim::explode("\n", &line)
            .iter()
            .map(|sub_line| {
                Helper::width(&Helper::remove_decoration(
                    todo!("$this->output->getFormatter()"),
                    &shirabe_php_shim::rtrim(sub_line, Some("\r")),
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
    fn build_line_apply(&self, regex: &str) -> anyhow::Result<String> {
        let format = self.format.clone().unwrap_or_default();

        // $callback in PHP, expressed as a closure over $this and the matches.
        let callback = |matches: &[Option<String>]| -> anyhow::Result<String> {
            let name = matches[1].clone().unwrap_or_default();

            let text: shirabe_php_shim::PhpMixed =
                if Self::get_placeholder_formatter_definition(&name).is_some() {
                    // $text = $formatter($this, $this->output);
                    let formatter_result = FORMATTERS.with(|formatters| {
                        let formatters = formatters.borrow();
                        let formatter = formatters.as_ref().unwrap().get(&name).unwrap();
                        formatter(self, &self.output)
                    });
                    match formatter_result? {
                        Ok(text) => text,
                        Err(e) => return Err(anyhow::Error::new(e)),
                    }
                } else if let Some(message) = self.messages.get(&name) {
                    shirabe_php_shim::PhpMixed::String(message.clone())
                } else {
                    return Ok(matches[0].clone().unwrap_or_default());
                };

            if let Some(modifier) = matches.get(2).and_then(|m| m.clone()) {
                return Ok(shirabe_php_shim::sprintf(&format!("%{modifier}"), &[text]));
            }

            // PHP implicitly casts the formatter result to string here.
            Ok(match text {
                shirabe_php_shim::PhpMixed::String(s) => s,
                shirabe_php_shim::PhpMixed::Int(i) => i.to_string(),
                shirabe_php_shim::PhpMixed::Float(f) => {
                    format!("{}", shirabe_php_shim::PhpMixed::Float(f))
                }
                other => format!("{}", other),
            })
        };

        shirabe_php_shim::preg_replace_callback(regex, callback, &format)
    }
}
