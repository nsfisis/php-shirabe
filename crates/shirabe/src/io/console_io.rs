//! ref: composer/src/Composer/IO/ConsoleIO.php

use crate::config::Config;
use crate::io::io_interface;
use indexmap::IndexMap;
use indexmap::indexmap;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::helper::HelperSet;
use shirabe_external_packages::symfony::console::helper::ProgressBar;
use shirabe_external_packages::symfony::console::helper::QuestionHelper;
use shirabe_external_packages::symfony::console::helper::Table;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::output::ConsoleOutput;
use shirabe_external_packages::symfony::console::output::ConsoleOutputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::{
    self as output_interface, OutputInterface,
};
use shirabe_external_packages::symfony::console::question::ChoiceQuestion;
use shirabe_external_packages::symfony::console::question::Question;
use shirabe_php_shim::{
    AsAny, PhpMixed, array_search, function_exists, implode, in_array, is_array, is_string,
    mb_check_encoding, mb_convert_encoding, microtime, sprintf, str_repeat, strip_tags, strlen,
};
use std::cell::RefCell;

use crate::io::BaseIO;
use crate::io::IOInterface;
use crate::io::IOInterfaceImmutable;
use crate::io::IOInterfaceMutable;
use crate::question::StrictConfirmationQuestion;
use crate::util::Silencer;

/// The Input/Output helper.
#[derive(Debug)]
pub struct ConsoleIO {
    authentications: indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>>,

    pub(crate) input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
    pub(crate) output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
    pub(crate) helper_set: HelperSet,
    pub(crate) last_message: RefCell<String>,
    pub(crate) last_message_err: RefCell<String>,

    /// @var float
    start_time: Option<f64>,
    /// @var array<IOInterface::*, OutputInterface::VERBOSITY_*>
    verbosity_map: IndexMap<i64, i64>,
}

impl ConsoleIO {
    /// Constructor.
    ///
    /// @param InputInterface  $input     The input instance
    /// @param OutputInterface $output    The output instance
    /// @param HelperSet       $helperSet The helperSet instance
    pub fn new(
        input: std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        helper_set: HelperSet,
    ) -> Self {
        let mut verbosity_map = IndexMap::new();
        verbosity_map.insert(io_interface::QUIET, output_interface::VERBOSITY_QUIET);
        verbosity_map.insert(io_interface::NORMAL, output_interface::VERBOSITY_NORMAL);
        verbosity_map.insert(io_interface::VERBOSE, output_interface::VERBOSITY_VERBOSE);
        verbosity_map.insert(
            io_interface::VERY_VERBOSE,
            output_interface::VERBOSITY_VERY_VERBOSE,
        );
        verbosity_map.insert(io_interface::DEBUG, output_interface::VERBOSITY_DEBUG);
        Self {
            authentications: indexmap![],
            input,
            output,
            helper_set,
            last_message: RefCell::new(String::new()),
            last_message_err: RefCell::new(String::new()),
            start_time: None,
            verbosity_map,
        }
    }

    pub fn enable_debugging(&mut self, start_time: f64) {
        self.start_time = Some(start_time);
    }

