//! ref: composer/vendor/symfony/console/Completion/CompletionSuggestions.php

use crate::symfony::console::completion::suggestion::Suggestion;
use crate::symfony::console::input::input_option::InputOption;

/// PHP union type `string|Suggestion` used by `suggestValue`/`suggestValues`.
#[derive(Debug)]
pub enum StringOrSuggestion {
    String(String),
    Suggestion(Suggestion),
}

/// Stores all completion suggestions for the current input.
#[derive(Debug)]
pub struct CompletionSuggestions {
    value_suggestions: Vec<Suggestion>,
    option_suggestions: Vec<InputOption>,
}

impl Default for CompletionSuggestions {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionSuggestions {
    pub fn new() -> Self {
        Self {
            value_suggestions: vec![],
            option_suggestions: vec![],
        }
    }

    /// Add a suggested value for an input option or argument.
    pub fn suggest_value(&mut self, value: StringOrSuggestion) -> &mut Self {
        self.value_suggestions.push(match value {
            StringOrSuggestion::Suggestion(value) => value,
            StringOrSuggestion::String(value) => Suggestion::new(value),
        });

        self
    }

    /// Add multiple suggested values at once for an input option or argument.
    pub fn suggest_values(&mut self, values: Vec<StringOrSuggestion>) -> &mut Self {
        for value in values {
            self.suggest_value(value);
        }

        self
    }

    /// Add a suggestion for an input option name.
    pub fn suggest_option(&mut self, option: InputOption) -> &mut Self {
        self.option_suggestions.push(option);

        self
    }

    /// Add multiple suggestions for input option names at once.
    pub fn suggest_options(&mut self, options: Vec<InputOption>) -> &mut Self {
        for option in options {
            self.suggest_option(option);
        }

        self
    }

    pub fn get_option_suggestions(&self) -> &Vec<InputOption> {
        &self.option_suggestions
    }

    pub fn get_value_suggestions(&self) -> &Vec<Suggestion> {
        &self.value_suggestions
    }
}
