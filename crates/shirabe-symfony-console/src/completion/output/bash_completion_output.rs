//! ref: composer/vendor/symfony/console/Completion/Output/BashCompletionOutput.php sha1:9f252051691f23b42011cfd4bd2d6d91c3b14d18

use crate::completion::{CompletionOutputInterface, CompletionSuggestions};
use crate::output::OutputInterface;
use bstr::BString;
use shirabe_php_shim::{bconcat, implode};

#[derive(Debug)]
pub struct BashCompletionOutput;

impl CompletionOutputInterface for BashCompletionOutput {
    fn write(&self, suggestions: &CompletionSuggestions, output: &dyn OutputInterface) {
        let mut values: Vec<BString> = suggestions
            .get_value_suggestions()
            .iter()
            .map(|suggestion| suggestion.get_value())
            .collect();
        for option in suggestions.get_option_suggestions() {
            values.push(bconcat!("--", option.get_name()));
            if option.is_negatable() {
                values.push(bconcat!("--no-", option.get_name()));
            }
        }
        output.writeln(&[implode("\n", &values)], 0);
    }
}
