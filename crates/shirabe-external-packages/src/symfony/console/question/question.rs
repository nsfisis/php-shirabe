//! ref: composer/vendor/symfony/console/Question/Question.php

use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::logic_exception::LogicException;
use crate::symfony::console::question::choice_question::ChoiceQuestion;
use crate::symfony::console::question::confirmation_question::ConfirmationQuestion;
use shirabe_php_shim::PhpMixed;

/// Polymorphic boundary for the Symfony Console Question hierarchy.
///
/// PHP has no `QuestionInterface`; `Question` is a concrete class extended by
/// `ChoiceQuestion`/`ConfirmationQuestion`. Modelling those subclasses as
/// `inner: Question` composition loses subtype identity, so consumers that take
/// a `Question` and run `instanceof` checks are expressed here as a trait whose
/// `as_choice`/`as_confirmation` downcasts stand in for `instanceof`.
pub trait QuestionInterface: std::fmt::Debug {
    fn get_question(&self) -> &str;

    fn get_default(&self) -> PhpMixed;

    fn is_multiline(&self) -> bool;

    fn is_hidden(&self) -> bool;

    fn is_hidden_fallback(&self) -> bool;

    fn get_autocompleter_values(&self) -> Option<Vec<PhpMixed>>;

    fn get_autocompleter_callback(&self) -> Option<&dyn Fn(&str) -> Option<Vec<PhpMixed>>>;

    fn get_validator(
        &self,
    ) -> Option<&dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>>;

    fn get_max_attempts(&self) -> Option<i64>;

    fn get_normalizer(&self) -> Option<&dyn Fn(PhpMixed) -> PhpMixed>;

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
    question: String,
    attempts: Option<i64>,
    hidden: bool,
    hidden_fallback: bool,
    autocompleter_callback: Option<Box<dyn Fn(&str) -> Option<Vec<PhpMixed>>>>,
    validator: Option<Box<dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>>>,
    default: Option<PhpMixed>,
    normalizer: Option<Box<dyn Fn(PhpMixed) -> PhpMixed>>,
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
    pub fn new(question: String, default: Option<PhpMixed>) -> Self {
        Self {
            question,
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
    pub fn get_question(&self) -> &str {
        &self.question
    }

    /// Returns the default answer.
    pub fn get_default(&self) -> PhpMixed {
        self.default.clone().unwrap_or(PhpMixed::Null)
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
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: "A hidden question cannot use the autocompleter.".to_string(),
                code: 0,
            }));
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
    pub fn get_autocompleter_values(&self) -> Option<Vec<PhpMixed>> {
        let callback = self.get_autocompleter_callback();

        match callback {
            Some(callback) => callback(""),
            None => None,
        }
    }

    /// Sets values for the autocompleter.
    ///
    /// Throws LogicException.
    pub fn set_autocompleter_values(
        &mut self,
        values: Option<PhpMixed>,
    ) -> Result<&mut Self, LogicException> {
        let callback: Option<Box<dyn Fn(&str) -> Option<Vec<PhpMixed>>>> = match values {
            // PHP: `if (\is_array($values))`. Both PhpMixed::List and ::Array model PHP arrays.
            Some(values) if matches!(values, PhpMixed::List(_) | PhpMixed::Array(_)) => {
                let values = if Self::is_assoc(&values) {
                    let array = match &values {
                        PhpMixed::Array(array) => array,
                        _ => unreachable!(),
                    };
                    // array_merge(array_keys($values), array_values($values))
                    let mut merged: Vec<PhpMixed> =
                        array.keys().map(|k| PhpMixed::String(k.clone())).collect();
                    merged.extend(array.values().cloned());
                    merged
                } else {
                    // array_values($values)
                    match &values {
                        PhpMixed::List(list) => list.to_vec(),
                        PhpMixed::Array(array) => array.values().cloned().collect(),
                        _ => unreachable!(),
                    }
                };

                Some(Box::new(move |_input: &str| Some(values.clone())))
            }
            // PHP: `elseif ($values instanceof \Traversable)`. In PHP this caches the
            // iterator result; here non-array iterables are not modeled, so treat any
            // remaining value as the Traversable branch.
            Some(values) => {
                // PHP: `iterator_to_array($values, false)` caches a Traversable.
                // Non-array iterables are not modeled by PhpMixed; extract any
                // list/array elements, otherwise treat as an empty iterator.
                let cached: Vec<PhpMixed> = match values {
                    PhpMixed::List(list) => list.into_iter().collect(),
                    PhpMixed::Array(array) => array.into_values().collect(),
                    _ => Vec::new(),
                };
                Some(Box::new(move |_input: &str| Some(cached.clone())))
            }
            None => None,
        };

        self.set_autocompleter_callback(callback)
    }

