pub trait OutputInterface {
    fn write(&mut self, messages: &str, newline: bool, r#type: i64);
    fn writeln(&mut self, messages: &str, r#type: i64);
    fn set_verbosity(&mut self, level: i64);
    fn get_verbosity(&self) -> i64;
    fn is_quiet(&self) -> bool;
    fn is_verbose(&self) -> bool;
    fn is_very_verbose(&self) -> bool;
    fn is_debug(&self) -> bool;
    fn set_decorated(&mut self, decorated: bool);
    fn is_decorated(&self) -> bool;
}

pub const VERBOSITY_QUIET: i64 = 16;
pub const VERBOSITY_NORMAL: i64 = 32;
pub const VERBOSITY_VERBOSE: i64 = 64;
pub const VERBOSITY_VERY_VERBOSE: i64 = 128;
pub const VERBOSITY_DEBUG: i64 = 256;

pub const OUTPUT_NORMAL: i64 = 1;
pub const OUTPUT_RAW: i64 = 2;
pub const OUTPUT_PLAIN: i64 = 4;
