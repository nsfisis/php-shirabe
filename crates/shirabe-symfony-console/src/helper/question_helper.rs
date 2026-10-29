//! ref: composer/vendor/symfony/console/Helper/QuestionHelper.php

use crate::cursor::Cursor;
use crate::exception::{MissingInputException, RuntimeException};
use crate::formatter::{OutputFormatter, OutputFormatterStyle};
use crate::helper::{FormatBlockMessages, Helper, HelperInterface, HelperSet};
use crate::input::InputInterface;
use crate::output::{
    ConsoleOutput, ConsoleOutputInterface, ConsoleSectionOutput, OutputInterface, output_interface,
};
use crate::question::{ChoiceQuestion, QuestionInterface};
use crate::terminal::Terminal;
use bstr::{B, BString};
use shirabe_php_shim::{
    Exception, PHP_EOL, PhpMixed, PhpResource, SEEK_SET, bconcat, copy, dir, explode, feof, fgetc,
    fgets, fopen, fread, fseek, ftell, function_exists, legacy_lossy, mb_detect_encoding,
    mb_strlen, ord, path_from_bytes, php_fopen_resource, php_to_string, rewind, rtrim,
    sapi_windows_cp_conv, sapi_windows_cp_get, sapi_windows_cp_set, shell_exec, stdin, str_bitand,
    str_repeat, stream_copy_to_stream, stream_get_meta_data, stream_isatty_resource, stream_select,
    stream_set_blocking, strlen, substr, sys_get_temp_dir, trim, unlink,
};
use shirabe_symfony_string::s;

/// The QuestionHelper class provides helpers to interact with the user.
#[derive(Debug, Default)]
pub struct QuestionHelper {
    inner: Helper,
    input_stream: Option<PhpResource>,
}

/// self::$stty
static STTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// self::$stdinIsInteractive
static STDIN_IS_INTERACTIVE: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);

/// PHP dispatches `$this->writePrompt()` / `$this->writeError()` virtually, and
/// `SymfonyQuestionHelper` overrides both protected methods. The embedded-super port expresses
/// that late binding as a trait: the template methods are provided here and reach the overridable
/// hooks through `Self`, while `inner()` reaches the base-class state. PHP has no such interface;
/// the invented name follows the `QuestionInterface` precedent.
pub trait QuestionHelperInterface {
    fn inner(&self) -> &QuestionHelper;

    fn inner_mut(&mut self) -> &mut QuestionHelper;

    /// Asks a question to the user.
    fn ask(
        &mut self,
        input: &mut dyn InputInterface,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
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
            return Ok(Ok(self.inner().get_default_answer(question)));
        }

        if let Some(streamable) = input.as_streamable()
            && let Some(stream) = streamable.get_stream()
        {
            self.inner_mut().input_stream = Some(stream);
        }

        let result: anyhow::Result<Result<PhpMixed, MissingInputException>> = (|| {
            if question.get_validator().is_none() {
                return self.do_ask(std::rc::Rc::clone(&output), question);
            }

            let interviewer = || self.do_ask(std::rc::Rc::clone(&output), question);

            self.validate_attempts(&interviewer, std::rc::Rc::clone(&output), question)
        })();

