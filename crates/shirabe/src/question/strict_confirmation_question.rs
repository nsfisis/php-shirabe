//! ref: composer/src/Composer/Question/StrictConfirmationQuestion.php

use shirabe_external_packages::composer::pcre::Preg;
use shirabe_external_packages::symfony::console::exception::InvalidArgumentException;
use shirabe_external_packages::symfony::console::question::Question;
use shirabe_external_packages::symfony::console::question::QuestionInterface;
use shirabe_php_shim::{PhpMixed, empty, is_bool};

#[derive(Debug)]
pub struct StrictConfirmationQuestion {
    inner: Question,
    true_answer_regex: String,
    false_answer_regex: String,
}

impl StrictConfirmationQuestion {
    pub fn new(
        question: String,
        default: bool,
        true_answer_regex: String,
        false_answer_regex: String,
    ) -> Self {
        let inner = Question::new(question, Some(PhpMixed::Bool(default)));
        let mut this = Self {
            inner,
            true_answer_regex,
            false_answer_regex,
        };
        let normalizer = this.get_default_normalizer();
        let validator = this.get_default_validator();
        this.inner.set_normalizer(normalizer);
        this.inner.set_validator(Some(validator));
        this
    }

    pub fn inner(&self) -> &Question {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut Question {
        &mut self.inner
    }

    fn get_default_normalizer(&self) -> Box<dyn Fn(PhpMixed) -> PhpMixed> {
        let default = self.inner.get_default();
        let true_regex = self.true_answer_regex.clone();
        let false_regex = self.false_answer_regex.clone();

        Box::new(move |answer: PhpMixed| {
            if is_bool(&answer) {
                return answer;
            }
            if empty(&answer) && !empty(&default) {
                return default.clone();
            }
            if let PhpMixed::String(s) = &answer {
                if Preg::is_match(&true_regex, s) {
                    return PhpMixed::Bool(true);
                }
                if Preg::is_match(&false_regex, s) {
                    return PhpMixed::Bool(false);
                }
            }
            PhpMixed::Null
        })
    }

    fn get_default_validator(
        &self,
    ) -> Box<dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>> {
        Box::new(|answer: Option<PhpMixed>| {
            let answer = answer.unwrap_or(PhpMixed::Null);
            if !is_bool(&answer) {
                return Err(InvalidArgumentException(
                    shirabe_php_shim::InvalidArgumentException {
                        message: "Please answer yes, y, no, or n.".to_string(),
                        code: 0,
                    },
                ));
            }
            Ok(answer)
        })
    }
}

// PHP: `class StrictConfirmationQuestion extends Question` (not ConfirmationQuestion),
// so it is not an instanceof ChoiceQuestion/ConfirmationQuestion and keeps the default
// downcasts returning None.
impl QuestionInterface for StrictConfirmationQuestion {
    fn get_question(&self) -> &str {
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

    fn get_autocompleter_callback(&self) -> Option<&dyn Fn(&str) -> Option<Vec<PhpMixed>>> {
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
}
