//! ref: composer/src/Composer/IO/BufferIO.php

use crate::io::console_io::ConsoleIO;
use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::console::formatter::output_formatter_interface::OutputFormatterInterface;
use shirabe_external_packages::symfony::console::helper::helper_set::HelperSet;
use shirabe_external_packages::symfony::console::helper::question_helper::QuestionHelper;
use shirabe_external_packages::symfony::console::input::streamable_input_interface::StreamableInputInterface;
use shirabe_external_packages::symfony::console::input::string_input::StringInput;
use shirabe_external_packages::symfony::console::output::stream_output::StreamOutput;
use shirabe_php_shim::{
    PHP_EOL, PhpMixed, RuntimeException, fopen, fseek, fwrite, rewind, stream_get_contents,
    strip_tags,
};

#[derive(Debug)]
pub struct BufferIO {
    inner: ConsoleIO,
}

impl BufferIO {
    pub fn new(
        input: String,
        verbosity: i64,
        formatter: Option<Box<dyn OutputFormatterInterface>>,
    ) -> Result<Self> {
        let mut input_obj = StringInput::new(input);
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
        let output = StreamOutput::new(stream, verbosity, decorated, formatter);

        let inner = ConsoleIO::new(
            input_obj,
            output,
            HelperSet::new(vec![Box::new(QuestionHelper::new())]),
        );

        Ok(Self { inner })
    }

    pub fn get_output(&self) -> String {
        fseek(self.inner.output.get_stream(), 0);

        let output = stream_get_contents(self.inner.output.get_stream()).unwrap_or_default();

        let output = Preg::replace_callback(
            r"{(?<=^|\n|\x08)(.+?)(\x08+)}",
            |matches: &[String]| -> String {
                let pre = strip_tags(&matches[1]);

                if pre.len() == matches[2].len() {
                    return String::new();
                }

                // TODO reverse parse the string, skipping span tags and \033\[([0-9;]+)m(.*?)\033\[0m style blobs
                format!("{}\n", matches[1].trim_end())
            },
            &output,
        );

        output
    }

    pub fn set_user_inputs(&mut self, inputs: Vec<String>) -> Result<()> {
        if self
            .inner
            .input
            .as_any()
            .downcast_ref::<dyn StreamableInputInterface>()
            .is_none()
        {
            return Err(RuntimeException {
                message: "Setting the user inputs requires at least the version 3.2 of the symfony/console component.".to_string(),
                code: 0,
            }
            .into());
        }

        self.inner.input.set_stream(self.create_stream(inputs)?);
        self.inner.input.set_interactive(true);

        Ok(())
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
