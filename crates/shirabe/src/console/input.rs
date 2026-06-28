pub mod input_argument;
pub mod input_option;

pub use input_argument::*;
pub use input_option::*;

pub enum InputDefinitionItem {
    Argument(input_argument::InputArgument),
    Option(input_option::InputOption),
}

impl InputDefinitionItem {
    /// Converts to the Symfony-typed definition item accepted by `CommandData::set_definition`.
    pub(crate) fn to_definition_item(
        &self,
    ) -> shirabe_external_packages::symfony::console::input::input_definition::DefinitionItem {
        use shirabe_external_packages::symfony::console::input::input_definition::DefinitionItem;
        match self {
            InputDefinitionItem::Argument(argument) => {
                DefinitionItem::InputArgument(argument.to_base())
            }
            InputDefinitionItem::Option(option) => DefinitionItem::InputOption(option.to_base()),
        }
    }
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
