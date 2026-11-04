//! ref: composer/vendor/symfony/console/Completion/Output/CompletionOutputInterface.php sha1:0a8c32cd8c0e000827f8b93f445c41d9f15c1f8e

use crate::completion::CompletionSuggestions;
use crate::output::OutputInterface;

/// Transforms the `CompletionSuggestions` object into output readable by the shell completion.
pub trait CompletionOutputInterface: std::fmt::Debug {
    fn write(&self, suggestions: &CompletionSuggestions, output: &dyn OutputInterface);
}
