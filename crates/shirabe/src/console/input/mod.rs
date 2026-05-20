pub mod input_argument;
pub mod input_option;

pub use input_argument::*;
pub use input_option::*;

pub enum InputDefinitionItem {
    Argument(input_argument::InputArgument),
    Option(input_option::InputOption),
}

impl From<input_argument::InputArgument> for InputDefinitionItem {
    fn from(value: input_argument::InputArgument) -> Self {
        Self::Argument(value)
    }
}

impl From<input_option::InputOption> for InputDefinitionItem {
    fn from(value: input_option::InputOption) -> Self {
        Self::Option(value)
    }
}
