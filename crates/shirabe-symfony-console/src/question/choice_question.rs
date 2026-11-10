//! ref: composer/vendor/symfony/console/Question/ChoiceQuestion.php sha1:c595c20b23898288355b5b51c68ab411996362c4

use crate::exception::{InvalidArgumentException, LogicException};
use crate::input::InputValue;
use crate::question::{Question, QuestionInterface};
use bstr::{B, BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpMixed, ToPhpString, bformat, explode, implode, loose_eq, php_regex, preg_is_match, sprintf,
    trim,
};

/// Represents a choice question.
// TODO(type-model): Symfony accepts choices of any type; only strings are modelled, which is what
// `ConsoleIO::select()` passes (its `sanitize()` takes strings under `strict_types`).
#[derive(Debug)]
pub struct ChoiceQuestion {
    inner: Question,
    choices: IndexMap<BString, BString>,
    multiselect: bool,
    prompt: BString,
    error_message: BString,
}

impl ChoiceQuestion {
    /// `$question` The question to ask to the user.
    /// `$choices` The list of available choices.
    /// `$default` The default answer to return.
    pub fn new(
        question: impl Into<BString>,
        choices: IndexMap<BString, BString>,
        default: InputValue,
    ) -> Result<Self, LogicException> {
        if choices.is_empty() {
            return Err(LogicException::new(
                "Choice question must have at least 1 choice available.",
            ));
        }

        let mut this = Self {
            inner: Question::new(question, default),
            choices: choices.clone(),
            multiselect: false,
            prompt: BString::from(" > "),
            error_message: BString::from("Value \"%s\" is invalid"),
        };

        let validator = this.get_default_validator();
        this.inner.set_validator(Some(validator));
        // setAutocompleterValues never throws for an array argument.
        this.inner
            .set_autocompleter_values(Some(choices))
            .expect("autocompleter cannot be set on a hidden question during construction");

        Ok(this)
    }

    /// Returns available choices.
    pub fn get_choices(&self) -> &IndexMap<BString, BString> {
        &self.choices
    }

    /// Sets multiselect option.
    ///
    /// When multiselect is set to true, multiple choices can be answered.
    pub fn set_multiselect(&mut self, multiselect: bool) -> &mut Self {
        self.multiselect = multiselect;
        let validator = self.get_default_validator();
        self.inner.set_validator(Some(validator));

        self
    }

    /// Returns whether the choices are multiselect.
    pub fn is_multiselect(&self) -> bool {
        self.multiselect
    }

    /// Gets the prompt for choices.
    pub fn get_prompt(&self) -> &BStr {
        self.prompt.as_bstr()
    }

    /// Sets the prompt for choices.
    pub fn set_prompt(&mut self, prompt: impl Into<BString>) -> &mut Self {
        self.prompt = prompt.into();

        self
    }

    /// Inherited from Question. Sets the maximum number of attempts.
    pub fn set_max_attempts(
        &mut self,
        attempts: Option<i64>,
    ) -> Result<&mut Self, InvalidArgumentException> {
        self.inner.set_max_attempts(attempts)?;

        Ok(self)
    }

    /// Sets the error message for invalid values.
    ///
    /// The error message has a string placeholder (%s) for the invalid value.
    pub fn set_error_message(&mut self, error_message: impl Into<BString>) -> &mut Self {
        self.error_message = error_message.into();
        let validator = self.get_default_validator();
        self.inner.set_validator(Some(validator));

        self
    }

    fn get_default_validator(
        &self,
    ) -> Box<dyn Fn(InputValue) -> Result<InputValue, InvalidArgumentException>> {
        let choices = self.choices.clone();
        let error_message = self.error_message.clone();
        let multiselect = self.multiselect;
        let is_assoc = Question::is_assoc(&self.choices);
        // PHP reads `$this->isTrimmable()` live inside the closure. A 'static boxed
        // closure cannot borrow `$this`, so the value is snapshotted at validator
        // creation time. setValidator is re-run on multiselect/errorMessage changes,
        // but a later setTrimmable would not be reflected.
        let trimmable = self.inner.is_trimmable();

        Box::new(move |selected: InputValue| {
            let mut selected_choices: Vec<InputValue> = if multiselect {
                // Check for a separated comma values
                if !preg_is_match(php_regex!("/^[^,]+(?:,[^,]+)*$/"), selected.to_php_string()) {
                    return Err(InvalidArgumentException::new(sprintf(
                        &error_message,
                        &[selected.to_php_mixed()],
                    )));
                }

                explode(",", selected.to_php_string())
                    .into_iter()
                    .map(InputValue::String)
                    .collect()
            } else {
                vec![selected]
            };

            if trimmable {
                for v in selected_choices.iter_mut() {
                    *v = InputValue::String(trim(v.to_php_string()));
                }
            }

            let mut multiselect_choices: Vec<BString> = Vec::new();
            for value in &selected_choices {
                let mut results: Vec<&BString> = Vec::new();
                for (key, choice) in &choices {
                    if InputValue::String(choice.clone()) == *value {
                        results.push(key);
                    }
                }

                if results.len() > 1 {
                    return Err(InvalidArgumentException::new(bformat!(
                        "The provided answer is ambiguous. Value should be one of \"{}\".",
                        implode("\" or \"", &results)
                    )));
                }

                // array_search($value, $choices)
                let needle = value.to_php_mixed();
                let result_key = choices
                    .iter()
                    .find(|(_, choice)| loose_eq(&PhpMixed::string((*choice).clone()), &needle))
                    .map(|(key, _)| key.clone());

                let result: Option<BString> = if !is_assoc {
                    match &result_key {
                        // $result = $choices[$result];
                        Some(found_key) => Some(choices[B(found_key)].clone()),
                        // isset($choices[$value])
                        None => choices.get(B(&value.array_key())).cloned(),
                    }
                } else if result_key.is_none() && choices.contains_key(B(&value.array_key())) {
                    // false === $result && isset($choices[$value])
                    Some(value.to_php_string())
                } else {
                    result_key
                };

                // false === $result
                let Some(result) = result else {
                    return Err(InvalidArgumentException::new(sprintf(
                        &error_message,
                        &[value.to_php_mixed()],
                    )));
                };

                // For associative choices, consistently return the key as string:
                multiselect_choices.push(result);
            }

            if multiselect {
                return Ok(InputValue::Array(multiselect_choices));
            }

            Ok(InputValue::String(
                multiselect_choices
                    .into_iter()
                    .next()
                    .expect("a single selection yields one choice"),
            ))
        })
    }
}

impl QuestionInterface for ChoiceQuestion {
    fn get_question(&self) -> &BStr {
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

    fn get_autocompleter_values(&self) -> Option<Vec<BString>> {
        self.inner.get_autocompleter_values()
    }

    fn get_autocompleter_callback(&self) -> Option<&dyn Fn(&BStr) -> Option<Vec<BString>>> {
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

    fn as_choice(&self) -> Option<&ChoiceQuestion> {
        Some(self)
    }
}
