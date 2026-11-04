//! ref: composer/vendor/symfony/console/Question/Question.php

use crate::exception::{InvalidArgumentException, LogicException};
use crate::input::InputValue;
use crate::question::choice_question::ChoiceQuestion;
use crate::question::confirmation_question::ConfirmationQuestion;
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::canonical_int_key;

/// Polymorphic boundary for the Symfony Console Question hierarchy.
///
/// PHP has no `QuestionInterface`; `Question` is a concrete class extended by
/// `ChoiceQuestion`/`ConfirmationQuestion`. Modelling those subclasses as
/// `inner: Question` composition loses subtype identity, so consumers that take
/// a `Question` and run `instanceof` checks are expressed here as a trait whose
/// `as_choice`/`as_confirmation` downcasts stand in for `instanceof`.
///
/// Defaults, answers and the values validators and normalizers take and return are
/// `InputValue`s; see there for how Symfony's `mixed` is narrowed.
pub trait QuestionInterface: std::fmt::Debug {
    fn get_question(&self) -> &BStr;

    fn get_default(&self) -> InputValue;

    fn is_multiline(&self) -> bool;

    fn is_hidden(&self) -> bool;

    fn is_hidden_fallback(&self) -> bool;

    fn get_autocompleter_values(&self) -> Option<Vec<BString>>;

    fn get_autocompleter_callback(&self) -> Option<&dyn Fn(&BStr) -> Option<Vec<BString>>>;

    fn get_validator(
        &self,
    ) -> Option<&dyn Fn(InputValue) -> Result<InputValue, InvalidArgumentException>>;

    fn get_max_attempts(&self) -> Option<i64>;

    fn get_normalizer(&self) -> Option<&dyn Fn(InputValue) -> InputValue>;

    fn is_trimmable(&self) -> bool;

    /// Models `$question instanceof ChoiceQuestion`.
    fn as_choice(&self) -> Option<&ChoiceQuestion> {
        None
    }

    /// Models `$question instanceof ConfirmationQuestion`.
    fn as_confirmation(&self) -> Option<&ConfirmationQuestion> {
        None
    }
}

/// Represents a Question.
pub struct Question {
    question: BString,
    attempts: Option<i64>,
    hidden: bool,
    hidden_fallback: bool,
    autocompleter_callback: Option<Box<dyn Fn(&BStr) -> Option<Vec<BString>>>>,
    validator: Option<Box<dyn Fn(InputValue) -> Result<InputValue, InvalidArgumentException>>>,
    default: InputValue,
    normalizer: Option<Box<dyn Fn(InputValue) -> InputValue>>,
    trimmable: bool,
    multiline: bool,
}

impl std::fmt::Debug for Question {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Question")
            .field("question", &self.question)
            .field("attempts", &self.attempts)
            .field("hidden", &self.hidden)
            .field("hidden_fallback", &self.hidden_fallback)
            .field("default", &self.default)
            .field("trimmable", &self.trimmable)
            .field("multiline", &self.multiline)
            .finish_non_exhaustive()
    }
}

impl Question {
    /// `$question` The question to ask to the user.
    /// `$default` The default answer to return if the user enters nothing.
    pub fn new(question: impl Into<BString>, default: InputValue) -> Self {
        Self {
            question: question.into(),
            attempts: None,
            hidden: false,
            hidden_fallback: true,
            autocompleter_callback: None,
            validator: None,
            default,
            normalizer: None,
            trimmable: true,
            multiline: false,
        }
    }

    /// Returns the question.
    pub fn get_question(&self) -> &BStr {
        self.question.as_bstr()
    }

    /// Returns the default answer.
    pub fn get_default(&self) -> InputValue {
        self.default.clone()
    }

    /// Returns whether the user response accepts newline characters.
    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    /// Sets whether the user response should accept newline characters.
    pub fn set_multiline(&mut self, multiline: bool) -> &mut Self {
        self.multiline = multiline;

        self
    }

    /// Returns whether the user response must be hidden.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }

    /// Sets whether the user response must be hidden or not.
    ///
    /// Throws LogicException in case the autocompleter is also used.
    pub fn set_hidden(&mut self, hidden: bool) -> Result<&mut Self, LogicException> {
        if self.autocompleter_callback.is_some() {
            return Err(LogicException::new(
                "A hidden question cannot use the autocompleter.",
            ));
        }

        self.hidden = hidden;

        Ok(self)
    }

    /// In case the response cannot be hidden, whether to fallback on non-hidden question or not.
    pub fn is_hidden_fallback(&self) -> bool {
        self.hidden_fallback
    }

    /// Sets whether to fallback on non-hidden question if the response cannot be hidden.
    pub fn set_hidden_fallback(&mut self, fallback: bool) -> &mut Self {
        self.hidden_fallback = fallback;

        self
    }

    /// Gets values for the autocompleter.
    pub fn get_autocompleter_values(&self) -> Option<Vec<BString>> {
        let callback = self.get_autocompleter_callback();

        match callback {
            Some(callback) => callback(BStr::new("")),
            None => None,
        }
    }

