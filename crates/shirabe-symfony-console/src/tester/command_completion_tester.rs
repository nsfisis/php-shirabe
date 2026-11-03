//! ref: composer/vendor/symfony/console/Tester/CommandCompletionTester.php

use crate::command::Command;
use crate::completion::{CompletionInput, CompletionSuggestions};
use bstr::BString;
use shirabe_php_shim::bconcat;

/// Eases the testing of command completion.
#[derive(Debug)]
pub struct CommandCompletionTester {
    command: std::rc::Rc<std::cell::RefCell<dyn Command>>,
}

impl CommandCompletionTester {
    pub fn new(command: std::rc::Rc<std::cell::RefCell<dyn Command>>) -> Self {
        Self { command }
    }

    /// Create completion suggestions from input tokens.
    pub fn complete(&self, input: &[impl AsRef<[u8]>]) -> anyhow::Result<Vec<BString>> {
        let mut input: Vec<BString> = input.iter().map(|s| BString::from(s.as_ref())).collect();
        let current_index = input.len() as i64;
        if input.last().is_some_and(|last| last.is_empty()) {
            input.pop();
        }
        // array_unshift($input, $this->command->getName())
        input.insert(0, self.command.borrow().get_name().unwrap_or_default());

        let mut completion_input = CompletionInput::from_tokens(input, current_index)?;
        {
            let command_ref = self.command.borrow();
            let definition = command_ref.get_definition();
            completion_input.bind(&definition)?;
        }
        let mut suggestions = CompletionSuggestions::new();

        self.command
            .borrow()
            .complete(&completion_input, &mut suggestions)?;

        let mut result: Vec<BString> = suggestions
            .get_option_suggestions()
            .iter()
            .map(|option| bconcat!("--", option.get_name()))
            .collect();
        // array_map('strval', ... $suggestions->getValueSuggestions())
        result.extend(
            suggestions
                .get_value_suggestions()
                .iter()
                .map(|suggestion| suggestion.get_value()),
        );
        Ok(result)
    }
}
