//! ref: composer/src/Composer/Console/Input/InputOption.php

use anyhow::Result;
use shirabe_external_packages::symfony::console::completion::completion_input::CompletionInput;
use shirabe_external_packages::symfony::console::completion::completion_suggestions::CompletionSuggestions;
use shirabe_external_packages::symfony::console::completion::suggestion::Suggestion;
use shirabe_external_packages::symfony::console::input::input_option::InputOption as BaseInputOption;
use shirabe_php_shim::LogicException;
use shirabe_php_shim::PhpMixed;

pub enum SuggestedValues {
    List(Vec<String>),
    Closure(Box<dyn Fn(&CompletionInput, &mut CompletionSuggestions) -> Vec<StringOrSuggestion>>),
}

impl std::fmt::Debug for SuggestedValues {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SuggestedValues::List(list) => write!(f, "SuggestedValues::List({:?})", list),
            SuggestedValues::Closure(_) => write!(f, "SuggestedValues::Closure(<closure>)"),
        }
    }
}

pub enum StringOrSuggestion {
    Str(String),
    Suggestion(Suggestion),
}

#[derive(Debug)]
pub struct InputOption {
    inner: BaseInputOption,
    suggested_values: SuggestedValues,
}

impl InputOption {
    pub fn new(
        name: &str,
        shortcut: Option<PhpMixed>,
        mode: Option<i64>,
        description: &str,
        default: Option<PhpMixed>,
        suggested_values: SuggestedValues,
    ) -> Result<Self> {
        let inner = BaseInputOption::new(name, shortcut, mode, description, default)?;
        let this = Self {
            inner,
            suggested_values,
        };

        if let SuggestedValues::List(ref list) = this.suggested_values {
            if !list.is_empty() && !this.inner.accept_value() {
                return Err(LogicException {
                    message: "Cannot set suggested values if the option does not accept a value.".to_string(),
                    code: 0,
                }.into());
            }
        } else if let SuggestedValues::Closure(_) = this.suggested_values {
            if !this.inner.accept_value() {
                return Err(LogicException {
                    message: "Cannot set suggested values if the option does not accept a value.".to_string(),
                    code: 0,
                }.into());
            }
        }

        Ok(this)
    }

    pub fn complete(
        &self,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> Result<()> {
        let values: Vec<StringOrSuggestion> = match &self.suggested_values {
            SuggestedValues::List(list) => list
                .iter()
                .map(|s| StringOrSuggestion::Str(s.clone()))
                .collect(),
            SuggestedValues::Closure(closure) => closure(input, suggestions),
        };
        if !values.is_empty() {
            suggestions.suggest_values(values);
        }
        Ok(())
    }
}
