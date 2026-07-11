//! ref: composer/tests/Composer/Test/Mock/IOMock.php

use shirabe::config::Config;
use shirabe::io::buffer_io::BufferIO;
use shirabe::io::io_interface;
use shirabe::io::{IOInterface, IOInterfaceImmutable, IOInterfaceMutable};
use shirabe::util::platform::Platform;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::output::output_interface;
use shirabe_php_shim::{PHP_EOL, PhpMixed, preg_quote};
use std::collections::VecDeque;

// A single entry of the IO expectation list. PHP models these as associative
// arrays (`{text, regex?}` / `{ask, reply}` / `{auth: [repo, user, pass]}`); the
// `verbosity?` key from the original docblock is dropped because `assertComplete`
// never reads it and no test sets it.
#[derive(Debug, Clone)]
pub enum Expectation {
    Text {
        text: String,
        regex: bool,
    },
    Ask {
        ask: String,
        reply: String,
    },
    Auth {
        repository_name: String,
        username: String,
        password: Option<String>,
    },
}

impl Expectation {
    pub fn text(text: impl Into<String>) -> Self {
        Expectation::Text {
            text: text.into(),
            regex: false,
        }
    }

    pub fn text_regex(text: impl Into<String>) -> Self {
        Expectation::Text {
            text: text.into(),
            regex: true,
        }
    }

    pub fn ask(ask: impl Into<String>, reply: impl Into<String>) -> Self {
        Expectation::Ask {
            ask: ask.into(),
            reply: reply.into(),
        }
    }

    pub fn auth(
        repository_name: impl Into<String>,
        username: impl Into<String>,
        password: Option<String>,
    ) -> Self {
        Expectation::Auth {
            repository_name: repository_name.into(),
            username: username.into(),
            password,
        }
    }
}

#[derive(Debug)]
pub struct IOMock {
    inner: BufferIO,
    expectations: Option<Vec<Expectation>>,
    strict: bool,
    auth_log: Vec<(String, String, Option<String>)>,
}

impl IOMock {
    pub fn new(verbosity: i64) -> anyhow::Result<Self> {
        let sf_verbosity = match verbosity {
            io_interface::QUIET => output_interface::VERBOSITY_QUIET,
            io_interface::NORMAL => output_interface::VERBOSITY_NORMAL,
            io_interface::VERBOSE => output_interface::VERBOSITY_VERBOSE,
            io_interface::VERY_VERBOSE => output_interface::VERBOSITY_VERY_VERBOSE,
            io_interface::DEBUG => output_interface::VERBOSITY_DEBUG,
            other => panic!("unknown IOInterface verbosity: {other}"),
        };

        Ok(Self {
            inner: BufferIO::new(String::new(), sf_verbosity, None)?,
            expectations: None,
            strict: false,
            auth_log: Vec::new(),
        })
    }

    // `$strict` set to true requires *all* expected messages, not just a subset.
    pub fn expects(&mut self, expectations: Vec<Expectation>, strict: bool) -> anyhow::Result<()> {
        let inputs: Vec<String> = expectations
            .iter()
            .filter_map(|expect| match expect {
                Expectation::Ask { reply, .. } => Some(reply.clone()),
                _ => None,
            })
            .collect();

        if !inputs.is_empty() {
            self.inner.set_user_inputs(inputs)?;
        }

        self.expectations = Some(expectations);
        self.strict = strict;

        Ok(())
    }

    pub fn assert_complete(&self) {
        let output = self.inner.get_output();

        if Platform::get_env("DEBUG_OUTPUT").as_deref() == Some("1") {
            println!("{PHP_EOL}Collected output: {output}{PHP_EOL}");
        }

        // Not configured to expect anything, so there is nothing to verify.
        let Some(expectations) = &self.expectations else {
            return;
        };

        if !expectations.is_empty() {
            let mut lines: VecDeque<String> = Preg::split("{\r?\n}", &output).into();
            let mut auth_log: VecDeque<(String, String, Option<String>)> =
                self.auth_log.clone().into();

            'expects: for expect in expectations {
                if let Expectation::Auth {
                    repository_name,
                    username,
                    password,
                } = expect
                {
                    let expected = (repository_name.clone(), username.clone(), password.clone());
                    while let Some(auth) = auth_log.pop_front() {
                        if auth == expected {
                            continue 'expects;
                        }

                        if self.strict {
                            panic!(
                                "IO authentication mismatch. Expected:{PHP_EOL}{expected:?}{PHP_EOL}Got:{PHP_EOL}{auth:?}"
                            );
                        }
                    }

                    panic!(
                        "Expected \"{expected:?}\" auth to be set but there are no setAuthentication calls left to consume."
                    );
                }

                let (pattern, label) = match expect {
                    Expectation::Ask { ask, .. } => {
                        (format!("{{^{}$}}", preg_quote(ask, None)), ask.clone())
                    }
                    Expectation::Text { text, regex: true } => (text.clone(), text.clone()),
                    Expectation::Text { text, regex: false } => {
                        (format!("{{^{}$}}", preg_quote(text, None)), text.clone())
                    }
                    Expectation::Auth { .. } => unreachable!("handled above"),
                };

                while let Some(line) = lines.pop_front() {
                    if Preg::is_match(&pattern, &line) {
                        continue 'expects;
                    }

                    if self.strict {
                        panic!(
                            "IO output mismatch. Expected:{PHP_EOL}{label}{PHP_EOL}Got:{PHP_EOL}{line}"
                        );
                    }
                }

                panic!(
                    "Expected \"{label}\" to be output still but there is no output left to consume. Complete output:{PHP_EOL}{output}"
                );
            }
        } else if !output.is_empty() && self.strict {
            panic!("There was strictly no output expected but some output occurred: {output}");
        }
    }
}

