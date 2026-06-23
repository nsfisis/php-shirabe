//! ref: composer/vendor/symfony/console/Helper/QuestionHelper.php

use crate::symfony::console::cursor::Cursor;
use crate::symfony::console::exception::missing_input_exception::MissingInputException;
use crate::symfony::console::exception::runtime_exception::RuntimeException;
use crate::symfony::console::formatter::output_formatter::OutputFormatter;
use crate::symfony::console::formatter::output_formatter_style::OutputFormatterStyle;
use crate::symfony::console::helper::formatter_helper::FormatBlockMessages;
use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::helper::helper_interface::HelperInterface;
use crate::symfony::console::helper::helper_set::HelperSet;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::output::console_output::ConsoleOutput;
use crate::symfony::console::output::console_output_interface::ConsoleOutputInterface;
use crate::symfony::console::output::console_section_output::ConsoleSectionOutput;
use crate::symfony::console::output::output_interface;
use crate::symfony::console::output::output_interface::OutputInterface;
use crate::symfony::console::question::ChoiceQuestion;
use crate::symfony::console::question::QuestionInterface;
use crate::symfony::console::terminal::Terminal;
use crate::symfony::string::s;
use shirabe_php_shim::AsAny;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

/// The QuestionHelper class provides helpers to interact with the user.
#[derive(Debug, Default)]
pub struct QuestionHelper {
    pub(crate) inner: Helper,

    /// @var resource|null
    input_stream: Option<shirabe_php_shim::PhpResource>,
}

/// self::$stty
static STTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// self::$stdinIsInteractive
static STDIN_IS_INTERACTIVE: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);

impl QuestionHelper {
    /// Asks a question to the user.
    ///
    /// @return mixed The user answer
    ///
    /// @throws RuntimeException If there is no data to read in the input stream
    pub fn ask(
        &mut self,
        input: &mut dyn InputInterface,
        output: Rc<RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) -> anyhow::Result<Result<PhpMixed, MissingInputException>> {
        let mut output = output;
        let error_output = {
            let borrowed = output.borrow();
            (*borrowed)
                .as_any()
                .downcast_ref::<ConsoleOutput>()
                .map(|console_output| console_output.get_error_output())
        };
        if let Some(error_output) = error_output {
            output = error_output;
        }

        if !input.is_interactive() {
            return Ok(Ok(self.get_default_answer(question)));
        }

        if let Some(streamable) = input.as_streamable()
            && let Some(stream) = streamable.get_stream()
        {
            self.input_stream = Some(stream);
        }

        let result: anyhow::Result<Result<PhpMixed, MissingInputException>> = (|| {
            if question.get_validator().is_none() {
                return self.do_ask(Rc::clone(&output), question);
            }

            let interviewer = || self.do_ask(Rc::clone(&output), question);

            self.validate_attempts(&interviewer, Rc::clone(&output), question)
        })();

        let result = result?;
        match result {
            Ok(value) => Ok(Ok(value)),
            Err(exception) => {
                input.set_interactive(false);

                let fallback_output = self.get_default_answer(question);
                if matches!(fallback_output, PhpMixed::Null) {
                    return Ok(Err(exception));
                }

                Ok(Ok(fallback_output))
            }
        }
    }

    pub fn get_name(&self) -> String {
        "question".to_string()
    }

