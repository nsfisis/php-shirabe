//! ref: composer/src/Composer/IO/BufferIO.php

use crate::io::ConsoleIO;
use anyhow::Result;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::component::console::helper::HelperSet;
use shirabe_external_packages::symfony::component::console::input::InputInterface;
use shirabe_external_packages::symfony::component::console::input::StringInput;
use shirabe_external_packages::symfony::component::console::output::OutputInterface;
use shirabe_external_packages::symfony::console::formatter::OutputFormatterInterface;
use shirabe_external_packages::symfony::console::helper::QuestionHelper;
use shirabe_external_packages::symfony::console::output::StreamOutput;
use shirabe_php_shim::{
    PHP_EOL, PhpMixed, RuntimeException, fopen, fseek, fwrite, rewind, stream_get_contents,
    strip_tags,
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
        let mut input_obj = StringInput::new(&input);
        input_obj.set_interactive(false);

        let stream = fopen("php://memory", "rw");
        if matches!(stream, PhpMixed::Bool(false)) {
            return Err(RuntimeException {
                message: "Unable to open memory output stream".to_string(),
                code: 0,
            }
            .into());
        }

        let decorated = formatter.as_ref().map_or(false, |f| f.is_decorated());
        // TODO(phase-b): StreamOutput lives under `symfony::console` (not `symfony::component::console`)
        // and so does not implement the `component::console::output::OutputInterface` ConsoleIO expects.
        // Real fix requires unifying the two crate paths.
        let _ = formatter;
        let _ = StreamOutput::new(stream, verbosity, Some(decorated));
        let output: Box<dyn OutputInterface> = todo!("StreamOutput as Box<dyn OutputInterface>");

        // TODO(phase-b): symfony console helper modules live under both `symfony::console`
        // and `symfony::component::console`; QuestionHelper::new is not yet provided.
        let helpers: Vec<PhpMixed> = vec![/* PhpMixed::Object(QuestionHelper::new()) */];
        let _ = std::marker::PhantomData::<QuestionHelper>;
        let inner = ConsoleIO::new(
            Box::new(input_obj) as Box<dyn InputInterface>,
            output,
            HelperSet::new(helpers),
        );

        Ok(Self { inner })
    }

    pub fn get_output(&self) -> String {
        // TODO(phase-b): OutputInterface::get_stream returns PhpResource, while
        // fseek/stream_get_contents take PhpMixed. Conversion is not yet defined.
        let stream: PhpMixed =
            todo!("PhpResource -> PhpMixed conversion for OutputInterface::get_stream");
        fseek(stream.clone(), 0);

        let output = stream_get_contents(stream).unwrap_or_default();

        let output = Preg::replace_callback(
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
        );

        // TODO(phase-b): Preg::replace_callback returns Result<String>, unwrap for now
        output.unwrap_or_default()
    }

    pub fn set_user_inputs(&mut self, inputs: Vec<String>) -> Result<()> {
        // TODO(phase-b): downcast Box<dyn InputInterface> to StreamableInputInterface.
        // as_any/set_stream are not yet exposed on the InputInterface trait object.
        let _ = inputs;
        let _ = |i: &Box<dyn InputInterface>| -> bool {
            let _ = i;
            false
        };
        todo!("port BufferIO::set_user_inputs once StreamableInputInterface downcast is available")
    }

    fn create_stream(&self, inputs: Vec<String>) -> Result<PhpMixed> {
        let stream = fopen("php://memory", "r+");
        if matches!(stream, PhpMixed::Bool(false)) {
            return Err(RuntimeException {
                message: "Unable to open memory output stream".to_string(),
                code: 0,
            }
            .into());
        }

        for input in inputs {
            fwrite(stream.clone(), &format!("{}{}", input, PHP_EOL), -1);
        }

        rewind(stream.clone());

        Ok(stream)
    }
}

// TODO(phase-b): PHP `class BufferIO extends ConsoleIO` — delegate all IOInterface,
// LoggerInterface, and BaseIO methods to `self.inner` (ConsoleIO).
impl shirabe_external_packages::psr::log::LoggerInterface for BufferIO {
    fn emergency(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.emergency(message, context)
    }
    fn alert(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.alert(message, context)
    }
    fn critical(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.critical(message, context)
    }
    fn error(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.error(message, context)
    }
    fn warning(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.warning(message, context)
    }
    fn notice(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.notice(message, context)
    }
    fn info(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.info(message, context)
    }
    fn debug(&self, message: &str, context: &[(&str, &str)]) {
        self.inner.debug(message, context)
    }
    fn log(&self, level: &str, message: &str, context: &[(&str, &str)]) {
        self.inner.log(level, message, context)
    }
}

impl crate::io::IOInterface for BufferIO {
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
        validator: Box<dyn Fn(PhpMixed) -> PhpMixed>,
        attempts: Option<i64>,
        default: PhpMixed,
    ) -> PhpMixed {
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
