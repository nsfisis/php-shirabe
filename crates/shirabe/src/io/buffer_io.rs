//! ref: composer/src/Composer/IO/BufferIO.php

use crate::io::ConsoleIO;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::formatter::OutputFormatterInterface;
use shirabe_external_packages::symfony::console::helper::QuestionHelper;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::input::StringInput;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_external_packages::symfony::console::output::StreamOutput;
use shirabe_php_shim::{
    PHP_EOL, PhpMixed, PhpResource, RuntimeException, SEEK_SET, fopen, fseek, fwrite, php_regex,
    rewind, stream_get_contents, strip_tags,
};

#[derive(Debug)]
pub struct BufferIO {
    pub(crate) inner: ConsoleIO,
}

impl BufferIO {
    pub fn new(
        input: String,
        verbosity: i64,
        formatter: Option<std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>>,
    ) -> anyhow::Result<Self> {
        let mut input_obj = StringInput::new(&input)?;
        input_obj.set_interactive(false);

        let stream = match fopen("php://memory", "rw") {
            Ok(stream) => stream,
            Err(_) => {
                return Err(RuntimeException {
                    message: "Unable to open memory output stream".to_string(),
                    code: 0,
                }
                .into());
            }
        };

        let decorated = formatter
            .as_ref()
            .is_some_and(|f| f.borrow().is_decorated());
        let output = StreamOutput::new(stream, Some(verbosity), Some(decorated), formatter)??;
        let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> =
            std::rc::Rc::new(std::cell::RefCell::new(output));

        let inner = ConsoleIO::new(
            std::rc::Rc::new(std::cell::RefCell::new(input_obj))
                as std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
            output,
            QuestionHelper::default(),
        );

        Ok(Self { inner })
    }

    pub fn get_output(&self) -> String {
        let output = self.inner.output.borrow();
        let stream_output = (*output)
            .as_any()
            .downcast_ref::<StreamOutput>()
            .expect("BufferIO output is always a StreamOutput");
        let stream = stream_output.get_stream();
        fseek(stream, 0, SEEK_SET);

        let output = stream_get_contents(stream).unwrap_or_default();

        // Regex pattern compatibility:
        // PHP uses `{(?<=^|\n|\x08)(.+?)(\x08+)}` to collapse backspace-overwritten spans (e.g.
        // progress bars). The `regex` crate has no look-behind, so the `(?<=^|\n|\x08)` anchor is
        // turned into a consuming optional leading group `(^|\n|\x08)` that is re-emitted in the
        // replacement. Because PCRE's look-behind is zero-width, a `\x08` ending one match can also
        // anchor the following match; consuming-and-restoring would break that chaining in a single
        // pass, so the replacement is applied to a fixpoint (each pass strictly shrinks the string).
        let mut output = output;
        loop {
            let next = Preg::replace_callback(
                php_regex!(r"{(^|\n|\x08)(.+?)(\x08+)}"),
                |matches: &indexmap::IndexMap<
                    shirabe_external_packages::composer::pcre::CaptureKey,
                    String,
                >|
                 -> String {
                    let empty = String::new();
                    let g1 = matches
                        .get(&shirabe_external_packages::composer::pcre::CaptureKey::ByIndex(1))
                        .unwrap_or(&empty);
                    let g2 = matches
                        .get(&shirabe_external_packages::composer::pcre::CaptureKey::ByIndex(2))
                        .unwrap_or(&empty);
                    let g3 = matches
                        .get(&shirabe_external_packages::composer::pcre::CaptureKey::ByIndex(3))
                        .unwrap_or(&empty);
                    let pre = strip_tags(g2);

                    if pre.len() == g3.len() {
                        return g1.clone();
                    }

                    // TODO reverse parse the string, skipping span tags and \033\[([0-9;]+)m(.*?)\033\[0m style blobs
                    format!("{}{}\n", g1, g2.trim_end())
                },
                &output,
            );
            if next == output {
                break;
            }
            output = next;
        }
        output
    }

    pub fn set_user_inputs(&mut self, inputs: Vec<String>) -> anyhow::Result<()> {
        let stream = self.create_stream(inputs)?;

        let mut input = self.inner.input.borrow_mut();
        let Some(streamable) = input.as_streamable_mut() else {
            return Err(RuntimeException {
                message: "Setting the user inputs requires at least the version 3.2 of the symfony/console component.".to_string(),
                code: 0,
            }
            .into());
        };

        streamable.set_stream(stream);
        streamable.set_interactive(true);

        Ok(())
    }

    fn create_stream(&self, inputs: Vec<String>) -> anyhow::Result<PhpResource> {
        let stream = match fopen("php://memory", "r+") {
            Ok(stream) => stream,
            Err(_) => {
                return Err(RuntimeException {
                    message: "Unable to open memory output stream".to_string(),
                    code: 0,
                }
                .into());
            }
        };

        for input in inputs {
            fwrite(&stream, &format!("{}{}", input, PHP_EOL), None);
        }

        rewind(&stream);

        Ok(stream)
    }
}

impl crate::io::IOInterfaceImmutable for BufferIO {
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
        self.inner.ask(question, default)
    }
    fn ask_confirmation(&self, question: String, default: bool) -> bool {
        self.inner.ask_confirmation(question, default)
    }
    fn ask_and_validate(
        &self,
        question: String,
        validator: Box<dyn Fn(PhpMixed) -> anyhow::Result<PhpMixed>>,
        attempts: Option<i64>,
        default: PhpMixed,
    ) -> anyhow::Result<PhpMixed> {
        self.inner
            .ask_and_validate(question, validator, attempts, default)
    }
    fn ask_and_hide_answer(&self, question: String) -> Option<String> {
        self.inner.ask_and_hide_answer(question)
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
            question,
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

impl crate::io::IOInterfaceMutable for BufferIO {
    fn set_authentication(
        &mut self,
        repository_name: String,
        username: String,
        password: Option<String>,
    ) {
        self.inner
            .set_authentication(repository_name, username, password)
    }
    fn load_configuration(&mut self, config: &mut crate::config::Config) -> anyhow::Result<()> {
        self.inner.load_configuration(config)
    }
}

impl crate::io::IOInterface for BufferIO {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_base_io_mut(&mut self) -> Option<&mut dyn crate::io::BaseIO> {
        Some(self)
    }

    fn enable_debugging(&mut self, start_time: f64) {
        self.inner.enable_debugging(start_time)
    }
}

impl crate::io::BaseIO for BufferIO {
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
