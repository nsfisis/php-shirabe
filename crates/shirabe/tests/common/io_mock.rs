//! ref: composer/tests/Composer/Test/Mock/IOMock.php sha1:3bacded1f921d96583c78932d0678a08b83bc488

use bstr::{BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe::config::Config;
use shirabe::io::{BufferIO, IOInterface, io_interface};
use shirabe::util::Platform;
use shirabe_php_shim::{
    PHP_EOL, PhpProp, bformat, php_regex, preg_is_match, preg_quote, preg_split,
};
use shirabe_symfony_console::input::InputValue;
use shirabe_symfony_console::output::output_interface;

// A single entry of the IO expectation list. PHP models these as associative
// arrays (`{text, regex?}` / `{ask, reply}` / `{auth: [repo, user, pass]}`); the
// `verbosity?` key from the original docblock is dropped because `assertComplete`
// never reads it and no test sets it.
#[derive(Debug, Clone)]
pub enum Expectation {
    Text {
        text: BString,
        regex: bool,
    },
    Ask {
        ask: BString,
        reply: BString,
    },
    Auth {
        repository_name: BString,
        username: BString,
        password: Option<BString>,
    },
}

impl Expectation {
    pub fn text(text: impl Into<BString>) -> Self {
        Expectation::Text {
            text: text.into(),
            regex: false,
        }
    }

    pub fn text_regex(text: impl Into<BString>) -> Self {
        Expectation::Text {
            text: text.into(),
            regex: true,
        }
    }

    pub fn ask(ask: impl Into<BString>, reply: impl Into<BString>) -> Self {
        Expectation::Ask {
            ask: ask.into(),
            reply: reply.into(),
        }
    }

    pub fn auth(
        repository_name: impl Into<BString>,
        username: impl Into<BString>,
        password: Option<BString>,
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
    expectations: PhpProp<Option<Vec<Expectation>>>,
    strict: PhpProp<bool>,
    auth_log: PhpProp<Vec<(bstr::BString, bstr::BString, Option<bstr::BString>)>>,
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
            inner: BufferIO::new("", sf_verbosity, None)?,
            expectations: PhpProp::new(None),
            strict: PhpProp::new(false),
            auth_log: PhpProp::default(),
        })
    }

    // `$strict` set to true requires *all* expected messages, not just a subset.
    pub fn expects(&self, expectations: Vec<Expectation>, strict: bool) -> anyhow::Result<()> {
        let inputs: Vec<BString> = expectations
            .iter()
            .filter_map(|expect| match expect {
                Expectation::Ask { reply, .. } => Some(reply.clone()),
                _ => None,
            })
            .collect();

        if !inputs.is_empty() {
            self.inner.set_user_inputs(inputs)?;
        }

        self.expectations.set(Some(expectations));
        self.strict.set(strict);

        Ok(())
    }

    pub fn assert_complete(&self) {
        let output = self.inner.get_output();

        if Platform::get_env("DEBUG_OUTPUT").is_some_and(|v| v == "1") {
            println!("\nCollected output: {}\n", output.display_lossy());
        }

        // Not configured to expect anything, so there is nothing to verify.
        let Some(expectations) = &self.expectations.cloned() else {
            return;
        };

        if !expectations.is_empty() {
            let mut lines: std::collections::VecDeque<bstr::BString> =
                preg_split(php_regex!("{\r?\n}"), &output)
                    .into_iter()
                    .collect();
            let mut auth_log: std::collections::VecDeque<(
                bstr::BString,
                bstr::BString,
                Option<bstr::BString>,
            )> = self.auth_log.cloned().into();

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

                        if self.strict.cloned() {
                            panic!(
                                "IO authentication mismatch. Expected:\n{expected:?}\nGot:\n{auth:?}"
                            );
                        }
                    }

                    panic!(
                        "Expected \"{expected:?}\" auth to be set but there are no setAuthentication calls left to consume."
                    );
                }

                let (pattern, label) = match expect {
                    Expectation::Ask { ask, .. } => {
                        (bformat!("{{^{}$}}", preg_quote(ask, None)), ask.clone())
                    }
                    Expectation::Text { text, regex: true } => (text.clone(), text.clone()),
                    Expectation::Text { text, regex: false } => {
                        (bformat!("{{^{}$}}", preg_quote(text, None)), text.clone())
                    }
                    Expectation::Auth { .. } => unreachable!("handled above"),
                };

                while let Some(line) = lines.pop_front() {
                    if preg_is_match(&pattern, &line) {
                        continue 'expects;
                    }

                    if self.strict.cloned() {
                        panic!(
                            "IO output mismatch. Expected:\n{}\nGot:\n{}",
                            label.display_lossy(),
                            line.display_lossy()
                        );
                    }
                }

                panic!(
                    "Expected \"{}\" to be output still but there is no output left to consume. Complete output:\n{}",
                    label.display_lossy(),
                    output.display_lossy()
                );
            }
        } else if !output.is_empty() && self.strict.cloned() {
            panic!(
                "There was strictly no output expected but some output occurred: {}",
                output.display_lossy()
            );
        }
    }
}

