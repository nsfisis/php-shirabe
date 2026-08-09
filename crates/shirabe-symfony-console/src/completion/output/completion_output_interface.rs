//! ref: composer/vendor/symfony/console/Completion/Output/CompletionOutputInterface.php

use crate::completion::completion_suggestions::CompletionSuggestions;
use crate::output::output_interface::OutputInterface;

/// Transforms the `CompletionSuggestions` object into output readable by the shell completion.
pub trait CompletionOutputInterface: std::fmt::Debug {
    fn write(&self, suggestions: &CompletionSuggestions, output: &dyn OutputInterface);
}
