//! ref: composer/vendor/symfony/console/Output/StreamOutput.php

use crate::exception::InvalidArgumentException;
use crate::formatter::OutputFormatterInterface;
use crate::output::OutputInterface;
use crate::output::VERBOSITY_NORMAL;
use crate::output::{DoWrite, Output};
use shirabe_php_shim::{
    PHP_EOL, PhpResource, fflush_resource, fwrite_resource, get_resource_type, getenv, php_regex,
    preg_is_match, sapi_windows_vt100_support, stream_isatty_resource,
};

/// StreamOutput writes the output to a given stream.
///
/// Usage:
///
/// ```php
/// $output = new StreamOutput(fopen('php://stdout', 'w'));
/// ```
///
/// As `StreamOutput` can use any stream, you can also use a file:
///
/// ```php
/// $output = new StreamOutput(fopen('/path/to/output.log', 'a', false));
/// ```
#[derive(Debug)]
pub struct StreamOutput {
    inner: Output,
    stream: PhpResource,
}

impl StreamOutput {
    /// `$verbosity` defaults to `self::VERBOSITY_NORMAL`; pass `None` to use it.
    pub fn new(
        stream: PhpResource,
        verbosity: Option<i64>,
        decorated: Option<bool>,
        formatter: Option<std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>>,
    ) -> anyhow::Result<Result<Self, InvalidArgumentException>> {
        let verbosity = verbosity.unwrap_or(VERBOSITY_NORMAL);

        if get_resource_type(&stream) != "stream" {
            return Ok(Err(InvalidArgumentException::new(
                "The StreamOutput class needs a stream as its first argument.".to_string(),
            )));
        }

        let decorated = match decorated {
            None => Some(Self::has_color_support(&stream)),
            other => other,
        };

        let inner = Output::new(Some(verbosity), decorated.unwrap_or(false), formatter);

        Ok(Ok(Self { inner, stream }))
    }

    pub(crate) fn inner(&self) -> &Output {
        &self.inner
    }

    /// Gets the stream attached to this StreamOutput instance.
    pub fn get_stream(&self) -> &PhpResource {
        &self.stream
    }

    /// For testing only. Overwrites the private `stream` field, mirroring what
    /// `Symfony\Component\Console\Tester\TesterTrait::initOutput` does via reflection on the
    /// `StreamOutput::$stream` property.
    pub fn __set_stream(&mut self, stream: PhpResource) {
        self.stream = stream;
    }

    /// Returns true if the stream supports colorization.
    ///
    /// Colorization is disabled if not supported by the stream:
    ///
    /// This is tricky on Windows, because Cygwin, Msys2 etc emulate pseudo
    /// terminals via named pipes, so we can only check the environment.
    ///
    /// Reference: Composer\XdebugHandler\Process::supportsColor
    /// https://github.com/composer/xdebug-handler
    fn has_color_support(stream: &PhpResource) -> bool {
        // Follow https://no-color.org/
        if no_color_first_char().is_some() {
            return false;
        }

        // Detect msysgit/mingw and assume this is a tty because detection
        // does not work correctly, see https://github.com/composer/composer/issues/9690
        if !stream_isatty_resource(stream)
            && ![b"MINGW32".as_slice(), b"MINGW64"].contains(
                &getenv("MSYSTEM")
                    .unwrap_or_default()
                    .as_encoded_bytes()
                    .to_ascii_uppercase()
                    .as_slice(),
            )
        {
            return false;
        }

        if cfg!(windows) && sapi_windows_vt100_support(stream) {
            return true;
        }

        if getenv("TERM_PROGRAM").as_deref() == Some(std::ffi::OsStr::new("Hyper"))
            || getenv("COLORTERM").is_some()
            || getenv("ANSICON").is_some()
            || getenv("ConEmuANSI").as_deref() == Some(std::ffi::OsStr::new("ON"))
        {
            return true;
        }

        // TODO(bytes): preg_match matches over a &str.
        let term = getenv("TERM")
            .unwrap_or_default()
            .into_string()
            .unwrap_or_else(|term| panic!("TERM is not valid UTF-8: {term:?}"));
        if "dumb" == term {
            return false;
        }

        // See https://github.com/chalk/supports-color/blob/d4f413efaf8da045c5ab440ed418ef02dbb28bf1/index.js#L157
        preg_is_match(
            php_regex!(
                "/^((screen|xterm|vt100|vt220|putty|rxvt|ansi|cygwin|linux).*)|(.*-256(color)?(-bce)?)$/"
            ),
            &term,
        )
    }
}

/// PHP: `(($_SERVER['NO_COLOR'] ?? getenv('NO_COLOR'))[0] ?? '')`.
fn no_color_first_char() -> Option<u8> {
    getenv("NO_COLOR")
        .unwrap_or_default()
        .as_encoded_bytes()
        .first()
        .copied()
}

impl DoWrite for StreamOutput {
    fn do_write(&self, message: &str, newline: bool) {
        let mut message = message.to_string();
        if newline {
            message.push_str(PHP_EOL);
        }

        fwrite_resource(&self.stream, &message);

        fflush_resource(&self.stream);
    }
}

impl OutputInterface for StreamOutput {
    fn write(&self, messages: &[String], newline: bool, options: i64) {
        self.inner.write(self, messages, newline, options);
    }
    fn writeln(&self, messages: &[String], options: i64) {
        self.inner.writeln(self, messages, options);
    }
    fn set_verbosity(&self, level: i64) {
        self.inner.set_verbosity(level);
    }
    fn get_verbosity(&self) -> i64 {
        self.inner.get_verbosity()
    }
    fn is_quiet(&self) -> bool {
        self.inner.is_quiet()
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
    fn set_decorated(&self, decorated: bool) {
        self.inner.set_decorated(decorated);
    }
    fn is_decorated(&self) -> bool {
        self.inner.is_decorated()
    }
    fn set_formatter(
        &self,
        formatter: std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>,
    ) {
        self.inner.set_formatter(formatter);
    }
    fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>> {
        self.inner.get_formatter()
    }
}
