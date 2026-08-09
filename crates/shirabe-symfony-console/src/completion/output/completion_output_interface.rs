//! ref: composer/vendor/symfony/console/Completion/Output/CompletionOutputInterface.php

use crate::completion::CompletionSuggestions;
use crate::output::OutputInterface;

/// Transforms the `CompletionSuggestions` object into output readable by the shell completion.
pub trait CompletionOutputInterface: std::fmt::Debug {
    fn write(&self, suggestions: &CompletionSuggestions, output: &dyn OutputInterface);
}
