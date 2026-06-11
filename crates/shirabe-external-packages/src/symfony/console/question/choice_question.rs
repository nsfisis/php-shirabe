use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::logic_exception::LogicException;
use crate::symfony::console::question::Question;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// Represents a choice question.
#[derive(Debug)]
pub struct ChoiceQuestion {
    inner: Question,
    choices: IndexMap<String, Box<PhpMixed>>,
    multiselect: bool,
    prompt: String,
    error_message: String,
}

impl ChoiceQuestion {
    /// `$question` The question to ask to the user.
    /// `$choices` The list of available choices.
    /// `$default` The default answer to return.
    pub fn new(
        question: String,
        choices: IndexMap<String, Box<PhpMixed>>,
        default: Option<PhpMixed>,
    ) -> Result<Self, LogicException> {
        if choices.is_empty() {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: "Choice question must have at least 1 choice available.".to_string(),
                code: 0,
            }));
        }

        let mut this = Self {
            inner: Question::new(question, default),
            choices: choices.clone(),
            multiselect: false,
            prompt: " > ".to_string(),
            error_message: "Value \"%s\" is invalid".to_string(),
        };

        let validator = this.get_default_validator();
        this.inner.set_validator(Some(validator));
        // setAutocompleterValues never throws for an array argument.
        this.inner
            .set_autocompleter_values(Some(PhpMixed::Array(choices)))
            .expect("autocompleter cannot be set on a hidden question during construction");

        Ok(this)
    }

    /// Returns available choices.
    pub fn get_choices(&self) -> &IndexMap<String, Box<PhpMixed>> {
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
    pub fn get_prompt(&self) -> &str {
        &self.prompt
    }

    /// Sets the prompt for choices.
    pub fn set_prompt(&mut self, prompt: String) -> &mut Self {
        self.prompt = prompt;

        self
    }

    /// Sets the error message for invalid values.
    ///
    /// The error message has a string placeholder (%s) for the invalid value.
    pub fn set_error_message(&mut self, error_message: String) -> &mut Self {
        self.error_message = error_message;
        let validator = self.get_default_validator();
        self.inner.set_validator(Some(validator));

        self
    }

    fn get_default_validator(
        &self,
    ) -> Box<dyn Fn(Option<PhpMixed>) -> Result<PhpMixed, InvalidArgumentException>> {
        let choices = self.choices.clone();
        let error_message = self.error_message.clone();
        let multiselect = self.multiselect;
        let is_assoc = Question::is_assoc(&PhpMixed::Array(self.choices.clone()));
        // PHP reads `$this->isTrimmable()` live inside the closure. A 'static boxed
        // closure cannot borrow `$this`, so the value is snapshotted at validator
        // creation time. setValidator is re-run on multiselect/errorMessage changes,
        // but a later setTrimmable would not be reflected. See review notes.
        let trimmable = self.inner.is_trimmable();

        Box::new(move |selected: Option<PhpMixed>| {
            let selected = selected.unwrap_or(PhpMixed::Null);

            let selected_choices: Vec<PhpMixed> = if multiselect {
                // Check for a separated comma values
                let mut matches: Vec<Option<String>> = Vec::new();
                if shirabe_php_shim::preg_match(
                    "/^[^,]+(?:,[^,]+)*$/",
                    &shirabe_php_shim::strval(&selected),
                    &mut matches,
                ) == 0
                {
                    return Err(InvalidArgumentException(
                        shirabe_php_shim::InvalidArgumentException {
                            message: shirabe_php_shim::sprintf(&error_message, &[selected.clone()]),
                            code: 0,
                        },
                    ));
                }

                shirabe_php_shim::explode(",", &shirabe_php_shim::strval(&selected))
                    .into_iter()
                    .map(PhpMixed::String)
                    .collect()
            } else {
                vec![selected.clone()]
            };

            let mut selected_choices = selected_choices;
            if trimmable {
                for v in selected_choices.iter_mut() {
                    *v = PhpMixed::String(shirabe_php_shim::trim(
                        &shirabe_php_shim::strval(v),
                        None,
                    ));
                }
            }

            let mut multiselect_choices: Vec<PhpMixed> = Vec::new();
            for value in &selected_choices {
                let mut results: Vec<String> = Vec::new();
                for (key, choice) in &choices {
                    if (**choice) == *value {
                        results.push(key.clone());
                    }
                }

                if results.len() > 1 {
                    return Err(InvalidArgumentException(
                        shirabe_php_shim::InvalidArgumentException {
                            message: shirabe_php_shim::sprintf(
                                "The provided answer is ambiguous. Value should be one of \"%s\".",
                                &[PhpMixed::String(shirabe_php_shim::implode(
                                    "\" or \"", &results,
                                ))],
                            ),
                            code: 0,
                        },
                    ));
                }

                // array_search($value, $choices)
                let result_key = shirabe_php_shim::array_search(
                    &shirabe_php_shim::strval(value),
                    &choices_as_str(&choices),
                );

                let mut result: PhpMixed;
                if !is_assoc {
                    if let Some(found_key) = &result_key {
                        // $result = $choices[$result];
                        result = (*choices[found_key]).clone();
                    } else if let Some(found) = choices.get(&shirabe_php_shim::strval(value)) {
                        // isset($choices[$value])
                        result = (**found).clone();
                    } else {
                        result = PhpMixed::Bool(false);
                    }
                } else if result_key.is_none() {
                    if let Some(_found) = choices.get(&shirabe_php_shim::strval(value)) {
                        // false === $result && isset($choices[$value])
                        result = value.clone();
                    } else {
                        result = PhpMixed::Bool(false);
                    }
                } else {
                    // associative, found: keep the matched key
                    result = PhpMixed::String(result_key.clone().unwrap());
                }

                // false === $result
                if matches!(result, PhpMixed::Bool(false)) {
                    return Err(InvalidArgumentException(
                        shirabe_php_shim::InvalidArgumentException {
                            message: shirabe_php_shim::sprintf(&error_message, &[value.clone()]),
                            code: 0,
                        },
                    ));
                }

                // For associative choices, consistently return the key as string:
                if is_assoc {
                    result = PhpMixed::String(shirabe_php_shim::strval(&result));
                }
                multiselect_choices.push(result);
            }

            if multiselect {
                return Ok(PhpMixed::List(
                    multiselect_choices.into_iter().map(Box::new).collect(),
                ));
            }

            Ok(shirabe_php_shim::current(PhpMixed::List(
                multiselect_choices.into_iter().map(Box::new).collect(),
            )))
        })
    }
}

/// array_search operates over the choice values as strings; this projects the
/// choices map's values into the string-keyed form the shim expects.
fn choices_as_str(choices: &IndexMap<String, Box<PhpMixed>>) -> IndexMap<String, String> {
    choices
        .iter()
        .map(|(k, v)| (k.clone(), shirabe_php_shim::strval(v)))
        .collect()
}
