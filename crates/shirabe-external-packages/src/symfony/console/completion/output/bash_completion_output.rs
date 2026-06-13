//! ref: composer/vendor/symfony/console/Completion/Output/BashCompletionOutput.php

use crate::symfony::console::completion::completion_suggestions::CompletionSuggestions;
use crate::symfony::console::completion::output::completion_output_interface::CompletionOutputInterface;
use crate::symfony::console::output::output_interface::OutputInterface;

#[derive(Debug)]
pub struct BashCompletionOutput;

impl CompletionOutputInterface for BashCompletionOutput {
    fn write(&self, suggestions: &CompletionSuggestions, output: &dyn OutputInterface) {
        let mut values: Vec<String> = suggestions
            .get_value_suggestions()
            .iter()
            .map(|suggestion| suggestion.get_value())
            .collect();
        for option in suggestions.get_option_suggestions() {
            values.push(format!("--{}", option.get_name()));
            if option.is_negatable() {
                values.push(format!("--no-{}", option.get_name()));
            }
        }
        output.writeln(&[values.join("\n")], 0);
    }
}
