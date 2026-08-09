//! ref: composer/src/Composer/Console/Input/InputArgument.php

use crate::console::input::SuggestedValues;
use shirabe_php_shim::PhpMixed;
use shirabe_symfony_console::completion::CompletionInput;
use shirabe_symfony_console::completion::CompletionSuggestions;
use shirabe_symfony_console::input::InputArgument as BaseInputArgument;

#[derive(Debug)]
pub struct InputArgument {
    inner: BaseInputArgument,
    suggested_values: SuggestedValues,
}

impl InputArgument {
    pub const REQUIRED: i64 = 1;
    pub const OPTIONAL: i64 = 2;
    pub const IS_ARRAY: i64 = 4;

    pub fn new(
        name: &str,
        mode: Option<i64>,
        description: &str,
        default: Option<PhpMixed>,
    ) -> anyhow::Result<Self> {
        Self::new5(
            name,
            mode,
            description,
            default,
            SuggestedValues::List(Vec::new()),
        )
    }

    /// PHP's constructor with the fifth parameter, `$suggestedValues`.
    pub fn new5(
        name: &str,
        mode: Option<i64>,
        description: &str,
        default: Option<PhpMixed>,
        suggested_values: SuggestedValues,
    ) -> anyhow::Result<Self> {
        let inner = BaseInputArgument::new(
            name.to_string(),
            mode,
            description.to_string(),
            default.unwrap_or(PhpMixed::Null),
        )?;
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

    /// Unwraps to the underlying Symfony `InputArgument` (used when forwarding a Composer-typed
    /// definition to the Symfony command state).
    pub(crate) fn to_base(&self) -> BaseInputArgument {
        self.inner.clone()
    }
}