    /// @param string[]|string $messages
    fn do_write(&self, messages: PhpMixed, newline: bool, stderr: bool, verbosity: i64, raw: bool) {
        let mut sf_verbosity = *self.verbosity_map.get(&verbosity).unwrap_or(&0);
        if sf_verbosity > self.output.borrow().get_verbosity() {
            return;
        }

        if raw {
            sf_verbosity |= output_interface::OUTPUT_RAW;
        }

        let messages = if let Some(start_time) = self.start_time {
            let memory_usage = (shirabe_php_shim::memory_get_usage() as f64) / 1024.0 / 1024.0;
            let time_spent = microtime(true) - start_time;
            // PHP: array_map(fn ($message): string => sprintf(...), (array) $messages)
            let arr: Vec<String> = match &messages {
                PhpMixed::String(s) => vec![s.clone()],
                PhpMixed::List(l) => l
                    .iter()
                    .filter_map(|v| v.as_string().map(|s| s.to_string()))
                    .collect(),
                _ => vec![],
            };
            let mapped: Vec<String> = arr
                .into_iter()
                .map(|message| {
                    format!(
                        "[{:.1}MiB/{:.2}s] {}",
                        memory_usage,
                        time_spent,
                        PhpMixed::String(message),
                    )
                })
                .collect();
            PhpMixed::List(
                mapped
                    .into_iter()
                    .map(|s| Box::new(PhpMixed::String(s)))
                    .collect(),
            )
        } else {
            messages
        };

        let error_output = if stderr {
            let output = self.output.borrow();
            (*output)
                .as_any()
                .downcast_ref::<ConsoleOutput>()
                .map(|console_output| console_output.get_error_output())
        } else {
            None
        };
        if let Some(error_output) = error_output {
            error_output.borrow().write(
                &[Self::to_string_list(&messages).join(if newline { "\n" } else { "" })],
                newline,
                sf_verbosity,
            );
            // PHP: implode($newline ? "\n" : '', (array) $messages)
            *self.last_message_err.borrow_mut() = implode(
                if newline { "\n" } else { "" },
                &Self::to_string_list(&messages),
            );

            return;
        }

        self.output.borrow().write(
            &[Self::to_string_list(&messages).join(if newline { "\n" } else { "" })],
            newline,
            sf_verbosity,
        );
        *self.last_message.borrow_mut() = implode(
            if newline { "\n" } else { "" },
            &Self::to_string_list(&messages),
        );
    }

    /// @param string[]|string $messages
    fn do_overwrite(
        &self,
        messages: PhpMixed,
        newline: bool,
        size: Option<i64>,
        stderr: bool,
        verbosity: i64,
    ) {
        // messages can be an array, let's convert it to string anyway
        let messages_str = implode(
            if newline { "\n" } else { "" },
            &Self::to_string_list(&messages),
        );

        // since overwrite is supposed to overwrite last message...
        let size = size.unwrap_or_else(|| {
            // removing possible formatting of lastMessage with strip_tags
            let last = if stderr {
                self.last_message_err.borrow().clone()
            } else {
                self.last_message.borrow().clone()
            };
            strlen(&strip_tags(&last))
        });
        // ...let's fill its length with backspaces
        self.do_write(
            PhpMixed::String(str_repeat("\x08", size as usize)),
            false,
            stderr,
            verbosity,
            false,
        );

        // write the new message
        self.do_write(
            PhpMixed::String(messages_str.clone()),
            false,
            stderr,
            verbosity,
            false,
        );

        // In cmd.exe on Win8.1 (possibly 10?), the line can not be cleared, so we need to
        // track the length of previous output and fill it with spaces to make sure the line is cleared.
        // See https://github.com/composer/composer/pull/5836 for more details
        let fill = size - strlen(&strip_tags(&messages_str));
        if fill > 0 {
            // whitespace whatever has left
            self.do_write(
                PhpMixed::String(str_repeat(" ", fill as usize)),
                false,
                stderr,
                verbosity,
                false,
            );
            // move the cursor back
            self.do_write(
                PhpMixed::String(str_repeat("\x08", fill as usize)),
                false,
                stderr,
                verbosity,
                false,
            );
        }

        if newline {
            self.do_write(
                PhpMixed::String(String::new()),
                true,
                stderr,
                verbosity,
                false,
            );
        }

        if stderr {
            *self.last_message_err.borrow_mut() = messages_str;
        } else {
            *self.last_message.borrow_mut() = messages_str;
        }
    }

    pub fn get_progress_bar(&self, max: i64) -> ProgressBar {
        // PHP ProgressBar::__construct default $minSecondsBetweenRedraws = 1 / 25.
        ProgressBar::new(self.get_error_output(), max, 1.0 / 25.0)
    }

    pub fn get_table(&self) -> Table {
        Table::new(self.output.clone())
    }

    fn get_error_output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> {
        let output = self.output.borrow();
        let error_output = (*output)
            .as_any()
            .downcast_ref::<ConsoleOutput>()
            .map(|console_output| console_output.get_error_output());
        drop(output);
        if let Some(error_output) = error_output {
            return error_output;
        }

        self.output.clone()
    }