impl IOInterface for IOMock {
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
    fn write_bytes(&self, message: &bstr::BStr, newline: bool, verbosity: i64) {
        self.inner.write_bytes(message, newline, verbosity)
    }
    fn write_error_bytes(&self, message: &bstr::BStr, newline: bool, verbosity: i64) {
        self.inner.write_error_bytes(message, newline, verbosity)
    }
    fn write_raw_bytes(&self, message: &bstr::BStr, newline: bool, verbosity: i64) {
        self.inner.write_raw_bytes(message, newline, verbosity)
    }
    fn write_error_raw_bytes(&self, message: &bstr::BStr, newline: bool, verbosity: i64) {
        self.inner
            .write_error_raw_bytes(message, newline, verbosity)
    }
    fn overwrite_bytes(
        &self,
        message: &bstr::BStr,
        newline: bool,
        size: Option<i64>,
        verbosity: i64,
    ) {
        self.inner
            .overwrite_bytes(message, newline, size, verbosity)
    }
    fn overwrite_error_bytes(
        &self,
        message: &bstr::BStr,
        newline: bool,
        size: Option<i64>,
        verbosity: i64,
    ) {
        self.inner
            .overwrite_error_bytes(message, newline, size, verbosity)
    }

    fn ask_bytes(&self, question: &bstr::BStr, default: InputValue) -> anyhow::Result<InputValue> {
        self.inner.ask_bytes(with_eol(question).as_ref(), default)
    }
    fn ask_confirmation_bytes(&self, question: &bstr::BStr, default: bool) -> anyhow::Result<bool> {
        self.inner
            .ask_confirmation_bytes(with_eol(question).as_ref(), default)
    }
    fn ask_and_validate_bytes(
        &self,
        question: &bstr::BStr,
        validator: Box<dyn Fn(InputValue) -> anyhow::Result<InputValue>>,
        attempts: Option<i64>,
        default: InputValue,
    ) -> anyhow::Result<InputValue> {
        self.inner
            .ask_and_validate_bytes(with_eol(question).as_ref(), validator, attempts, default)
    }
    fn ask_and_hide_answer_bytes(
        &self,
        question: &bstr::BStr,
    ) -> anyhow::Result<Option<bstr::BString>> {
        // Do not hide the answer in tests because that blocks on Windows with
        // hiddeninput.exe, so PHP delegates to `ask` rather than the hidden variant.
        let result = self
            .inner
            .ask_bytes(with_eol(question).as_ref(), InputValue::Null)?;
        Ok(result.as_string().map(bstr::BString::from))
    }
    fn select_bytes(
        &self,
        question: &bstr::BStr,
        choices: IndexMap<BString, BString>,
        default: InputValue,
        attempts: Option<i64>,
        error_message: &bstr::BStr,
        multiselect: bool,
    ) -> anyhow::Result<InputValue> {
        self.inner.select_bytes(
            with_eol(question).as_ref(),
            choices,
            default,
            attempts,
            error_message,
            multiselect,
        )
    }

    fn get_authentications(
        &self,
    ) -> indexmap::IndexMap<bstr::BString, indexmap::IndexMap<bstr::BString, Option<bstr::BString>>>
    {
        self.inner.get_authentications()
    }
    fn has_authentication(&self, repository_name: &bstr::BStr) -> bool {
        self.inner.has_authentication(repository_name)
    }
    fn get_authentication(
        &self,
        repository_name: &bstr::BStr,
    ) -> indexmap::IndexMap<bstr::BString, Option<bstr::BString>> {
        self.inner.get_authentication(repository_name)
    }
    fn error_bytes(&self, message: &bstr::BStr, context: &[(&bstr::BStr, &bstr::BStr)]) {
        self.inner.error_bytes(message, context)
    }
    fn warning_bytes(&self, message: &bstr::BStr, context: &[(&bstr::BStr, &bstr::BStr)]) {
        self.inner.warning_bytes(message, context)
    }
    fn debug_bytes(&self, message: &bstr::BStr, context: &[(&bstr::BStr, &bstr::BStr)]) {
        self.inner.debug_bytes(message, context)
    }

    fn set_authentication(
        &self,
        repository_name: bstr::BString,
        username: bstr::BString,
        password: Option<bstr::BString>,
    ) {
        self.auth_log.update(|auth_log| {
            auth_log.push((repository_name.clone(), username.clone(), password.clone()))
        });
        self.inner
            .set_authentication(repository_name, username, password)
    }
    fn load_configuration(&self, config: &Config) -> anyhow::Result<()> {
        self.inner.load_configuration(config)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_base_io(&self) -> Option<&dyn shirabe::io::BaseIO> {
        Some(self)
    }
}

impl shirabe::io::BaseIO for IOMock {
    fn authentications(
        &self,
    ) -> &PhpProp<
        std::rc::Rc<
            indexmap::IndexMap<
                bstr::BString,
                indexmap::IndexMap<bstr::BString, Option<bstr::BString>>,
            >,
        >,
    > {
        self.inner.authentications()
    }
}

fn with_eol(question: &bstr::BStr) -> bstr::BString {
    bformat!(
        "{}{PHP_EOL}",
        question.trim_end_with(|c| c == '\r' || c == '\n')
    )
}

pub struct IOMockGuard(std::rc::Rc<IOMock>);

impl Drop for IOMockGuard {
    fn drop(&mut self) {
        // Avoid aborting on a double panic when a test assertion is already unwinding.
        if std::thread::panicking() {
            return;
        }
        self.0.assert_complete();
    }
}

// For testing only. Mirrors TestCase::getIOMock: returns a shared IOMock handle
// plus a guard that runs assert_complete when it drops at the end of the test scope.
pub fn get_io_mock(verbosity: i64) -> anyhow::Result<(std::rc::Rc<IOMock>, IOMockGuard)> {
    let mock = std::rc::Rc::new(IOMock::new(verbosity)?);
    Ok((mock.clone(), IOMockGuard(mock)))
}
