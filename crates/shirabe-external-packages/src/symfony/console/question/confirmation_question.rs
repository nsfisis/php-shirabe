use crate::symfony::console::question::Question;
use shirabe_php_shim::PhpMixed;

/// Represents a yes/no question.
#[derive(Debug)]
pub struct ConfirmationQuestion {
    inner: Question,
    true_answer_regex: String,
}

impl ConfirmationQuestion {
    /// `$question` The question to ask to the user.
    /// `$default` The default answer to return, true or false.
    /// `$trueAnswerRegex` A regex to match the "yes" answer.
    pub fn new(question: String, default: bool, true_answer_regex: String) -> Self {
        let mut this = Self {
            inner: Question::new(question, Some(PhpMixed::Bool(default))),
            true_answer_regex,
        };

        let normalizer = this.get_default_normalizer();
        this.inner.set_normalizer(normalizer);

        this
    }

    /// Returns the default answer normalizer.
    fn get_default_normalizer(&self) -> Box<dyn Fn(PhpMixed) -> PhpMixed> {
        let default = self.inner.get_default();
        let regex = self.true_answer_regex.clone();

        Box::new(move |answer: PhpMixed| {
            if let PhpMixed::Bool(_) = answer {
                return answer;
            }

            let answer_is_true = {
                let mut matches: Vec<Option<String>> = Vec::new();
                shirabe_php_shim::preg_match(
                    &regex,
                    &shirabe_php_shim::strval(&answer),
                    &mut matches,
                ) != 0
            };

            // false === $default
            if matches!(default, PhpMixed::Bool(false)) {
                // $answer && $answerIsTrue
                return PhpMixed::Bool(!shirabe_php_shim::empty(&answer) && answer_is_true);
            }

            // '' === $answer || $answerIsTrue
            let answer_is_empty_string = matches!(&answer, PhpMixed::String(s) if s.is_empty());
            PhpMixed::Bool(answer_is_empty_string || answer_is_true)
        })
    }
}
