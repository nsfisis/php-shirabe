//! ref: composer/src/Composer/Console/Input/InputOption.php

use crate::console::input::SuggestedValues;
use shirabe_symfony_console::completion::CompletionInput;
use shirabe_symfony_console::completion::CompletionSuggestions;
use shirabe_symfony_console::input::InputOption as BaseInputOption;
use shirabe_symfony_console::input::InputValue;

#[derive(Debug)]
pub struct InputOption {
    inner: BaseInputOption,
    suggested_values: SuggestedValues,
}

impl InputOption {
    pub const VALUE_NONE: i64 = 1;
    pub const VALUE_REQUIRED: i64 = 2;
    pub const VALUE_OPTIONAL: i64 = 4;
    pub const VALUE_IS_ARRAY: i64 = 8;
    pub const VALUE_NEGATABLE: i64 = 16;

    pub fn new(
        name: &str,
        shortcut: Option<&str>,
        mode: Option<i64>,
        description: &str,
        default: Option<InputValue>,
    ) -> anyhow::Result<Self> {
        Self::new6(
            name,
            shortcut,
            mode,
            description,
            default,
            SuggestedValues::List(Vec::new()),
        )
    }

    /// PHP's constructor with the sixth parameter, `$suggestedValues`.
    pub fn new6(
        name: &str,
        shortcut: Option<&str>,
        mode: Option<i64>,
        description: &str,
        default: Option<InputValue>,
        suggested_values: SuggestedValues,
    ) -> anyhow::Result<Self> {
        let default = default.unwrap_or(InputValue::Null);
        let inner = BaseInputOption::new(name, shortcut, mode, description.to_string(), default)?;
        // PHP throws LogicException here; suggested values on a valueless option cannot happen
        // at runtime unless a configure() is wrong, so this is a programming error.
        assert!(
            suggested_values.is_empty() || inner.accept_value(),
            "Cannot set suggested values if the option does not accept a value."
        );
        Ok(Self {
            inner,
            suggested_values,
        })
    }

    /// Adds suggestions to `suggestions` for the current completion input.
    ///
    /// PHP closures are bound to the command; `this` is the command dispatching the
    /// completion (see [`SuggestedValues`]).
    pub fn complete(
        &self,
        this: &dyn crate::command::BaseCommand,
        input: &CompletionInput,
        suggestions: &mut CompletionSuggestions,
    ) -> anyhow::Result<()> {
        self.suggested_values.complete(this, input, suggestions)
    }

    pub(crate) fn get_name(&self) -> String {
        self.inner.get_name().to_string()
    }

    /// Unwraps to the underlying Symfony `InputOption` (used when forwarding a Composer-typed
    /// definition to the Symfony command state).
    pub(crate) fn to_base(&self) -> BaseInputOption {
        self.inner.clone()
    }
}