    /// Sanitize string to remove control characters
    ///
    /// If $allowNewlines is true, \x0A (\n) and \x0D\x0A (\r\n) are let through. Single \r are still sanitized away to prevent overwriting whole lines.
    ///
    /// All other control chars (except NULL bytes) as well as ANSI escape sequences are removed.
    ///
    /// Invalid unicode sequences are turned into question marks.
    ///
    /// @param string|iterable<string> $messages
    /// @return string|array<string>
    /// @phpstan-return ($messages is string ? string : array<string>)
    pub fn sanitize(messages: PhpMixed, allow_newlines: bool) -> PhpMixed {
        // Match ANSI escape sequences:
        // - CSI (Control Sequence Introducer): ESC [ params intermediate final
        // - OSC (Operating System Command): ESC ] ... ESC \ or BEL
        // - Other ESC sequences: ESC followed by any character
        let escape_pattern =
            r"\x1B\[[\x30-\x3F]*[\x20-\x2F]*[\x40-\x7E]|\x1B\].*?(?:\x1B\\|\x07)|\x1B.";
        let pattern = if allow_newlines {
            format!(
                "{{{}|[\\x01-\\x09\\x0B\\x0C\\x0E-\\x1A]|\\r(?!\\n)}}u",
                escape_pattern
            )
        } else {
            format!("{{{}|[\\x01-\\x1A]}}u", escape_pattern)
        };
        if is_string(&messages) {
            let message = Self::ensure_valid_utf8(messages.as_string().unwrap_or(""));
            return PhpMixed::String(Preg::replace(&pattern, "", &message).unwrap_or_default());
        }

        // PHP: $sanitized = []; foreach ($messages as $key => $message) { ... }
        let mut sanitized: IndexMap<String, PhpMixed> = IndexMap::new();
        match &messages {
            PhpMixed::List(l) => {
                for (key, message) in l.iter().enumerate() {
                    let s = Self::ensure_valid_utf8(message.as_string().unwrap_or(""));
                    sanitized.insert(
                        key.to_string(),
                        PhpMixed::String(Preg::replace(&pattern, "", &s).unwrap_or_default()),
                    );
                }
            }
            PhpMixed::Array(a) => {
                for (key, message) in a {
                    let s = Self::ensure_valid_utf8(message.as_string().unwrap_or(""));
                    sanitized.insert(
                        key.clone(),
                        PhpMixed::String(Preg::replace(&pattern, "", &s).unwrap_or_default()),
                    );
                }
            }
            _ => {}
        }

        PhpMixed::Array(
            sanitized
                .into_iter()
                .map(|(k, v)| (k, Box::new(v)))
                .collect(),
        )
    }

    /// Ensures a string is valid UTF-8, replacing invalid byte sequences with '?'
    fn ensure_valid_utf8(string: &str) -> String {
        // Quick check: if string is already valid UTF-8, return as-is
        if function_exists("mb_check_encoding") && mb_check_encoding(string, "UTF-8") {
            return string.to_string();
        }

        // Use mb_convert_encoding to replace invalid sequences with '?'
        // This makes it visible when data quality issues occur
        if function_exists("mb_convert_encoding") {
            return mb_convert_encoding(string.as_bytes().to_vec(), "UTF-8", "UTF-8");
        }

        // Fallback to iconv if mbstring unavailable
        if function_exists("iconv") {
            let cleaned =
                Silencer::call(|| Ok(shirabe_php_shim::iconv("UTF-8", "UTF-8//TRANSLIT", string)));
            if let Ok(Some(c)) = cleaned {
                return c;
            }
        }

        // Last resort: return as-is (should never happen - Composer requires mbstring OR iconv)
        string.to_string()
    }

