//! ref: composer/src/Composer/Question/StrictConfirmationQuestion.php sha1:bcd82725eac5bd21e87d33e8155b41d65274dafa

use shirabe_php_shim::preg_is_match;
use shirabe_symfony_console::exception::InvalidArgumentException;
use shirabe_symfony_console::input::InputValue;
use shirabe_symfony_console::question::{Question, QuestionInterface};

#[derive(Debug)]
pub struct StrictConfirmationQuestion {
    inner: Question,
    true_answer_regex: bstr::BString,
    false_answer_regex: bstr::BString,
}

impl StrictConfirmationQuestion {
    pub fn new(
        question: impl Into<bstr::BString>,
        default: bool,
        true_answer_regex: impl Into<bstr::BString>,
        false_answer_regex: impl Into<bstr::BString>,
    ) -> Self {
        let inner = Question::new(question, InputValue::Bool(default));
        let mut this = Self {
            inner,
            true_answer_regex: true_answer_regex.into(),
            false_answer_regex: false_answer_regex.into(),
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

    fn get_default_normalizer(&self) -> Box<dyn Fn(InputValue) -> InputValue> {
        let default = self.inner.get_default();
        let true_regex = self.true_answer_regex.clone();
        let false_regex = self.false_answer_regex.clone();

        Box::new(move |answer: InputValue| {
            if let InputValue::Bool(_) = answer {
                return answer;
            }
            if !answer.to_bool() && default.to_bool() {
                return default.clone();
            }
            if let InputValue::String(s) = &answer {
                if preg_is_match(&true_regex, s) {
                    return InputValue::Bool(true);
                }
                if preg_is_match(&false_regex, s) {
                    return InputValue::Bool(false);
                }
            }
            InputValue::Null
        })
    }

    fn get_default_validator(
        &self,
    ) -> Box<dyn Fn(InputValue) -> Result<InputValue, InvalidArgumentException>> {
        Box::new(|answer: InputValue| {
            if !matches!(answer, InputValue::Bool(_)) {
                return Err(InvalidArgumentException::new(
                    "Please answer yes, y, no, or n.",
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
    fn get_question(&self) -> &bstr::BStr {
        self.inner.get_question()
    }

    fn get_default(&self) -> InputValue {
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

    fn get_autocompleter_values(&self) -> Option<Vec<bstr::BString>> {
        self.inner.get_autocompleter_values()
    }

    fn get_autocompleter_callback(
        &self,
    ) -> Option<&dyn Fn(&bstr::BStr) -> Option<Vec<bstr::BString>>> {
        self.inner.get_autocompleter_callback()
    }

    fn get_validator(
        &self,
    ) -> Option<&dyn Fn(InputValue) -> Result<InputValue, InvalidArgumentException>> {
        self.inner.get_validator()
    }

    fn get_max_attempts(&self) -> Option<i64> {
        self.inner.get_max_attempts()
    }

    fn get_normalizer(&self) -> Option<&dyn Fn(InputValue) -> InputValue> {
        self.inner.get_normalizer()
    }

    fn is_trimmable(&self) -> bool {
        self.inner.is_trimmable()
    }
}
