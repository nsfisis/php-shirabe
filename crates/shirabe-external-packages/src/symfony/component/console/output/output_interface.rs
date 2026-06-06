use crate::symfony::component::console::formatter::OutputFormatter;
use crate::symfony::component::console::output::ConsoleOutputInterface;

pub trait OutputInterface: std::fmt::Debug {
    // PHP class semantics: OutputInterface methods take &self with interior mutability,
    // because output objects are shared by reference across the PHP code.
    fn write(&self, messages: &str, newline: bool, r#type: i64);
    fn writeln(&self, messages: &str, r#type: i64);
    fn set_verbosity(&self, level: i64);
    fn get_verbosity(&self) -> i64;
    fn is_quiet(&self) -> bool;
    fn is_verbose(&self) -> bool;
    fn is_very_verbose(&self) -> bool;
    fn is_debug(&self) -> bool;
    fn set_decorated(&self, decorated: bool);
    fn is_decorated(&self) -> bool;
    fn set_formatter(&self, formatter: OutputFormatter);
    fn get_formatter(&self) -> &OutputFormatter;

    /// PHP: `$output instanceof ConsoleOutputInterface`. Default false; ConsoleOutput overrides.
    fn is_console_output_interface(&self) -> bool {
        false
    }

    /// PHP: `$output instanceof ConsoleOutputInterface`. Returns the output as a
    /// ConsoleOutputInterface trait object when it is one. Default None; ConsoleOutput overrides.
    fn as_console_output_interface(&self) -> Option<&dyn ConsoleOutputInterface> {
        None
    }

    /// PHP: only StreamOutput exposes `getStream()`. Default panics for outputs without one.
    fn get_stream(&self) -> shirabe_php_shim::PhpResource {
        todo!("get_stream not available on this OutputInterface implementation")
    }
}

pub const VERBOSITY_QUIET: i64 = 16;
pub const VERBOSITY_NORMAL: i64 = 32;
pub const VERBOSITY_VERBOSE: i64 = 64;
pub const VERBOSITY_VERY_VERBOSE: i64 = 128;
pub const VERBOSITY_DEBUG: i64 = 256;

pub const OUTPUT_NORMAL: i64 = 1;
pub const OUTPUT_RAW: i64 = 2;
pub const OUTPUT_PLAIN: i64 = 4;
