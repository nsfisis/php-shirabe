//! ref: composer/src/Composer/IO/BufferIO.php sha1:92cbd3ce7909bd23e07b2d280a5f49ad5ea099bb

use crate::io::ConsoleIO;
use bstr::BString;
use indexmap::IndexMap;
use shirabe_php_shim::{
    PHP_EOL, PhpResource, PregMatches, RuntimeException, SEEK_SET, bconcat, fopen, fseek, fwrite,
    php_regex, preg_replace_callback, rewind, rtrim, stream_get_contents, strip_tags,
};
use shirabe_symfony_console::formatter::OutputFormatterInterface;
use shirabe_symfony_console::helper::QuestionHelper;
use shirabe_symfony_console::input::{InputInterface, InputValue, StringInput};
use shirabe_symfony_console::output::{OutputInterface, StreamOutput};

#[derive(Debug)]
pub struct BufferIO {
    pub(crate) inner: ConsoleIO,
}

impl BufferIO {
    pub fn new(
        input: impl AsRef<[u8]>,
        verbosity: i64,
        formatter: Option<std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>>,
    ) -> anyhow::Result<Self> {
        let mut input_obj = StringInput::new(input)?;
        input_obj.set_interactive(false);

        let stream = match fopen("php://memory", "rw") {
            Ok(stream) => stream,
            Err(_) => {
                return Err(RuntimeException::new(BString::from(
                    "Unable to open memory output stream",
                ))
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

    pub fn get_output(&self) -> BString {
        let output = self.inner.output.borrow();
        let stream_output = (*output)
            .as_any()
            .downcast_ref::<StreamOutput>()
            .expect("BufferIO output is always a StreamOutput");
        let stream = stream_output.get_stream();
        fseek(stream, 0, SEEK_SET);

        let output = BString::from(stream_get_contents(stream).unwrap_or_default());

        // Regex pattern compatibility:
        // PHP uses `{(?<=^|\n|\x08)(.+?)(\x08+)}` to collapse backspace-overwritten spans (e.g.
        // progress bars). The `regex` crate has no look-behind, so the `(?<=^|\n|\x08)` anchor is
        // turned into a consuming optional leading group `(^|\n|\x08)` that is re-emitted in the
        // replacement. Because PCRE's look-behind is zero-width, a `\x08` ending one match can also
        // anchor the following match; consuming-and-restoring would break that chaining in a single
        // pass, so the replacement is applied to a fixpoint (each pass strictly shrinks the string).
        let mut output = output;
        loop {
            let next = preg_replace_callback(
                php_regex!(r"{(^|\n|\x08)(.+?)(\x08+)}"),
                |matches: &PregMatches| -> anyhow::Result<BString> {
                    let g1 = matches.get(1).unwrap_or_default();
                    let g2 = matches.get(2).unwrap_or_default();
                    let g3 = matches.get(3).unwrap_or_default();
                    let pre = strip_tags(g2);

                    if pre.len() == g3.len() {
                        return Ok(BString::from(g1));
                    }

                    // TODO reverse parse the string, skipping span tags and \033\[([0-9;]+)m(.*?)\033\[0m style blobs
                    Ok(bconcat!(g1, rtrim(g2), "\n"))
                },
                &output,
            )
            .expect("the replacement callback cannot fail");
            if next == output {
                break;
            }
            output = next;
        }
        output
    }

    pub fn set_user_inputs(&self, inputs: Vec<impl AsRef<[u8]>>) -> anyhow::Result<()> {
        let stream = self.create_stream(inputs)?;

        let mut input = self.inner.input.borrow_mut();
        let Some(streamable) = input.as_streamable_mut() else {
            return Err(RuntimeException::new(BString::from("Setting the user inputs requires at least the version 3.2 of the symfony/console component."))
            .into());
        };

        streamable.set_stream(stream);
        streamable.set_interactive(true);

        Ok(())
    }

    fn create_stream(&self, inputs: Vec<impl AsRef<[u8]>>) -> anyhow::Result<PhpResource> {
        let stream = match fopen("php://memory", "r+") {
            Ok(stream) => stream,
            Err(_) => {
                return Err(RuntimeException::new(BString::from(
                    "Unable to open memory output stream",
                ))
                .into());
            }
        };

        for input in inputs {
            fwrite(&stream, bconcat!(input.as_ref(), PHP_EOL), None);
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
        self.inner.ask_bytes(question, default)
    }
    fn ask_confirmation_bytes(&self, question: &bstr::BStr, default: bool) -> anyhow::Result<bool> {
        self.inner.ask_confirmation_bytes(question, default)
    }
    fn ask_and_validate_bytes(
        &self,
        question: &bstr::BStr,
        validator: Box<dyn Fn(InputValue) -> anyhow::Result<InputValue>>,
        attempts: Option<i64>,
        default: InputValue,
    ) -> anyhow::Result<InputValue> {
        self.inner
            .ask_and_validate_bytes(question, validator, attempts, default)
    }
    fn ask_and_hide_answer_bytes(
        &self,
        question: &bstr::BStr,
    ) -> anyhow::Result<Option<bstr::BString>> {
        self.inner.ask_and_hide_answer_bytes(question)
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
}

impl crate::io::IOInterfaceMutable for BufferIO {
    fn set_authentication(
        &mut self,
        repository_name: bstr::BString,
        username: bstr::BString,
        password: Option<bstr::BString>,
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
    ) -> &indexmap::IndexMap<bstr::BString, indexmap::IndexMap<bstr::BString, Option<bstr::BString>>>
    {
        self.inner.authentications()
    }
    fn authentications_mut(
        &mut self,
    ) -> &mut indexmap::IndexMap<
        bstr::BString,
        indexmap::IndexMap<bstr::BString, Option<bstr::BString>>,
    > {
        self.inner.authentications_mut()
    }
}
