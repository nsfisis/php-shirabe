//! ref: composer/src/Composer/Question/StrictConfirmationQuestion.php

use anyhow::Result;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use shirabe_external_packages::symfony::console::question::question::Question;
use shirabe_php_shim::{empty, is_bool, PhpMixed};

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
        let inner = Question::new(question, PhpMixed::Bool(default));
        let mut this = Self {
            inner,
            true_answer_regex,
            false_answer_regex,
        };
        let normalizer = this.get_default_normalizer();
        let validator = this.get_default_validator();
        this.inner.set_normalizer(normalizer);
        this.inner.set_validator(validator);
        this
    }

    fn get_default_normalizer(&self) -> Box<dyn Fn(&PhpMixed) -> PhpMixed> {
        let default = self.inner.get_default();
        let true_regex = self.true_answer_regex.clone();
        let false_regex = self.false_answer_regex.clone();

        Box::new(move |answer: &PhpMixed| {
            if is_bool(answer) {
                return answer.clone();
            }
            if empty(answer) && !empty(&default) {
                return default.clone();
            }
            if let PhpMixed::String(s) = answer {
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

    fn get_default_validator(&self) -> Box<dyn Fn(&PhpMixed) -> Result<PhpMixed>> {
        Box::new(|answer: &PhpMixed| {
            if !is_bool(answer) {
                return Err(InvalidArgumentException {
                    message: "Please answer yes, y, no, or n.".to_string(),
                    code: 0,
                }.into());
            }
            Ok(answer.clone())
        })
    }
}
