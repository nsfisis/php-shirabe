use crate::symfony::console::formatter::OutputFormatterInterface;

pub const VERBOSITY_QUIET: i64 = 16;
pub const VERBOSITY_NORMAL: i64 = 32;
pub const VERBOSITY_VERBOSE: i64 = 64;
pub const VERBOSITY_VERY_VERBOSE: i64 = 128;
pub const VERBOSITY_DEBUG: i64 = 256;

pub const OUTPUT_NORMAL: i64 = 1;
pub const OUTPUT_RAW: i64 = 2;
pub const OUTPUT_PLAIN: i64 = 4;

/// OutputInterface is the interface implemented by all Output classes.
pub trait OutputInterface: std::fmt::Debug + shirabe_php_shim::AsAny {
    /// Writes a message to the output.
    ///
    /// `$messages` is a single string or an iterable of strings.
    fn write(&self, messages: &[String], newline: bool, options: i64);

    /// Writes a message to the output and adds a newline at the end.
    fn writeln(&self, messages: &[String], options: i64);

    /// Sets the verbosity of the output.
    fn set_verbosity(&self, level: i64);

    /// Gets the current verbosity of the output.
    fn get_verbosity(&self) -> i64;

    /// Returns whether verbosity is quiet (-q).
    fn is_quiet(&self) -> bool;

    /// Returns whether verbosity is verbose (-v).
    fn is_verbose(&self) -> bool;

    /// Returns whether verbosity is very verbose (-vv).
    fn is_very_verbose(&self) -> bool;

    /// Returns whether verbosity is debug (-vvv).
    fn is_debug(&self) -> bool;

    /// Sets the decorated flag.
    fn set_decorated(&self, decorated: bool);

    /// Gets the decorated flag.
    fn is_decorated(&self) -> bool;

    fn set_formatter(
        &self,
        formatter: std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>,
    );

    /// Returns current output formatter instance.
    fn get_formatter(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputFormatterInterface>>;
}