    /// Helper: PHP `(array) $messages` then collect strings
    fn to_string_list(messages: &PhpMixed) -> Vec<String> {
        match messages {
            PhpMixed::String(s) => vec![s.clone()],
            PhpMixed::List(l) => l
                .iter()
                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                .collect(),
            PhpMixed::Array(a) => a
                .values()
                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                .collect(),
            _ => vec![],
        }
    }

    /// Resolves the "question" helper and delegates to `QuestionHelper::ask`.
    ///
    /// PHP: `$this->helperSet->get('question')->ask($this->input, $this->getErrorOutput(), $question)`.
    /// `HelperSet::get` and `QuestionHelper::ask` both surface PHP exceptions; ConsoleIO does not
    /// catch them, so an unrecoverable error here is a PHP fatal. The double `Result` is collapsed:
    /// the outer `anyhow::Result` (fatal) and the inner `MissingInputException` (unhandled, hence
    /// also fatal in PHP) both abort.
    fn ask_question(&self, question: &Question) -> PhpMixed {
        let helper_rc = self
            .helper_set
            .get("question")
            .expect("the \"question\" helper is always registered");
        let error_output = self.get_error_output();
        let mut helper = helper_rc.borrow_mut();
        let question_helper = (*helper)
            .as_any_mut()
            .downcast_mut::<QuestionHelper>()
            .expect("the \"question\" helper is a QuestionHelper");
        let mut input = self.input.borrow_mut();
        question_helper
            .ask(&mut *input, error_output, question)
            .expect("QuestionHelper::ask raised a fatal error")
            .expect("QuestionHelper::ask returned no input")
    }
}

impl IOInterfaceImmutable for ConsoleIO {
    fn is_interactive(&self) -> bool {
        self.input.borrow().is_interactive()
    }

    fn is_verbose(&self) -> bool {
        self.output.borrow().is_verbose()
    }

    fn is_very_verbose(&self) -> bool {
        self.output.borrow().is_very_verbose()
    }

    fn is_debug(&self) -> bool {
        self.output.borrow().is_debug()
    }

    fn is_decorated(&self) -> bool {
        self.output.borrow().is_decorated()
    }

    fn write3(&self, message: &str, newline: bool, verbosity: i64) {
        let message = Self::sanitize(PhpMixed::String(message.to_string()), true);

        self.do_write(message, newline, false, verbosity, false);
    }

    fn write_error3(&self, message: &str, newline: bool, verbosity: i64) {
        let message = Self::sanitize(PhpMixed::String(message.to_string()), true);

        self.do_write(message, newline, true, verbosity, false);
    }

    fn write_raw3(&self, message: &str, newline: bool, verbosity: i64) {
        self.do_write(
            PhpMixed::String(message.to_string()),
            newline,
            false,
            verbosity,
            true,
        );
    }

    fn write_error_raw3(&self, message: &str, newline: bool, verbosity: i64) {
        self.do_write(
            PhpMixed::String(message.to_string()),
            newline,
            true,
            verbosity,
            true,
        );
    }

    fn overwrite4(&self, message: &str, newline: bool, size: Option<i64>, verbosity: i64) {
        self.do_overwrite(
            PhpMixed::String(message.to_string()),
            newline,
            size,
            false,
            verbosity,
        );
    }

    fn overwrite_error4(&self, message: &str, newline: bool, size: Option<i64>, verbosity: i64) {
        self.do_overwrite(
            PhpMixed::String(message.to_string()),
            newline,
            size,
            true,
            verbosity,
        );
    }

    fn ask(&self, question: String, default: PhpMixed) -> PhpMixed {
        let sanitized_question = Self::sanitize(PhpMixed::String(question), true)
            .as_string()
            .unwrap_or("")
            .to_string();
        let sanitized_default = if is_string(&default) {
            Some(Self::sanitize(default, true))
        } else {
            Some(default)
        };
        let question = Question::new(sanitized_question, sanitized_default);

        self.ask_question(&question)
    }

    fn ask_confirmation(&self, question: String, default: bool) -> bool {
        let sanitized = Self::sanitize(PhpMixed::String(question), true)
            .as_string()
            .unwrap_or("")
            .to_string();
        let question = StrictConfirmationQuestion::new(
            sanitized,
            default,
            "/^y(?:es)?$/i".to_string(),
            "/^no?$/i".to_string(),
        );

        let result = self.ask_question(question.inner());
        result.as_bool().unwrap_or(false)
    }

