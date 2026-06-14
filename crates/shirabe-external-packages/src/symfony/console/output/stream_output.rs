//! ref: composer/vendor/symfony/console/Output/StreamOutput.php

use crate::symfony::console::exception::InvalidArgumentException;
use crate::symfony::console::formatter::OutputFormatterInterface;
use crate::symfony::console::output::OutputInterface;
use crate::symfony::console::output::output::{DoWrite, Output};
use crate::symfony::console::output::output_interface::VERBOSITY_NORMAL;

/// StreamOutput writes the output to a given stream.
///
/// Usage:
///
///     $output = new StreamOutput(fopen('php://stdout', 'w'));
///
/// As `StreamOutput` can use any stream, you can also use a file:
///
///     $output = new StreamOutput(fopen('/path/to/output.log', 'a', false));
#[derive(Debug)]
pub struct StreamOutput {
    inner: Output,
    stream: shirabe_php_shim::PhpResource,
}

impl StreamOutput {
    /// `$verbosity` defaults to `self::VERBOSITY_NORMAL`; pass `None` to use it.
    pub fn new(
        stream: shirabe_php_shim::PhpResource,
        verbosity: Option<i64>,
        decorated: Option<bool>,
        formatter: Option<std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>>,
    ) -> anyhow::Result<Result<Self, InvalidArgumentException>> {
        let verbosity = verbosity.unwrap_or(VERBOSITY_NORMAL);

        if !shirabe_php_shim::is_resource_value(&stream)
            || "stream" != shirabe_php_shim::get_resource_type(&stream)
        {
            return Ok(Err(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: "The StreamOutput class needs a stream as its first argument."
                        .to_string(),
                    code: 0,
                },
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
    pub fn get_stream(&self) -> &shirabe_php_shim::PhpResource {
        &self.stream
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
    pub(crate) fn has_color_support(stream: &shirabe_php_shim::PhpResource) -> bool {
        // Follow https://no-color.org/
        if "" != no_color_first_char() {
            return false;
        }

        // Detect msysgit/mingw and assume this is a tty because detection
        // does not work correctly, see https://github.com/composer/composer/issues/9690
        if !shirabe_php_shim::stream_isatty_resource(stream)
            && !["MINGW32", "MINGW64"].contains(
                &shirabe_php_shim::strtoupper(
                    &shirabe_php_shim::getenv("MSYSTEM").unwrap_or_default(),
                )
                .as_str(),
            )
        {
            return false;
        }

        if "\\" == shirabe_php_shim::DIRECTORY_SEPARATOR
            && shirabe_php_shim::sapi_windows_vt100_support(stream)
        {
            return true;
        }

        if Some("Hyper".to_string()) == shirabe_php_shim::getenv("TERM_PROGRAM")
            || shirabe_php_shim::getenv("COLORTERM").is_some()
            || shirabe_php_shim::getenv("ANSICON").is_some()
            || Some("ON".to_string()) == shirabe_php_shim::getenv("ConEmuANSI")
        {
            return true;
        }

        let term = shirabe_php_shim::getenv("TERM").unwrap_or_default();
        if "dumb" == term {
            return false;
        }

        // See https://github.com/chalk/supports-color/blob/d4f413efaf8da045c5ab440ed418ef02dbb28bf1/index.js#L157
        let mut matches: Vec<Option<String>> = Vec::new();
        shirabe_php_shim::preg_match(
            "/^((screen|xterm|vt100|vt220|putty|rxvt|ansi|cygwin|linux).*)|(.*-256(color)?(-bce)?)$/",
            &term,
            &mut matches,
        )
    }
}

/// PHP: `(($_SERVER['NO_COLOR'] ?? getenv('NO_COLOR'))[0] ?? '')`.
fn no_color_first_char() -> String {
    let value = shirabe_php_shim::getenv("NO_COLOR").unwrap_or_default();
    value
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default()
}

impl DoWrite for StreamOutput {
    fn do_write(&self, message: &str, newline: bool) {
        let mut message = message.to_string();
        if newline {
            message.push_str(shirabe_php_shim::PHP_EOL);
        }

        shirabe_php_shim::fwrite_resource(&self.stream, &message);

        shirabe_php_shim::fflush_resource(&self.stream);
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
