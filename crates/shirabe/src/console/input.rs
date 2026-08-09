pub mod input_argument;
pub mod input_option;

pub use input_argument::*;
pub use input_option::*;

use shirabe_symfony_console::completion::completion_input::CompletionInput;
use shirabe_symfony_console::completion::completion_suggestions::{
    CompletionSuggestions, StringOrSuggestion,
};

/// PHP: `\Closure(CompletionInput,CompletionSuggestions):list<string|Suggestion>`.
///
/// PHP closures are bound to the command instance (`$this`); commands cannot capture a handle
/// to themselves while `configure()` runs inside `new()`, so the bound command is passed in as
/// `this` at call time instead.
pub type SuggestedValuesClosure = Box<
    dyn Fn(
        &dyn crate::command::BaseCommand,
        &CompletionInput,
        &mut CompletionSuggestions,
    ) -> anyhow::Result<Vec<String>>,
>;

/// PHP: the `list<string>|\Closure(...)` union taken by the suggestedValues parameter of the
/// `Composer\Console\Input\InputArgument` / `InputOption` backport.
///
/// The closure returns `Vec<String>` rather than `list<string|Suggestion>`: every Composer
/// closure returns plain strings, and `complete` lifts them into suggestions. PHP's runtime
/// "Closure must return an array" LogicException is statically guaranteed by the type.
pub enum SuggestedValues {
    List(Vec<String>),
    Closure(SuggestedValuesClosure),
}

impl std::fmt::Debug for SuggestedValues {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SuggestedValues::List(values) => f.debug_tuple("List").field(values).finish(),
            SuggestedValues::Closure(_) => f.write_str("Closure(..)"),
        }
    }
}

impl SuggestedValues {
    /// Whether PHP's `[] !== $suggestedValues` is false, i.e. no suggestions were declared.
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, SuggestedValues::List(values) if values.is_empty())
    }

    /// The shared body of the `InputArgument::complete` / `InputOption::complete` backport.
    pub(crate) fn complete(
        &self,
        this: &dyn crate::command::BaseCommand,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        let values = match self {
            SuggestedValues::List(values) => values.clone(),
            SuggestedValues::Closure(closure) => closure(this, input, suggestions)?,
        };
        if !values.is_empty() {
            suggestions
                .suggest_values(values.into_iter().map(StringOrSuggestion::String).collect());
        }
        Ok(())
    }

    /// PHP: `$this->suggestX()($input)` — invoking a suggestion closure directly. Calling this
    /// on a List is a programming error (PHP would fatal on `$array()`).
    pub fn call(
        &self,
        this: &dyn crate::command::BaseCommand,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<Vec<String>> {
        match self {
            SuggestedValues::Closure(closure) => closure(this, input, suggestions),
            SuggestedValues::List(_) => panic!("SuggestedValues::call on a non-closure"),
        }
    }
}

pub enum InputDefinitionItem {
    Argument(std::rc::Rc<input_argument::InputArgument>),
    Option(std::rc::Rc<input_option::InputOption>),
}

impl InputDefinitionItem {
    /// Converts to the Symfony-typed definition item accepted by `CommandData::set_definition`.
    pub(crate) fn to_definition_item(
        &self,
    ) -> shirabe_symfony_console::input::input_definition::DefinitionItem {
        use shirabe_symfony_console::input::input_definition::DefinitionItem;
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
        Self::Argument(std::rc::Rc::new(value))
    }
}

impl From<input_option::InputOption> for InputDefinitionItem {
    fn from(value: input_option::InputOption) -> Self {
        Self::Option(std::rc::Rc::new(value))
    }
}