        let result = result?;
        match result {
            Ok(value) => Ok(Ok(value)),
            Err(exception) => {
                input.set_interactive(false);

                let fallback_output = self.inner().get_default_answer(question);
                if matches!(fallback_output, PhpMixed::Null) {
                    return Ok(Err(exception));
                }

                Ok(Ok(fallback_output))
            }
        }
    }

    /// Asks the question to the user (PHP private; on the trait so it can late-bind the hooks).
    fn do_ask(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) -> anyhow::Result<Result<PhpMixed, MissingInputException>> {
        self.write_prompt(std::rc::Rc::clone(&output), question);

        let input_stream = self.inner().input_stream.clone().unwrap_or_else(stdin);
        let autocomplete = question.get_autocompleter_callback();

        let ret: PhpMixed;

        if let Some(autocomplete) = autocomplete
            && STTY.load(std::sync::atomic::Ordering::SeqCst)
            && Terminal::has_stty_available()
        {
            let callback = autocomplete;
            // The autocompleter callback yields an iterable (Option here); PHP
            // treats a null result as an empty list of suggestions.
            let callback = move |input: &str| callback(input).unwrap_or_default();
            let autocomplete = match self.inner().autocomplete(
                std::rc::Rc::clone(&output),
                question,
                &input_stream,
                &callback,
            ) {
                Ok(value) => value,
                Err(exception) => return Ok(Err(exception)),
            };
            ret = PhpMixed::string(if question.is_trimmable() {
                trim(&autocomplete)
            } else {
                bstr::BString::from(autocomplete)
            });
        } else {
            let mut r: PhpMixed = PhpMixed::Bool(false);
            if question.is_hidden() {
                match self.inner().get_hidden_response(
                    std::rc::Rc::clone(&output),
                    &input_stream,
                    question.is_trimmable(),
                )? {
                    Ok(hidden_response) => {
                        r = PhpMixed::string(if question.is_trimmable() {
                            trim(&hidden_response)
                        } else {
                            bstr::BString::from(hidden_response)
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
                let is_blocked = stream_get_meta_data(&input_stream).blocked;

                if !is_blocked {
                    stream_set_blocking(&input_stream, true);
                }

                let read = self.inner().read_input(&input_stream, question);

                if !is_blocked {
                    stream_set_blocking(&input_stream, false);
                }

                if matches!(read, PhpMixed::Bool(false)) {
                    return Ok(Err(MissingInputException::new("Aborted.".to_string())));
                }
                r = read;
                if question.is_trimmable() {
                    r = PhpMixed::string(trim(r.to_string()));
                }
            }
            ret = r;
        }

        let mut ret = ret;
        {
            let borrowed = output.borrow();
            if let Some(section_output) =
                (*borrowed).as_any().downcast_ref::<ConsoleSectionOutput>()
            {
                section_output.add_content(bstr::BStr::new(&ret.to_string()));
            }
        }

        ret = if strlen(ret.to_string()) > 0 {
            ret
        } else {
            question.get_default()
        };

        if let Some(normalizer) = question.get_normalizer() {
            return Ok(Ok(normalizer(ret)));
        }

        Ok(Ok(ret))
    }

    /// Validates an attempt (PHP private; on the trait so it can late-bind the hooks).
    fn validate_attempts(
        &self,
        interviewer: &dyn Fn() -> anyhow::Result<Result<PhpMixed, MissingInputException>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) -> anyhow::Result<Result<PhpMixed, MissingInputException>> {
        let mut error: Option<Exception> = None;
        let mut attempts = question.get_max_attempts();

        loop {
            // while (null === $attempts || $attempts--)
            match attempts {
                None => {}
                Some(0) => break,
                Some(n) => attempts = Some(n - 1),
            }

            if let Some(ref error) = error {
                self.write_error(std::rc::Rc::clone(&output), error);
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
                    error = Some(Exception::with_code(
                        e.get_message().to_string(),
                        e.get_code(),
                    ));
                }
            }
        }

        // throw $error;
        Err(anyhow::Error::msg(
            error
                .map(|e| e.get_message().to_string())
                .unwrap_or_default(),
        ))
    }

    /// Outputs the question prompt (PHP protected; the overridable hook).
    fn write_prompt(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) {
        self.inner().write_prompt(output, question);
    }

    /// Outputs an error message (PHP protected; the overridable hook).
    fn write_error(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        error: &Exception,
    ) {
        self.inner().write_error(output, error);
    }
}

impl QuestionHelperInterface for QuestionHelper {
    fn inner(&self) -> &QuestionHelper {
        self
    }

    fn inner_mut(&mut self) -> &mut QuestionHelper {
        self
    }
}

impl QuestionHelper {
    pub fn get_name(&self) -> String {
        "question".to_string()
    }

    /// Prevents usage of stty.
    pub fn disable_stty() {
        STTY.store(false, std::sync::atomic::Ordering::SeqCst);
    }

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
                    .get(B(&default.to_string()))
                    .cloned()
                    .unwrap_or(default);
            }

            let default_parts = explode(",", default.to_string());
            let mut resolved: indexmap::IndexMap<BString, PhpMixed> = indexmap::IndexMap::new();
            for (k, v) in default_parts.iter().enumerate() {
                let v = if question.is_trimmable() {
                    trim(v)
                } else {
                    v.clone()
                };
                let value = choices.get(B(&v)).cloned().unwrap_or(PhpMixed::string(v));
                resolved.insert(k.to_string().into(), value);
            }

            return PhpMixed::Array(resolved);
        }

        default
    }

    /// Outputs the question prompt.
    fn write_prompt(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
    ) {
        let mut message = BString::from(question.get_question());

        if let Some(choice_question) = question.as_choice() {
            let mut lines = vec![BString::from(question.get_question())];
            lines.extend(self.format_choice_question_choices(choice_question, "info"));
            output
                .borrow()
                .writeln(&lines, output_interface::OUTPUT_NORMAL);

            message = BString::from(choice_question.get_prompt());
        }

        output
            .borrow()
            .write(&[message], false, output_interface::OUTPUT_NORMAL);
    }

    pub(crate) fn format_choice_question_choices(
        &self,
        question: &ChoiceQuestion,
        tag: &str,
    ) -> Vec<BString> {
        let mut messages: Vec<BString> = vec![];

        let choices = question.get_choices();
        let max_width = choices
            .keys()
            // TODO(bytes): widths are measured on UTF-8 text, so non-UTF-8 bytes are replaced.
            .map(Helper::width)
            .max()
            .unwrap_or(0);

        for (key, value) in choices {
            let padding = str_repeat(" ", (max_width - Helper::width(key)) as usize);

            messages.push(bconcat!(
                "  [<",
                tag,
                ">",
                key,
                padding,
                "</",
                tag,
                ">] ",
                php_to_string(value),
            ));
        }

        messages
    }

    /// Outputs an error message.
    fn write_error(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        error: &Exception,
    ) {
        let message = if let Some(helper_set) = self.get_helper_set() {
            let formatter = helper_set.borrow().get_formatter();

            formatter.borrow().format_block(
                FormatBlockMessages::String(error.get_message().to_string()),
                "error",
                false,
            )
        } else {
            format!("<error>{}</error>", error.get_message())
        };

        output.borrow().writeln(
            &[bstr::BString::from(message)],
            output_interface::OUTPUT_NORMAL,
        );
    }

    /// Autocompletes a question.
    fn autocomplete(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        question: &impl QuestionInterface,
        input_stream: &PhpResource,
        autocomplete: &dyn Fn(&str) -> Vec<PhpMixed>,
    ) -> Result<String, MissingInputException> {
        let cursor = Cursor::new(std::rc::Rc::clone(&output), Some(input_stream.clone()));

        let mut full_choice = String::new();
        let mut ret = String::new();

        let mut i: i64 = 0;
        let mut ofs: i64 = -1;
        let mut matches = autocomplete(&ret);
        let mut num_matches = matches.len() as i64;

        let stty_mode = shell_exec("stty -g").unwrap_or_default();
        let is_stdin = stream_get_meta_data(input_stream)
            .uri
            .is_some_and(|uri| uri == "php://stdin");
        let mut r = vec![input_stream.clone()];
        let w: Vec<PhpResource> = vec![];

        // Disable icanon (so we can fread each keypress) and echo (we'll do echoing here instead)
        shell_exec("stty -icanon -echo");

        // Add highlighted text style
        output.borrow().get_formatter().borrow_mut().set_style(
            bstr::BStr::new("hl"),
            Box::new(OutputFormatterStyle::new(
                Some(bstr::BStr::new("black")),
                Some(bstr::BStr::new("white")),
                vec![],
            )),
        );

        // Read a keypress
        while !feof(input_stream) {
            while is_stdin
                && Some(0) == stream_select(&mut r, &mut w.clone(), &mut w.clone(), 0, Some(100))
            {
                // Give signal handlers a chance to run
                r = vec![input_stream.clone()];
            }
            let mut c = fread(input_stream, 1);

            // as opposed to fgets(), fread() returns an empty string when the stream content is empty, not false.
            if c.is_none()
                || (ret.is_empty()
                    && c.as_deref() == Some(b"".as_slice())
                    && matches!(question.get_default(), PhpMixed::Null))
            {
                shell_exec(format!("stty {}", stty_mode));
                return Err(MissingInputException::new("Aborted.".to_string()));
            } else if c.as_deref() == Some(b"\x7f".as_slice()) {
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
            } else if c.as_deref() == Some(b"\x1b".as_slice()) {
                // Did we read an escape sequence?
                let escape = fread(input_stream, 2).unwrap_or_default();
                let cc = [c.clone().unwrap_or_default(), escape].concat();
                c = Some(cc.clone());

                // A = Up Arrow. B = Down Arrow
                let c2 = cc.get(2).copied();
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
            } else if ord(c.as_deref().unwrap_or_default()) < 32 {
                if c.as_deref() == Some(b"\t".as_slice()) || c.as_deref() == Some(b"\n".as_slice())
                {
                    if num_matches > 0 && -1 != ofs {
                        ret = matches[ofs as usize].to_string();
                        // Echo out remaining chars for current match
                        let remaining_characters = substr(
                            &ret,
                            strlen(trim(self.most_recently_entered_value(&full_choice))),
                            None,
                        );
                        output.borrow().write(
                            std::slice::from_ref(&bstr::BString::from(
                                legacy_lossy(&remaining_characters).into_owned().clone(),
                            )),
                            false,
                            output_interface::OUTPUT_NORMAL,
                        );
                        full_choice.push_str(&legacy_lossy(&remaining_characters));
                        i = match mb_detect_encoding(&full_choice, None, true) {
                            None => strlen(&full_choice),
                            Some(encoding) => mb_strlen(&full_choice, &encoding),
                        };

                        let ret_for_filter = ret.clone();
                        matches = autocomplete(&ret)
                            .into_iter()
                            .filter(|m| {
                                ret_for_filter.is_empty()
                                    || m.to_string().starts_with(&ret_for_filter)
                            })
                            .collect();
                        ofs = -1;
                    }

                    if c.as_deref() == Some(b"\n".as_slice()) {
                        output.borrow().write(
                            &[bstr::BString::from("\n".to_string())],
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
                if b"\x80".as_slice() <= cur.as_slice() {
                    let len = match str_bitand(&cur, b"\xf0").as_slice() {
                        b"\xc0" => 1,
                        b"\xd0" => 1,
                        b"\xe0" => 2,
                        b"\xf0" => 3,
                        _ => 0,
                    };
                    let extra = fread(input_stream, len).unwrap_or_default();
                    c = Some([cur, extra].concat());
                }

                // TODO(bytes): the output, the answer and the autocomplete matching all work on
                // Strings.
                let cur = String::from_utf8_lossy(&c.clone().unwrap_or_default()).into_owned();
                output.borrow().write(
                    std::slice::from_ref(&bstr::BString::from(cur.clone())),
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
                    if value.to_string().starts_with(&temp_ret) {
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
                let characters_entered =
                    strlen(trim(self.most_recently_entered_value(&full_choice)));
                output.borrow().write(
                    &[bstr::BString::from(format!(
                        "<hl>{}</hl>",
                        OutputFormatter::escape_trailing_backslash(substr(
                            matches[ofs as usize].to_string(),
                            characters_entered,
                            None,
                        ))
                    ))],
                    false,
                    output_interface::OUTPUT_NORMAL,
                );
                cursor.restore_position();
            }
        }

        // Reset stty so it behaves normally again
        shell_exec(format!("stty {}", stty_mode));

        Ok(full_choice)
    }

    fn most_recently_entered_value(&self, entered: &str) -> String {
        // Determine the most recent value that the user entered
        if !entered.contains(',') {
            return entered.to_string();
        }

        let choices = explode(",", entered);
        let last_choice = trim(&choices[choices.len() - 1]);
        if !last_choice.is_empty() {
            return legacy_lossy(&last_choice).into_owned();
        }

        entered.to_string()
    }

    /// Gets a hidden response from user.
    fn get_hidden_response(
        &self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        input_stream: &PhpResource,
        trimmable: bool,
    ) -> anyhow::Result<Result<String, RuntimeException>> {
        if cfg!(windows) {
            let mut exe = format!("{}/../Resources/bin/hiddeninput.exe", dir());

            // handle code running from a phar
            let mut tmp_exe: Option<String> = None;
            if substr(magic_file(), 0, Some(5)) == "phar:" {
                // TODO(bytes): the executable path is kept as a String.
                let tmp_dir = sys_get_temp_dir()
                    .into_os_string()
                    .into_string()
                    .unwrap_or_else(|dir| panic!("temp dir is not valid UTF-8: {dir:?}"));
                let tmp = format!("{}/hiddeninput.exe", tmp_dir);
                let _ = copy(&exe, &tmp);
                exe = tmp.clone();
                tmp_exe = Some(tmp);
            }

            let s_exec = shell_exec(format!("\"{}\"", exe)).unwrap_or_default();
            let value = if trimmable { rtrim(&s_exec) } else { s_exec };
            output.borrow().writeln(
                &[bstr::BString::from("".to_string())],
                output_interface::OUTPUT_NORMAL,
            );

            if let Some(tmp) = tmp_exe {
                unlink(&tmp);
            }

            return Ok(Ok(legacy_lossy(&value).into_owned()));
        }

        let mut stty_mode = String::new();
        if STTY.load(std::sync::atomic::Ordering::SeqCst) && Terminal::has_stty_available() {
            stty_mode = legacy_lossy(&shell_exec("stty -g").unwrap_or_default()).into_owned();
            shell_exec("stty -echo");
        } else if self.is_interactive_input(input_stream) {
            return Ok(Err(RuntimeException::new(
                "Unable to hide the response.".to_string(),
            )));
        }

        let value = fgets(input_stream, Some(4096));

        if STTY.load(std::sync::atomic::Ordering::SeqCst) && Terminal::has_stty_available() {
            shell_exec(format!("stty {}", stty_mode));
        }

        let mut value = match value {
            // TODO(bytes): the answer is returned as a String.
            Some(value) => String::from_utf8_lossy(&value).into_owned(),
            None => {
                return Err(MissingInputException::new("Aborted.".to_string()).into());
            }
        };
        if trimmable {
            value = legacy_lossy(&trim(&value)).into_owned();
        }
        output.borrow().writeln(
            &[bstr::BString::from("".to_string())],
            output_interface::OUTPUT_NORMAL,
        );

        Ok(Ok(value))
    }

    fn is_interactive_input(&self, input_stream: &PhpResource) -> bool {
        let uri = stream_get_meta_data(input_stream).uri;
        if uri.is_none_or(|uri| uri != "php://stdin") {
            return false;
        }

        let mut stdin_is_interactive = STDIN_IS_INTERACTIVE.lock().unwrap();
        if let Some(value) = *stdin_is_interactive {
            return value;
        }

        let value = stream_isatty_resource(&php_fopen_resource("php://stdin", "r"));
        *stdin_is_interactive = Some(value);
        value
    }

    /// Reads one or more lines of input and returns what is read.
    fn read_input(
        &self,
        input_stream: &PhpResource,
        question: &impl QuestionInterface,
    ) -> PhpMixed {
        if !question.is_multiline() {
            let cp = self.set_io_codepage();
            let ret = fgets(input_stream, Some(4096));

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

        let mut ret = BString::default();
        let cp = self.set_io_codepage();
        loop {
            let char = fgetc(&multi_line_stream_reader);
            let char = match char {
                Some(char) => char,
                None => break,
            };
            if PHP_EOL.as_bytes() == [ret.as_slice(), char.as_slice()].concat() {
                break;
            }
            ret.extend_from_slice(&char);
        }

        self.reset_io_codepage(cp, PhpMixed::String(ret))
    }

    /// Sets console I/O to the host code page.
    fn set_io_codepage(&self) -> i64 {
        if function_exists("sapi_windows_cp_set") {
            let cp = sapi_windows_cp_get(None);
            sapi_windows_cp_set(sapi_windows_cp_get(Some("oem")));

            return cp;
        }

        0
    }

    /// Sets console I/O to the specified code page and converts the user input.
    fn reset_io_codepage(&self, cp: i64, input: PhpMixed) -> PhpMixed {
        let mut input = input;
        if 0 != cp {
            sapi_windows_cp_set(cp);

            if !matches!(input, PhpMixed::Bool(false)) && input.to_string() != "" {
                input = PhpMixed::string(sapi_windows_cp_conv(
                    sapi_windows_cp_get(Some("oem")),
                    cp,
                    &input.to_string(),
                ));
            }
        }

        input
    }

    /// Clones an input stream in order to act on one instance of the same
    /// stream without affecting the other instance.
    fn clone_input_stream(&self, input_stream: &PhpResource) -> Option<PhpResource> {
        let stream_meta_data = stream_get_meta_data(input_stream);
        let seekable = stream_meta_data.seekable;
        let mode = stream_meta_data.mode;
        let uri = stream_meta_data.uri;

        let uri = uri?;

        let clone_stream = fopen(path_from_bytes(&uri), &mode).ok()?;

        // For seekable and writable streams, add all the same data to the
        // cloned stream and then seek to the same offset.
        if seekable && !["r", "rb", "rt"].contains(&mode.as_str()) {
            let offset = ftell(input_stream).unwrap_or(0);
            rewind(input_stream);
            stream_copy_to_stream(input_stream, &clone_stream);
            fseek(input_stream, offset, SEEK_SET);
            fseek(&clone_stream, offset, SEEK_SET);
        }

        Some(clone_stream)
    }

    /// Helper::substr proxy (inherited static helper).
    fn substr(string: Option<&str>, from: i64, length: Option<i64>) -> String {
        legacy_lossy(&Helper::substr(string.unwrap_or_default(), from, length)).into_owned()
    }
}

/// PHP `__FILE__` magic constant. The executing code lives in the Shirabe binary itself, which is
/// the closest analogue for a native executable; it never carries the `phar:` scheme, so the
/// phar-relocation branch in `get_hidden_response` correctly never triggers.
fn magic_file() -> String {
    std::env::current_exe()
        .expect("current executable path")
        .display()
        .to_string()
}

impl HelperInterface for QuestionHelper {
    fn set_helper_set(&mut self, helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>) {
        self.inner.set_helper_set(helper_set);
    }

    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>> {
        self.inner.get_helper_set()
    }

    fn get_name(&self) -> String {
        self.get_name()
    }
}
