//! ref: composer/src/Composer/IO/BufferIO.php

use crate::io::ConsoleIO;
use anyhow::Result;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::formatter::OutputFormatterInterface;
use shirabe_external_packages::symfony::console::helper::QuestionHelper;
use shirabe_external_packages::symfony::console::input::InputInterface;
use shirabe_external_packages::symfony::console::input::StringInput;
use shirabe_external_packages::symfony::console::output::OutputInterface;
use shirabe_php_shim::{
    PHP_EOL, PhpMixed, PhpResource, RuntimeException, SEEK_SET, fopen, fseek, fwrite, rewind,
    stream_get_contents, strip_tags,
};

#[derive(Debug)]
pub struct BufferIO {
    pub(crate) inner: ConsoleIO,
}

impl BufferIO {
    pub fn new(
        input: String,
        verbosity: i64,
        formatter: Option<Box<dyn OutputFormatterInterface>>,
    ) -> Result<Self> {
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

        let _decorated = formatter.as_ref().is_some_and(|f| f.is_decorated());
        // TODO(phase-c): wire StreamOutput as the output. StreamOutput::new requires a
        // PhpResource, but `fopen` here yields a PhpMixed; PhpMixed has no resource variant,
        // so the stream cannot be passed through yet (same PhpResource/PhpMixed gap noted in
        // QuestionHelper). The constructed StreamOutput is therefore not wired as the output.
        // formatter, stream and verbosity feed the pending StreamOutput::new wiring below.
        let _ = formatter;
        let _ = stream;
        let _ = verbosity;
        let output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> =
            todo!("wire StreamOutput as the ConsoleIO output (needs PhpResource stream)");

        let inner = ConsoleIO::new(
            std::rc::Rc::new(std::cell::RefCell::new(input_obj))
                as std::rc::Rc<std::cell::RefCell<dyn InputInterface>>,
            output,
            QuestionHelper::default(),
        );

        Ok(Self { inner })
    }

    pub fn get_output(&self) -> String {
        // TODO(phase-c): OutputInterface::get_stream returns PhpResource, while
        // fseek/stream_get_contents take PhpMixed. The PhpResource stream model is not yet defined.
        let stream: PhpResource =
            todo!("retrieve the StreamOutput's PhpResource from OutputInterface::get_stream");
        fseek(&stream, 0, SEEK_SET);

        let output = stream_get_contents(&stream).unwrap_or_default();

        Preg::replace_callback(
            r"{(?<=^|\n|\x08)(.+?)(\x08+)}",
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
                let pre = strip_tags(g1);

                if pre.len() == g2.len() {
                    return String::new();
                }

                // TODO reverse parse the string, skipping span tags and \033\[([0-9;]+)m(.*?)\033\[0m style blobs
                format!("{}\n", g1.trim_end())
            },
            &output,
        )
    }

    pub fn set_user_inputs(&mut self, inputs: Vec<String>) -> Result<()> {
        // PHP: `if (!$this->input instanceof StreamableInputInterface) { throw ... }`
        //      `$this->input->setStream($this->createStream($inputs)); $this->input->setInteractive(true);`
        //
        // TODO(phase-c): unblocked by the console tree merge (StreamableInputInterface and
        // InputInterface now share one tree). Wiring the downcast still needs an as_streamable
        // accessor on InputInterface to reach ConsoleIO's input.
        let _ = inputs;
        todo!("BufferIO::set_user_inputs: needs an as_streamable accessor on InputInterface")
    }

    fn create_stream(&self, inputs: Vec<String>) -> Result<PhpResource> {
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
