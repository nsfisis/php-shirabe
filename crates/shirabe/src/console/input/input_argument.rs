//! ref: composer/src/Composer/Console/Input/InputArgument.php

use anyhow::Result;
use shirabe_external_packages::symfony::console::completion::completion_input::CompletionInput;
use shirabe_external_packages::symfony::console::completion::completion_suggestions::CompletionSuggestions;
use shirabe_external_packages::symfony::console::completion::suggestion::Suggestion;
use shirabe_external_packages::symfony::console::input::input_argument::InputArgument as BaseInputArgument;
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
pub struct InputArgument {
    inner: BaseInputArgument,
    suggested_values: SuggestedValues,
}

impl InputArgument {
    pub fn new(
        name: &str,
        mode: Option<i64>,
        description: &str,
        default: Option<PhpMixed>,
        suggested_values: SuggestedValues,
    ) -> Result<Self> {
        let inner = BaseInputArgument::new(name, mode, description, default)?;
        Ok(Self {
            inner,
            suggested_values,
        })
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