    /// Sets values for the autocompleter.
    ///
    /// Throws LogicException.
    // TODO(type-model): PHP also accepts a `\Traversable`, whose values are cached on the first
    // call; only arrays are modelled.
    pub fn set_autocompleter_values(
        &mut self,
        values: Option<IndexMap<BString, BString>>,
    ) -> Result<&mut Self, LogicException> {
        let callback: Option<Box<dyn Fn(&BStr) -> Option<Vec<BString>>>> = match values {
            Some(values) => {
                let values: Vec<BString> = if Self::is_assoc(&values) {
                    // array_merge(array_keys($values), array_values($values))
                    values.keys().chain(values.values()).cloned().collect()
                } else {
                    // array_values($values)
                    values.into_values().collect()
                };

                Some(Box::new(move |_input: &BStr| Some(values.clone())))
            }
            None => None,
        };

        self.set_autocompleter_callback(callback)
    }

    /// Gets the callback function used for the autocompleter.
    pub fn get_autocompleter_callback(&self) -> Option<&dyn Fn(&BStr) -> Option<Vec<BString>>> {
        self.autocompleter_callback.as_deref()
    }

    /// Sets the callback function used for the autocompleter.
    ///
    /// The callback is passed the user input as argument and should return an iterable of
    /// corresponding suggestions.
    pub fn set_autocompleter_callback(
        &mut self,
        callback: Option<Box<dyn Fn(&BStr) -> Option<Vec<BString>>>>,
    ) -> Result<&mut Self, LogicException> {
        if self.hidden && callback.is_some() {
            return Err(LogicException::new(
                "A hidden question cannot use the autocompleter.",
            ));
        }

        self.autocompleter_callback = callback;

        Ok(self)
    }

    /// Sets a validator for the question.
    pub fn set_validator(
        &mut self,
        validator: Option<Box<dyn Fn(InputValue) -> Result<InputValue, InvalidArgumentException>>>,
    ) -> &mut Self {
        self.validator = validator;

        self
    }

    /// Gets the validator for the question.
    pub fn get_validator(
        &self,
    ) -> Option<&dyn Fn(InputValue) -> Result<InputValue, InvalidArgumentException>> {
        self.validator.as_deref()
    }

    /// Sets the maximum number of attempts.
    ///
    /// Null means an unlimited number of attempts.
    ///
    /// Throws InvalidArgumentException in case the number of attempts is invalid.
    pub fn set_max_attempts(
        &mut self,
        attempts: Option<i64>,
    ) -> Result<&mut Self, InvalidArgumentException> {
        if let Some(attempts) = attempts
            && attempts < 1
        {
            return Err(InvalidArgumentException::new(
                "Maximum number of attempts must be a positive value.",
            ));
        }

        self.attempts = attempts;

        Ok(self)
    }

    /// Gets the maximum number of attempts.
    ///
    /// Null means an unlimited number of attempts.
    pub fn get_max_attempts(&self) -> Option<i64> {
        self.attempts
    }

    /// Sets a normalizer for the response.
    ///
    /// The normalizer can be a callable (a string), a closure or a class implementing __invoke.
    pub fn set_normalizer(
        &mut self,
        normalizer: Box<dyn Fn(InputValue) -> InputValue>,
    ) -> &mut Self {
        self.normalizer = Some(normalizer);

        self
    }

    /// Gets the normalizer for the response.
    ///
    /// The normalizer can ba a callable (a string), a closure or a class implementing __invoke.
    pub fn get_normalizer(&self) -> Option<&dyn Fn(InputValue) -> InputValue> {
        self.normalizer.as_deref()
    }

    // PHP: `(bool) \count(array_filter(array_keys($array), 'is_string'))`. PHP normalizes
    // canonical-integer string keys (e.g. "0", "12") to int keys, so those do not count.
    pub(crate) fn is_assoc<V>(array: &IndexMap<BString, V>) -> bool {
        array.keys().any(|key| canonical_int_key(key).is_none())
    }

    pub fn is_trimmable(&self) -> bool {
        self.trimmable
    }

    pub fn set_trimmable(&mut self, trimmable: bool) -> &mut Self {
        self.trimmable = trimmable;

        self
    }
}

impl QuestionInterface for Question {
    fn get_question(&self) -> &BStr {
        self.get_question()
    }

    fn get_default(&self) -> InputValue {
        self.get_default()
    }

    fn is_multiline(&self) -> bool {
        self.is_multiline()
    }

    fn is_hidden(&self) -> bool {
        self.is_hidden()
    }

    fn is_hidden_fallback(&self) -> bool {
        self.is_hidden_fallback()
    }

    fn get_autocompleter_values(&self) -> Option<Vec<BString>> {
        self.get_autocompleter_values()
    }

    fn get_autocompleter_callback(&self) -> Option<&dyn Fn(&BStr) -> Option<Vec<BString>>> {
        self.get_autocompleter_callback()
    }

    fn get_validator(
        &self,
    ) -> Option<&dyn Fn(InputValue) -> Result<InputValue, InvalidArgumentException>> {
        self.get_validator()
    }

    fn get_max_attempts(&self) -> Option<i64> {
        self.get_max_attempts()
    }

    fn get_normalizer(&self) -> Option<&dyn Fn(InputValue) -> InputValue> {
        self.get_normalizer()
    }

    fn is_trimmable(&self) -> bool {
        self.is_trimmable()
    }
}
