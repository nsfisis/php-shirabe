//! ref: composer/vendor/symfony/console/Question/ConfirmationQuestion.php

use crate::exception::InvalidArgumentException;
use crate::question::{Question, QuestionInterface};
use bstr::{BStr, BString};
use shirabe_php_shim::{PhpMixed, empty, preg_is_match, strval};

/// Represents a yes/no question.
#[derive(Debug)]
pub struct ConfirmationQuestion {
    inner: Question,
    true_answer_regex: BString,
}

impl ConfirmationQuestion {
    /// `$question` The question to ask to the user.
    /// `$default` The default answer to return, true or false.
    /// `$trueAnswerRegex` A regex to match the "yes" answer.
    pub fn new(
        question: impl Into<BString>,
        default: bool,
        true_answer_regex: impl Into<BString>,
    ) -> Self {
        let mut this = Self {
            inner: Question::new(question, Some(PhpMixed::Bool(default))),
            true_answer_regex: true_answer_regex.into(),
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

            let answer_is_true = preg_is_match(&regex, strval(&answer));

            // false === $default
            if matches!(default, PhpMixed::Bool(false)) {
                // $answer && $answerIsTrue
                return PhpMixed::Bool(!empty(&answer) && answer_is_true);
            }

            // '' === $answer || $answerIsTrue
            let answer_is_empty_string = matches!(&answer, PhpMixed::String(s) if s.is_empty());
            PhpMixed::Bool(answer_is_empty_string || answer_is_true)
        })
    }
}

impl QuestionInterface for ConfirmationQuestion {
    fn get_question(&self) -> &BStr {
        self.inner.get_question()
    }

    fn get_default(&self) -> PhpMixed {
        self.inner.get_default()
    }

    fn is_multiline(&self) -> bool {
        self.inner.is_multiline()
    }

    fn is_hidden(&self) -> bool {
        self.inner.is_hidden()
    }

    fn is_hidden_fallback(&self) -> bool {
        self.inner.is_hidden_fallback()
    }

    fn get_autocompleter_values(&self) -> Option<Vec<PhpMixed>> {
        self.inner.get_autocompleter_values()
    }

    fn get_autocompleter_callback(&self) -> Option<&dyn Fn(&BStr) -> Option<Vec<PhpMixed>>> {
        self.inner.get_autocompleter_callback()
    }

    fn get_validator(
        &self,
    ) -> Option<&dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>> {
        self.inner.get_validator()
    }

    fn get_max_attempts(&self) -> Option<i64> {
        self.inner.get_max_attempts()
    }

    fn get_normalizer(&self) -> Option<&dyn Fn(PhpMixed) -> PhpMixed> {
        self.inner.get_normalizer()
    }

    fn is_trimmable(&self) -> bool {
        self.inner.is_trimmable()
    }

    fn as_confirmation(&self) -> Option<&ConfirmationQuestion> {
        Some(self)
    }
}