    fn ask_and_validate(
        &self,
        question: String,
        validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>>,
        attempts: Option<i64>,
        default: PhpMixed,
    ) -> anyhow::Result<PhpMixed> {
        let sanitized_question = Self::sanitize(PhpMixed::String(question), true)
            .as_string()
            .unwrap_or("")
            .to_string();
        let sanitized_default = if is_string(&default) {
            Some(Self::sanitize(default, true))
        } else {
            Some(default)
        };
        let mut question = Question::new(sanitized_question, sanitized_default);
        // Question::set_validator takes Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>;
        // adapt the None answer to PHP's null and translate the validator's anyhow error into the
        // InvalidArgumentException the Question validator type expects (QuestionHelper retries on it).
        let adapted: Box<
            dyn Fn(
                Option<PhpMixed>,
            ) -> Result<
                PhpMixed,
                shirabe_external_packages::symfony::console::exception::InvalidArgumentException,
            >,
        > = Box::new(move |answer: Option<PhpMixed>| {
            validator(answer.unwrap_or(PhpMixed::Null)).map_err(|e| {
                shirabe_external_packages::symfony::console::exception::InvalidArgumentException(
                    shirabe_php_shim::InvalidArgumentException {
                        message: e.to_string(),
                        code: 0,
                    },
                )
            })
        });
        question.set_validator(Some(adapted));
        question
            .set_max_attempts(attempts)
            .map_err(|e| anyhow::anyhow!(e.0.message))?;

        Ok(self.ask_question(&question))
    }

    fn ask_and_hide_answer(&self, question: String) -> Option<String> {
        let sanitized_question = Self::sanitize(PhpMixed::String(question), true)
            .as_string()
            .unwrap_or("")
            .to_string();
        let mut question = Question::new(sanitized_question, Some(PhpMixed::Null));
        // setHidden only throws when an autocompleter is set, which is not the case here.
        question
            .set_hidden(true)
            .expect("a freshly constructed question has no autocompleter");

        let result = self.ask_question(&question);
        result.as_string().map(|s| s.to_string())
    }