    /// Prevents usage of stty.
    pub fn disable_stty() {
        STTY.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Asks the question to the user.
    ///
    /// @return mixed
    ///
    /// @throws RuntimeException In case the fallback is deactivated and the response cannot be hidden
    fn do_ask(
        &self,
        output: Rc<RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) -> anyhow::Result<Result<PhpMixed, MissingInputException>> {
        self.write_prompt(Rc::clone(&output), question);

        let input_stream = self
            .input_stream
            .clone()
            .unwrap_or_else(shirabe_php_shim::stdin);
        let autocomplete = question.get_autocompleter_callback();

        let ret: PhpMixed;
        if autocomplete.is_none()
            || !STTY.load(std::sync::atomic::Ordering::SeqCst)
            || !Terminal::has_stty_available()
        {
            let mut r: PhpMixed = PhpMixed::Bool(false);
            if question.is_hidden() {
                match self.get_hidden_response(
                    Rc::clone(&output),
                    &input_stream,
                    question.is_trimmable(),
                )? {
                    Ok(hidden_response) => {
                        r = PhpMixed::String(if question.is_trimmable() {
                            shirabe_php_shim::trim(&hidden_response, None)
                        } else {
                            hidden_response
                        });
                    }
                    Err(e) => {
                        if !question.is_hidden_fallback() {
                            return Err(e.into());
                        }
                    }
                }
            }

            if matches!(r, PhpMixed::Bool(false)) {
                let is_blocked = shirabe_php_shim::stream_get_meta_data(&input_stream)
                    .get("blocked")
                    .cloned()
                    .unwrap_or(PhpMixed::Bool(true));

                if !shirabe_php_shim::boolval(&is_blocked) {
                    shirabe_php_shim::stream_set_blocking(&input_stream, true);
                }

                let read = self.read_input(&input_stream, question);

                if !shirabe_php_shim::boolval(&is_blocked) {
                    shirabe_php_shim::stream_set_blocking(&input_stream, false);
                }

                if matches!(read, PhpMixed::Bool(false)) {
                    return Ok(Err(MissingInputException(RuntimeException(
                        shirabe_php_shim::RuntimeException {
                            message: "Aborted.".to_string(),
                            code: 0,
                        },
                    ))));
                }
                r = read;
                if question.is_trimmable() {
                    r = PhpMixed::String(shirabe_php_shim::trim(&r.to_string(), None));
                }
            }
            ret = r;
        } else {
            let callback = autocomplete.unwrap();
            // The autocompleter callback yields an iterable (Option here); PHP
            // treats a null result as an empty list of suggestions.
            let callback = move |input: &str| callback(input).unwrap_or_default();
            let autocomplete =
                self.autocomplete(Rc::clone(&output), question, &input_stream, &callback);
            ret = PhpMixed::String(if question.is_trimmable() {
                shirabe_php_shim::trim(&autocomplete, None)
            } else {
                autocomplete
            });
        }

        let mut ret = ret;
        {
            let borrowed = output.borrow();
            if let Some(section_output) =
                (*borrowed).as_any().downcast_ref::<ConsoleSectionOutput>()
            {
                section_output.add_content(&ret.to_string());
            }
        }

        ret = if shirabe_php_shim::strlen(&ret.to_string()) > 0 {
            ret
        } else {
            question.get_default()
        };

        if let Some(normalizer) = question.get_normalizer() {
            return Ok(Ok(normalizer(ret)));
        }

        Ok(Ok(ret))
    }

    /// @return mixed
    fn get_default_answer(&self, question: &impl QuestionInterface) -> PhpMixed {
        let default = question.get_default();

        if matches!(default, PhpMixed::Null) {
            return default;
        }

        if let Some(validator) = question.get_validator() {
            // call_user_func($question->getValidator(), $default)
            return validator(Some(default)).unwrap();
        } else if let Some(choice_question) = question.as_choice() {
            let choices = choice_question.get_choices();

            if !choice_question.is_multiselect() {
                return choices
                    .get(&default.to_string())
                    .cloned()
                    .unwrap_or(default);
            }

            let default_parts = shirabe_php_shim::explode(",", &default.to_string());
            let mut resolved: indexmap::IndexMap<String, PhpMixed> = indexmap::IndexMap::new();
            for (k, v) in default_parts.iter().enumerate() {
                let v = if question.is_trimmable() {
                    shirabe_php_shim::trim(v, None)
                } else {
                    v.clone()
                };
                let value = choices.get(&v).cloned().unwrap_or(PhpMixed::String(v));
                resolved.insert(k.to_string(), value);
            }

            return PhpMixed::Array(resolved);
        }

        default
    }

    /// Outputs the question prompt.
    pub(crate) fn write_prompt(
        &self,
        output: Rc<RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) {
        let mut message = question.get_question().to_string();

        if let Some(choice_question) = question.as_choice() {
            let mut lines = vec![question.get_question().to_string()];
            lines.extend(self.format_choice_question_choices(choice_question, "info"));
            output
                .borrow()
                .writeln(&lines, output_interface::OUTPUT_NORMAL);

            message = choice_question.get_prompt().to_string();
        }

        output
            .borrow()
            .write(&[message], false, output_interface::OUTPUT_NORMAL);
    }

    /// @return string[]
    pub(crate) fn format_choice_question_choices(
        &self,
        question: &ChoiceQuestion,
        tag: &str,
    ) -> Vec<String> {
        let mut messages: Vec<String> = vec![];

        let choices = question.get_choices();
        let max_width = choices
            .keys()
            .map(|key| Helper::width(key))
            .max()
            .unwrap_or(0);

        for (key, value) in choices {
            let padding =
                shirabe_php_shim::str_repeat(" ", (max_width - Helper::width(key)) as usize);

            messages.push(format!(
                "  [<{tag}>{}{padding}</{tag}>] {}",
                PhpMixed::String(key.clone()),
                value.clone(),
            ));
        }

        messages
    }

    /// Outputs an error message.
    pub(crate) fn write_error(
        &self,
        output: Rc<RefCell<dyn OutputInterface>>,
        error: &shirabe_php_shim::Exception,
    ) {
        let message = if let Some(helper_set) = self.get_helper_set() {
            let formatter = helper_set.borrow().get_formatter();
            let message = formatter.borrow().format_block(
                FormatBlockMessages::String(error.message.clone()),
                "error",
                false,
            );
            message
        } else {
            format!("<error>{}</error>", error.message)
        };

        output
            .borrow()
            .writeln(&[message], output_interface::OUTPUT_NORMAL);
    }

    /// Autocompletes a question.
    ///
    /// @param resource $inputStream
    fn autocomplete(
        &self,
        output: Rc<RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
        input_stream: &shirabe_php_shim::PhpResource,
        autocomplete: &dyn Fn(&str) -> Vec<PhpMixed>,
    ) -> String {
        let cursor = Cursor::new(Rc::clone(&output), Some(input_stream.clone()));

        let mut full_choice = String::new();
        let mut ret = String::new();

        let mut i: i64 = 0;
        let mut ofs: i64 = -1;
        let mut matches = autocomplete(&ret);
        let mut num_matches = matches.len() as i64;

        let stty_mode = shirabe_php_shim::shell_exec("stty -g").unwrap_or_default();
        let is_stdin = shirabe_php_shim::stream_get_meta_data(input_stream)
            .get("uri")
            .map(|uri| uri.to_string() == "php://stdin")
            .unwrap_or(false);
        let mut r = vec![input_stream.clone()];
        let w: Vec<shirabe_php_shim::PhpResource> = vec![];

        // Disable icanon (so we can fread each keypress) and echo (we'll do echoing here instead)
        shirabe_php_shim::shell_exec("stty -icanon -echo");

        // Add highlighted text style
        output.borrow().get_formatter().borrow_mut().set_style(
            "hl",
            Box::new(OutputFormatterStyle::new(
                Some("black"),
                Some("white"),
                vec![],
            )),
        );

        // Read a keypress
        while !shirabe_php_shim::feof(input_stream) {
            while is_stdin
                && Some(0)
                    == shirabe_php_shim::stream_select(
                        &mut r,
                        &mut w.clone(),
                        &mut w.clone(),
                        0,
                        Some(100),
                    )
            {
                // Give signal handlers a chance to run
                r = vec![input_stream.clone()];
            }
            let mut c = shirabe_php_shim::fread(input_stream, 1);

            // as opposed to fgets(), fread() returns an empty string when the stream content is empty, not false.
            if c.is_none()
                || (ret.is_empty()
                    && c.as_deref() == Some("")
                    && matches!(question.get_default(), PhpMixed::Null))
            {
                shirabe_php_shim::shell_exec(&format!("stty {}", stty_mode));
                // throw new MissingInputException('Aborted.');
                // autocomplete() returns string in PHP; this throw aborts the
                // whole read. Faithful exception propagation is resolved later.
                todo!("MissingInputException('Aborted.') thrown inside autocomplete");
            } else if c.as_deref() == Some("\u{7f}") {
                // Backspace Character
                if 0 == num_matches && 0 != i {
                    i -= 1;
                    cursor.move_left(s(&full_choice).slice(-1, None).width(false));

                    full_choice = QuestionHelper::substr(Some(&full_choice), 0, Some(i));
                }

                if 0 == i {
                    ofs = -1;
                    matches = autocomplete(&ret);
                    num_matches = matches.len() as i64;
                } else {
                    num_matches = 0;
                }

                // Pop the last character off the end of our string
                ret = QuestionHelper::substr(Some(&ret), 0, Some(i));
            } else if c.as_deref() == Some("\u{1b}") {
                // Did we read an escape sequence?
                let escape = shirabe_php_shim::fread(input_stream, 2).unwrap_or_default();
                let cc = format!("{}{}", c.clone().unwrap_or_default(), escape);
                c = Some(cc.clone());

                // A = Up Arrow. B = Down Arrow
                let c2 = cc.as_bytes().get(2).copied();
                if c2 == Some(b'A') || c2 == Some(b'B') {
                    if c2 == Some(b'A') && -1 == ofs {
                        ofs = 0;
                    }

                    if 0 == num_matches {
                        continue;
                    }

                    ofs += if c2 == Some(b'A') { -1 } else { 1 };
                    ofs = (num_matches + ofs) % num_matches;
                }
            } else if shirabe_php_shim::ord(c.as_deref().unwrap_or("")) < 32 {
                if c.as_deref() == Some("\t") || c.as_deref() == Some("\n") {
                    if num_matches > 0 && -1 != ofs {
                        ret = matches[ofs as usize].to_string();
                        // Echo out remaining chars for current match
                        let remaining_characters = shirabe_php_shim::substr(
                            &ret,
                            shirabe_php_shim::strlen(&shirabe_php_shim::trim(
                                &self.most_recently_entered_value(&full_choice),
                                None,
                            )),
                            None,
                        );
                        output.borrow().write(
                            std::slice::from_ref(&remaining_characters),
                            false,
                            output_interface::OUTPUT_NORMAL,
                        );
                        full_choice.push_str(&remaining_characters);
                        i = match shirabe_php_shim::mb_detect_encoding(&full_choice, None, true) {
                            None => shirabe_php_shim::strlen(&full_choice),
                            Some(encoding) => shirabe_php_shim::mb_strlen(&full_choice, &encoding),
                        };

                        let ret_for_filter = ret.clone();
                        matches = autocomplete(&ret)
                            .into_iter()
                            .filter(|m| {
                                ret_for_filter.is_empty()
                                    || shirabe_php_shim::str_starts_with(
                                        &m.to_string(),
                                        &ret_for_filter,
                                    )
                            })
                            .collect();
                        num_matches = matches.len() as i64;
                        ofs = -1;
                    }

                    if c.as_deref() == Some("\n") {
                        output.borrow().write(
                            &[c.clone().unwrap_or_default()],
                            false,
                            output_interface::OUTPUT_NORMAL,
                        );
                        break;
                    }

                    num_matches = 0;
                }

                continue;
            } else {
                let cur = c.clone().unwrap_or_default();
                if "\u{80}" <= cur.as_str() {
                    let len = match shirabe_php_shim::str_bitand(&cur, "\u{f0}").as_str() {
                        "\u{c0}" => 1,
                        "\u{d0}" => 1,
                        "\u{e0}" => 2,
                        "\u{f0}" => 3,
                        _ => 0,
                    };
                    let extra = shirabe_php_shim::fread(input_stream, len).unwrap_or_default();
                    c = Some(format!("{}{}", cur, extra));
                }

                let cur = c.clone().unwrap_or_default();
                output.borrow().write(
                    std::slice::from_ref(&cur),
                    false,
                    output_interface::OUTPUT_NORMAL,
                );
                ret.push_str(&cur);
                full_choice.push_str(&cur);
                i += 1;

                let mut temp_ret = ret.clone();

                if let Some(choice_question) = question.as_choice()
                    && choice_question.is_multiselect()
                {
                    temp_ret = self.most_recently_entered_value(&full_choice);
                }

                num_matches = 0;
                ofs = 0;

                for value in autocomplete(&ret) {
                    // If typed characters match the beginning chunk of value (e.g. [AcmeDe]moBundle)
                    if shirabe_php_shim::str_starts_with(&value.to_string(), &temp_ret) {
                        if (num_matches as usize) < matches.len() {
                            matches[num_matches as usize] = value;
                        } else {
                            matches.push(value);
                        }
                        num_matches += 1;
                    }
                }
            }

            cursor.clear_line_after();

            if num_matches > 0 && -1 != ofs {
                cursor.save_position();
                // Write highlighted text, complete the partially entered response
                let characters_entered = shirabe_php_shim::strlen(&shirabe_php_shim::trim(
                    &self.most_recently_entered_value(&full_choice),
                    None,
                ));
                output.borrow().write(
                    &[format!(
                        "<hl>{}</hl>",
                        OutputFormatter::escape_trailing_backslash(&shirabe_php_shim::substr(
                            &matches[ofs as usize].to_string(),
                            characters_entered,
                            None,
                        ))
                    )],
                    false,
                    output_interface::OUTPUT_NORMAL,
                );
                cursor.restore_position();
            }
        }

        // Reset stty so it behaves normally again
        shirabe_php_shim::shell_exec(&format!("stty {}", stty_mode));

        full_choice
    }

    fn most_recently_entered_value(&self, entered: &str) -> String {
        // Determine the most recent value that the user entered
        if !shirabe_php_shim::str_contains(entered, ",") {
            return entered.to_string();
        }

        let choices = shirabe_php_shim::explode(",", entered);
        let last_choice = shirabe_php_shim::trim(&choices[choices.len() - 1], None);
        if !last_choice.is_empty() {
            return last_choice;
        }

        entered.to_string()
    }

    /// Gets a hidden response from user.
    ///
    /// @param resource $inputStream The handler resource
    /// @param bool     $trimmable   Is the answer trimmable
    ///
    /// @throws RuntimeException In case the fallback is deactivated and the response cannot be hidden
    fn get_hidden_response(
        &self,
        output: Rc<RefCell<dyn OutputInterface>>,
        input_stream: &shirabe_php_shim::PhpResource,
        trimmable: bool,
    ) -> anyhow::Result<Result<String, RuntimeException>> {
        if shirabe_php_shim::DIRECTORY_SEPARATOR == "\\" {
            let mut exe = format!(
                "{}/../Resources/bin/hiddeninput.exe",
                shirabe_php_shim::dir()
            );

            // handle code running from a phar
            let mut tmp_exe: Option<String> = None;
            if shirabe_php_shim::substr(&magic_file(), 0, Some(5)) == "phar:" {
                let tmp = format!("{}/hiddeninput.exe", shirabe_php_shim::sys_get_temp_dir());
                shirabe_php_shim::copy(&exe, &tmp);
                exe = tmp.clone();
                tmp_exe = Some(tmp);
            }

            let s_exec = shirabe_php_shim::shell_exec(&format!("\"{}\"", exe)).unwrap_or_default();
            let value = if trimmable {
                shirabe_php_shim::rtrim(&s_exec, None)
            } else {
                s_exec
            };
            output
                .borrow()
                .writeln(&["".to_string()], output_interface::OUTPUT_NORMAL);

            if let Some(tmp) = tmp_exe {
                shirabe_php_shim::unlink(&tmp);
            }

            return Ok(Ok(value));
        }

        let mut stty_mode = String::new();
        if STTY.load(std::sync::atomic::Ordering::SeqCst) && Terminal::has_stty_available() {
            stty_mode = shirabe_php_shim::shell_exec("stty -g").unwrap_or_default();
            shirabe_php_shim::shell_exec("stty -echo");
        } else if self.is_interactive_input(input_stream) {
            return Ok(Err(RuntimeException(shirabe_php_shim::RuntimeException {
                message: "Unable to hide the response.".to_string(),
                code: 0,
            })));
        }

        let value = shirabe_php_shim::fgets(input_stream, Some(4096));

        if STTY.load(std::sync::atomic::Ordering::SeqCst) && Terminal::has_stty_available() {
            shirabe_php_shim::shell_exec(&format!("stty {}", stty_mode));
        }

        let mut value = match value {
            Some(value) => value,
            None => {
                return Err(MissingInputException(RuntimeException(
                    shirabe_php_shim::RuntimeException {
                        message: "Aborted.".to_string(),
                        code: 0,
                    },
                ))
                .into());
            }
        };
        if trimmable {
            value = shirabe_php_shim::trim(&value, None);
        }
        output
            .borrow()
            .writeln(&["".to_string()], output_interface::OUTPUT_NORMAL);

        Ok(Ok(value))
    }

    /// Validates an attempt.
    ///
    /// @param callable $interviewer A callable that will ask for a question and return the result
    ///
    /// @return mixed The validated response
    ///
    /// @throws \Exception In case the max number of attempts has been reached and no valid response has been given
    fn validate_attempts(
        &self,
        interviewer: &dyn Fn() -> anyhow::Result<Result<PhpMixed, MissingInputException>>,
        output: Rc<RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) -> anyhow::Result<Result<PhpMixed, MissingInputException>> {
        let mut error: Option<shirabe_php_shim::Exception> = None;
        let mut attempts = question.get_max_attempts();

        loop {
            // while (null === $attempts || $attempts--)
            match attempts {
                None => {}
                Some(0) => break,
                Some(n) => attempts = Some(n - 1),
            }

            if let Some(ref error) = error {
                self.write_error(Rc::clone(&output), error);
            }

            let interviewed = match interviewer()? {
                Ok(value) => value,
                Err(missing) => return Ok(Err(missing)),
            };

            match question.get_validator().unwrap()(Some(interviewed)) {
                Ok(value) => return Ok(Ok(value)),
                Err(e) => {
                    // PHP: `catch (RuntimeException $e) { throw $e; } catch (\Exception $error) {}`.
                    // The validator return type is fixed to InvalidArgumentException here, so the
                    // RuntimeException rethrow branch is statically unreachable; record the error
                    // and retry.
                    error = Some(shirabe_php_shim::Exception {
                        message: e.0.message.clone(),
                        code: e.0.code,
                    });
                }
            }
        }

        // throw $error;
        Err(anyhow::Error::msg(
            error.map(|e| e.message).unwrap_or_default(),
        ))
    }

    fn is_interactive_input(&self, input_stream: &shirabe_php_shim::PhpResource) -> bool {
        let uri = shirabe_php_shim::stream_get_meta_data(input_stream)
            .get("uri")
            .map(|uri| uri.to_string());
        if uri.as_deref() != Some("php://stdin") {
            return false;
        }

        let mut stdin_is_interactive = STDIN_IS_INTERACTIVE.lock().unwrap();
        if let Some(value) = *stdin_is_interactive {
            return value;
        }

        let value = shirabe_php_shim::stream_isatty_resource(
            &shirabe_php_shim::php_fopen_resource("php://stdin", "r"),
        );
        *stdin_is_interactive = Some(value);
        value
    }

    /// Reads one or more lines of input and returns what is read.
    ///
    /// @param resource $inputStream The handler resource
    /// @param Question $question    The question being asked
    ///
    /// @return string|false The input received, false in case input could not be read
    fn read_input(
        &self,
        input_stream: &shirabe_php_shim::PhpResource,
        question: &impl QuestionInterface,
    ) -> PhpMixed {
        if !question.is_multiline() {
            let cp = self.set_io_codepage();
            let ret = shirabe_php_shim::fgets(input_stream, Some(4096));

            return self.reset_io_codepage(
                cp,
                ret.map(PhpMixed::String).unwrap_or(PhpMixed::Bool(false)),
            );
        }

        let multi_line_stream_reader = self.clone_input_stream(input_stream);
        let multi_line_stream_reader = match multi_line_stream_reader {
            Some(reader) => reader,
            None => return PhpMixed::Bool(false),
        };

        let mut ret = String::new();
        let cp = self.set_io_codepage();
        loop {
            let char = shirabe_php_shim::fgetc(&multi_line_stream_reader);
            let char = match char {
                Some(char) => char,
                None => break,
            };
            if shirabe_php_shim::PHP_EOL == format!("{}{}", ret, char) {
                break;
            }
            ret.push_str(&char);
        }

        self.reset_io_codepage(cp, PhpMixed::String(ret))
    }

    /// Sets console I/O to the host code page.
    ///
    /// @return int Previous code page in IBM/EBCDIC format
    fn set_io_codepage(&self) -> i64 {
        if shirabe_php_shim::function_exists("sapi_windows_cp_set") {
            let cp = shirabe_php_shim::sapi_windows_cp_get(None);
            shirabe_php_shim::sapi_windows_cp_set(shirabe_php_shim::sapi_windows_cp_get(Some(
                "oem",
            )));

            return cp;
        }

        0
    }

    /// Sets console I/O to the specified code page and converts the user input.
    ///
    /// @param string|false $input
    ///
    /// @return string|false
    fn reset_io_codepage(&self, cp: i64, input: PhpMixed) -> PhpMixed {
        let mut input = input;
        if 0 != cp {
            shirabe_php_shim::sapi_windows_cp_set(cp);

            if !matches!(input, PhpMixed::Bool(false)) && input.to_string() != "" {
                input = PhpMixed::String(shirabe_php_shim::sapi_windows_cp_conv(
                    shirabe_php_shim::sapi_windows_cp_get(Some("oem")),
                    cp,
                    &input.to_string(),
                ));
            }
        }

        input
    }

    /// Clones an input stream in order to act on one instance of the same
    /// stream without affecting the other instance.
    ///
    /// @param resource $inputStream The handler resource
    ///
    /// @return resource|null The cloned resource, null in case it could not be cloned
    fn clone_input_stream(
        &self,
        input_stream: &shirabe_php_shim::PhpResource,
    ) -> Option<shirabe_php_shim::PhpResource> {
        let stream_meta_data = shirabe_php_shim::stream_get_meta_data(input_stream);
        let seekable = stream_meta_data
            .get("seekable")
            .cloned()
            .unwrap_or(PhpMixed::Bool(false));
        let mode = stream_meta_data
            .get("mode")
            .map(|m| m.to_string())
            .unwrap_or_else(|| "rb".to_string());
        let uri = stream_meta_data.get("uri").map(|u| u.to_string());

        let uri = uri?;

        let clone_stream = shirabe_php_shim::fopen(&uri, &mode).ok()?;

        // For seekable and writable streams, add all the same data to the
        // cloned stream and then seek to the same offset.
        if matches!(seekable, PhpMixed::Bool(true)) && !["r", "rb", "rt"].contains(&mode.as_str()) {
            let offset = shirabe_php_shim::ftell(input_stream).unwrap_or(0);
            shirabe_php_shim::rewind(input_stream);
            shirabe_php_shim::stream_copy_to_stream(input_stream, &clone_stream);
            shirabe_php_shim::fseek(input_stream, offset, shirabe_php_shim::SEEK_SET);
            shirabe_php_shim::fseek(&clone_stream, offset, shirabe_php_shim::SEEK_SET);
        }

        Some(clone_stream)
    }

    /// Helper::substr proxy (inherited static helper).
    fn substr(string: Option<&str>, from: i64, length: Option<i64>) -> String {
        Helper::substr(string.unwrap_or(""), from, length)
    }
}

// PHP `__FILE__` magic constant. The shim's `file()` is PHP's file() function,
// not the magic constant, and there is no `__FILE__` shim yet (see report).
fn magic_file() -> String {
    todo!("magic_file: shim needs a __FILE__ magic-constant equivalent")
}

impl HelperInterface for QuestionHelper {
    fn set_helper_set(&mut self, helper_set: Option<Rc<RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<Rc<RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> String {
        self.get_name()
    }
}