    /// Gets the callback function used for the autocompleter.
    pub fn get_autocompleter_callback(&self) -> Option<&dyn Fn(&str) -> Option<Vec<PhpMixed>>> {
        self.autocompleter_callback.as_deref()
    }

    /// Sets the callback function used for the autocompleter.
    ///
    /// The callback is passed the user input as argument and should return an iterable of
    /// corresponding suggestions.
    pub fn set_autocompleter_callback(
        &mut self,
        callback: Option<Box<dyn Fn(&str) -> Option<Vec<PhpMixed>>>>,
    ) -> Result<&mut Self, LogicException> {
        if self.hidden && callback.is_some() {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: "A hidden question cannot use the autocompleter.".to_string(),
                code: 0,
            }));
        }

        self.autocompleter_callback = callback;

        Ok(self)
    }

    /// Sets a validator for the question.
    pub fn set_validator(
        &mut self,
        validator: Option<
            Box<dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>>,
        >,
    ) -> &mut Self {
        self.validator = validator;

        self
    }

    /// Gets the validator for the question.
    pub fn get_validator(
        &self,
    ) -> Option<&dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>> {
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
            return Err(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: "Maximum number of attempts must be a positive value.".to_string(),
                    code: 0,
                },
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
    pub fn set_normalizer(&mut self, normalizer: Box<dyn Fn(PhpMixed) -> PhpMixed>) -> &mut Self {
        self.normalizer = Some(normalizer);

        self
    }

    /// Gets the normalizer for the response.
    ///
    /// The normalizer can ba a callable (a string), a closure or a class implementing __invoke.
    pub fn get_normalizer(&self) -> Option<&dyn Fn(PhpMixed) -> PhpMixed> {
        self.normalizer.as_deref()
    }

    // PHP: `(bool) \count(array_filter(array_keys($array), 'is_string'))`.
    // A `List` has only sequential int keys, so it is never associative. An `Array` is
    // associative only when at least one key is a genuine string key; PHP normalizes
    // canonical-integer string keys (e.g. "0", "12") back to int keys, so those do not count.
    // The same heuristic (a key is "string" iff it does not parse as an i64) is used by
    // ConsoleIO::select when computing `$isAssoc` over the choice map.
    pub(crate) fn is_assoc(array: &PhpMixed) -> bool {
        match array {
            PhpMixed::Array(map) => map.keys().any(|key| key.parse::<i64>().is_err()),
            _ => false,
        }
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
    fn get_question(&self) -> &str {
        self.get_question()
    }

    fn get_default(&self) -> PhpMixed {
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

    fn get_autocompleter_values(&self) -> Option<Vec<PhpMixed>> {
        self.get_autocompleter_values()
    }

    fn get_autocompleter_callback(&self) -> Option<&dyn Fn(&str) -> Option<Vec<PhpMixed>>> {
        self.get_autocompleter_callback()
    }

    fn get_validator(
        &self,
    ) -> Option<&dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>> {
        self.get_validator()
    }

    fn get_max_attempts(&self) -> Option<i64> {
        self.get_max_attempts()
    }

    fn get_normalizer(&self) -> Option<&dyn Fn(PhpMixed) -> PhpMixed> {
        self.get_normalizer()
    }

    fn is_trimmable(&self) -> bool {
        self.is_trimmable()
    }
}
