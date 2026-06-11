use shirabe_php_shim::PhpMixed;

/// Output style helpers.
pub trait StyleInterface {
    /// Formats a command title.
    fn title(&mut self, message: &str);

    /// Formats a section title.
    fn section(&mut self, message: &str);

    /// Formats a list.
    fn listing(&mut self, elements: Vec<PhpMixed>);

    /// Formats informational text.
    fn text(&mut self, message: PhpMixed);

    /// Formats a success result bar.
    fn success(&mut self, message: PhpMixed);

    /// Formats an error result bar.
    fn error(&mut self, message: PhpMixed);

    /// Formats an warning result bar.
    fn warning(&mut self, message: PhpMixed);

    /// Formats a note admonition.
    fn note(&mut self, message: PhpMixed);

    /// Formats a caution admonition.
    fn caution(&mut self, message: PhpMixed);

    /// Formats a table.
    fn table(&mut self, headers: Vec<PhpMixed>, rows: Vec<PhpMixed>);

    /// Asks a question.
    fn ask(
        &mut self,
        question: &str,
        default: Option<&str>,
        validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) -> PhpMixed;

    /// Asks a question with the user input hidden.
    fn ask_hidden(
        &mut self,
        question: &str,
        validator: Option<Box<dyn Fn(Option<PhpMixed>) -> anyhow::Result<PhpMixed>>>,
    ) -> PhpMixed;

    /// Asks for confirmation.
    fn confirm(&mut self, question: &str, default: bool) -> bool;

    /// Asks a choice question.
    fn choice(
        &mut self,
        question: &str,
        choices: Vec<PhpMixed>,
        default: Option<PhpMixed>,
    ) -> PhpMixed;

    /// Add newline(s).
    fn new_line(&mut self, count: i64);

    /// Starts the progress output.
    fn progress_start(&mut self, max: i64);

    /// Advances the progress output X steps.
    fn progress_advance(&mut self, step: i64);

    /// Finishes the progress output.
    fn progress_finish(&mut self);
}