impl IOInterfaceImmutable for IOMock {
    fn is_interactive(&self) -> bool {
        self.inner.is_interactive()
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
    fn is_decorated(&self) -> bool {
        self.inner.is_decorated()
    }
    fn write3(&self, message: &str, newline: bool, verbosity: i64) {
        self.inner.write3(message, newline, verbosity)
    }
    fn write_error3(&self, message: &str, newline: bool, verbosity: i64) {
        self.inner.write_error3(message, newline, verbosity)
    }
    fn write_raw3(&self, message: &str, newline: bool, verbosity: i64) {
        self.inner.write_raw3(message, newline, verbosity)
    }
    fn write_error_raw3(&self, message: &str, newline: bool, verbosity: i64) {
        self.inner.write_error_raw3(message, newline, verbosity)
    }
    fn overwrite4(&self, message: &str, newline: bool, size: Option<i64>, verbosity: i64) {
        self.inner.overwrite4(message, newline, size, verbosity)
    }
    fn overwrite_error4(&self, message: &str, newline: bool, size: Option<i64>, verbosity: i64) {
        self.inner
            .overwrite_error4(message, newline, size, verbosity)
    }

    fn ask(&self, question: String, default: PhpMixed) -> PhpMixed {
        self.inner
            .ask(format!("{}{}", trim_eol(&question), PHP_EOL), default)
    }
    fn ask_confirmation(&self, question: String, default: bool) -> bool {
        self.inner
            .ask_confirmation(format!("{}{}", trim_eol(&question), PHP_EOL), default)
    }
    fn ask_and_validate(
        &self,
        question: String,
        validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>>,
        attempts: Option<i64>,
        default: PhpMixed,
    ) -> anyhow::Result<PhpMixed> {
        self.inner.ask_and_validate(
            format!("{}{}", trim_eol(&question), PHP_EOL),
            validator,
            attempts,
            default,
        )
    }
    fn ask_and_hide_answer(&self, question: String) -> Option<String> {
        // Do not hide the answer in tests because that blocks on Windows with
        // hiddeninput.exe, so PHP delegates to `ask` rather than the hidden variant.
        let result = self.inner.ask(
            format!("{}{}", trim_eol(&question), PHP_EOL),
            PhpMixed::Null,
        );
        result.as_string().map(|s| s.to_string())
    }
    fn select(
        &self,
        question: String,
        choices: Vec<String>,
        default: PhpMixed,
        attempts: PhpMixed,
        error_message: String,
        multiselect: bool,
    ) -> PhpMixed {
        self.inner.select(
            format!("{}{}", trim_eol(&question), PHP_EOL),
            choices,
            default,
            attempts,
            error_message,
            multiselect,
        )
    }

    fn get_authentications(
        &self,
    ) -> indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>> {
        self.inner.get_authentications()
    }
    fn has_authentication(&self, repository_name: &str) -> bool {
        self.inner.has_authentication(repository_name)
    }
    fn get_authentication(
        &self,
        repository_name: &str,
    ) -> indexmap::IndexMap<String, Option<String>> {
        self.inner.get_authentication(repository_name)
    }
    fn error(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.error(message, context)
    }
    fn warning(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.warning(message, context)
    }
    fn debug(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.debug(message, context)
    }
}

impl IOInterfaceMutable for IOMock {
    fn set_authentication(
        &mut self,
        repository_name: String,
        username: String,
        password: Option<String>,
    ) {
        self.auth_log
            .push((repository_name.clone(), username.clone(), password.clone()));
        self.inner
            .set_authentication(repository_name, username, password)
    }
    fn load_configuration(&mut self, config: &mut Config) -> anyhow::Result<()> {
        self.inner.load_configuration(config)
    }
}

impl IOInterface for IOMock {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_base_io_mut(&mut self) -> Option<&mut dyn shirabe::io::BaseIO> {
        Some(self)
    }
}

impl shirabe::io::BaseIO for IOMock {
    fn authentications(
        &self,
    ) -> &indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>> {
        self.inner.authentications()
    }
    fn authentications_mut(
        &mut self,
    ) -> &mut indexmap::IndexMap<String, indexmap::IndexMap<String, Option<String>>> {
        self.inner.authentications_mut()
    }
}

fn trim_eol(question: &str) -> &str {
    question.trim_end_matches(['\r', '\n'])
}

pub struct IOMockGuard(std::rc::Rc<std::cell::RefCell<IOMock>>);

impl Drop for IOMockGuard {
    fn drop(&mut self) {
        // Avoid aborting on a double panic when a test assertion is already unwinding.
        if std::thread::panicking() {
            return;
        }
        self.0.borrow().assert_complete();
    }
}

// For testing only. Mirrors TestCase::getIOMock: returns a shared IOMock handle
// plus a guard that runs assert_complete when it drops at the end of the test scope.
pub fn get_io_mock(
    verbosity: i64,
) -> anyhow::Result<(std::rc::Rc<std::cell::RefCell<IOMock>>, IOMockGuard)> {
    let mock = std::rc::Rc::new(std::cell::RefCell::new(IOMock::new(verbosity)?));
    Ok((mock.clone(), IOMockGuard(mock)))
}