    fn select(
        &self,
        question: String,
        choices: Vec<String>,
        default: PhpMixed,
        // PHP: int|false attempts
        attempts: PhpMixed,
        error_message: String,
        multiselect: bool,
    ) -> PhpMixed {
        let choices: PhpMixed = PhpMixed::List(
            choices
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        );
        let _helper = self
            .helper_set
            .get("question")
            .expect("the \"question\" helper is always registered");
        let sanitized_question = Self::sanitize(PhpMixed::String(question), true)
            .as_string()
            .unwrap_or("")
            .to_string();
        // ChoiceQuestion::new expects an IndexMap<String, Box<PhpMixed>>; project the
        // sanitized choice list/map into the keyed form, preserving keys.
        let sanitized_choices_mixed = Self::sanitize(choices.clone(), true);
        let sanitized_choices: IndexMap<String, Box<PhpMixed>> = match sanitized_choices_mixed {
            PhpMixed::List(l) => l
                .into_iter()
                .enumerate()
                .map(|(i, b)| (i.to_string(), b))
                .collect(),
            PhpMixed::Array(a) => a,
            other => indexmap! { "0".to_string() => Box::new(other) },
        };
        let sanitized_default = if is_string(&default) {
            Some(Self::sanitize(default, true))
        } else {
            Some(default)
        };
        // Empty choices raise a LogicException (programmer error in PHP).
        let mut question =
            ChoiceQuestion::new(sanitized_question, sanitized_choices, sanitized_default)
                .expect("select() always provides at least one choice");
        // PHP: IOInterface requires false, and Question requires null or int
        let _max_attempts = match attempts {
            PhpMixed::Bool(false) => None,
            PhpMixed::Int(i) => Some(i),
            _ => None,
        };
        // TODO(phase-c): PHP calls $question->setMaxAttempts($attempts), inherited from Question.
        // ChoiceQuestion (shirabe-external-packages) keeps its inner Question private and exposes
        // no setMaxAttempts passthrough, so the attempts cannot be propagated here yet. See report.
        question.set_error_message(error_message);
        question.set_multiselect(multiselect);

        // TODO(phase-c): QuestionHelper::ask takes a concrete `&Question`, but PHP passes a
        // ChoiceQuestion whose getValidator/getDefault/autocompleter overrides drive the answer
        // handling via polymorphism. Passing the inner Question would silently drop that behaviour.
        // Faithful support needs the Symfony Question hierarchy modelled as a trait (or ask taking a
        // trait object) so ChoiceQuestion's overrides dispatch — unlike StrictConfirmationQuestion,
        // whose behaviour lives entirely on its inner Question (see ask_confirmation above).
        let result: PhpMixed = todo!("call QuestionHelper::ask with a polymorphic ChoiceQuestion");

        // PHP: $isAssoc = (bool) \count(array_filter(array_keys($choices), 'is_string'));
        let choice_keys: Vec<String> = match &choices {
            PhpMixed::Array(a) => a.keys().cloned().collect(),
            PhpMixed::List(_) => vec![],
            _ => vec![],
        };
        let is_assoc =
            !choice_keys.is_empty() && choice_keys.iter().any(|k| !k.parse::<i64>().is_ok());
        if is_assoc {
            return result;
        }

        if !is_array(&result) {
            let result_str = result.as_string().unwrap_or("").to_string();
            let haystack: IndexMap<String, String> = match &choices {
                PhpMixed::List(l) => l
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.as_string().map(|s| (i.to_string(), s.to_string())))
                    .collect(),
                _ => IndexMap::new(),
            };
            return PhpMixed::String(array_search(&result_str, &haystack).unwrap_or_default());
        }

        let mut results: Vec<String> = vec![];
        let result_list = result.as_list().cloned().unwrap_or_default();
        let choice_list: Vec<(String, PhpMixed)> = match &choices {
            PhpMixed::List(l) => l
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), (**v).clone()))
                .collect(),
            PhpMixed::Array(a) => a.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect(),
            _ => vec![],
        };
        for (index, choice) in &choice_list {
            if in_array(choice.clone(), &PhpMixed::List(result_list.clone()), true) {
                results.push(index.clone());
            }
        }

        PhpMixed::List(
            results
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        )
    }

    fn get_authentications(&self) -> IndexMap<String, IndexMap<String, Option<String>>> {
        <Self as BaseIO>::get_authentications(self)
    }

    fn has_authentication(&self, repository_name: &str) -> bool {
        <Self as BaseIO>::has_authentication(self, repository_name)
    }

    fn get_authentication(&self, repository_name: &str) -> IndexMap<String, Option<String>> {
        <Self as BaseIO>::get_authentication(self, repository_name)
    }

    fn error(&self, _message: &str, _context: &[(&str, &str)]) {
        todo!()
    }

    fn warning(&self, _message: &str, _context: &[(&str, &str)]) {
        todo!()
    }

    fn debug(&self, _message: &str, _context: &[(&str, &str)]) {
        todo!()
    }
}

impl IOInterfaceMutable for ConsoleIO {
    fn set_authentication(
        &mut self,
        repository_name: String,
        username: String,
        password: Option<String>,
    ) {
        <Self as BaseIO>::set_authentication(self, repository_name, username, password)
    }

    fn load_configuration(&mut self, config: &mut Config) -> anyhow::Result<()> {
        <Self as BaseIO>::load_configuration(self, config)
    }
}

impl IOInterface for ConsoleIO {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_base_io_mut(&mut self) -> Option<&mut dyn BaseIO> {
        Some(self)
    }
}

impl BaseIO for ConsoleIO {
    fn authentications(
        &self,
    ) -> &indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>> {
        &self.authentications
    }

    fn authentications_mut(
        &mut self,
    ) -> &mut indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>> {
        &mut self.authentications
    }
}
