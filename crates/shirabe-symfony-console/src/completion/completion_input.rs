//! ref: composer/vendor/symfony/console/Completion/CompletionInput.php

use crate::input::argv_input::ArgvInput;
use crate::input::input_definition::InputDefinition;
use crate::input::input_option::InputOption;
use shirabe_php_shim::{PhpMixed, php_regex};

/// An input specialized for shell completion.
///
/// This input allows unfinished option names or values and exposes what kind of
/// completion is expected.
#[derive(Debug, Clone)]
pub struct CompletionInput {
    inner: ArgvInput,
    tokens: Vec<String>,
    current_index: i64,
    completion_type: String,
    completion_name: Option<String>,
    completion_value: String,
}

impl CompletionInput {
    pub const TYPE_ARGUMENT_VALUE: &'static str = "argument_value";
    pub const TYPE_OPTION_VALUE: &'static str = "option_value";
    pub const TYPE_OPTION_NAME: &'static str = "option_name";
    pub const TYPE_NONE: &'static str = "none";

    /// Converts a terminal string into tokens.
    ///
    /// This is required for shell completions without COMP_WORDS support.
    pub fn from_string(input_str: &str, current_index: i64) -> anyhow::Result<Self> {
        let tokens = shirabe_php_shim::preg_match_all(
            php_regex!("/(?<=^|\\s)(['\"]?)(.+?)(?<!\\\\)\\1(?=$|\\s)/"),
            input_str,
        );

        Self::from_tokens(tokens[0].clone(), current_index)
    }

    /// Create an input based on an COMP_WORDS token list.
    ///
    /// `tokens` is the set of split tokens (e.g. COMP_WORDS or argv),
    /// `current_index` the index of the cursor (e.g. COMP_CWORD).
    pub fn from_tokens(tokens: Vec<String>, current_index: i64) -> anyhow::Result<Self> {
        let mut input = Self {
            inner: ArgvInput::new(Some(tokens.clone()), None)?,
            tokens: vec![],
            current_index: 0,
            completion_type: String::new(),
            completion_name: None,
            completion_value: String::new(),
        };
        input.tokens = tokens;
        input.current_index = current_index;

        Ok(input)
    }

    pub fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        self.inner
            .base_bind(definition, |argv, token, parse_options| {
                Ok(CompletionInput::parse_token(argv, token, parse_options))
            })?;

        let relevant_token = self.get_relevant_token();
        if "-" == &relevant_token[0..1] {
            // the current token is an input option: complete either option name or option value
            let parts = shirabe_php_shim::explode_with_limit("=", &relevant_token, 2);
            let option_token = parts.first().cloned().unwrap_or_default();
            let option_value = parts.get(1).cloned().unwrap_or_default();

            let option = self.get_option_from_token(&option_token);
            if option.is_none() && !self.is_cursor_free() {
                self.completion_type = Self::TYPE_OPTION_NAME.to_string();
                self.completion_value = relevant_token;

                return Ok(());
            }

            if let Some(option) = &option
                && option.accept_value()
            {
                self.completion_type = Self::TYPE_OPTION_VALUE.to_string();
                self.completion_name = Some(option.get_name().to_string());
                self.completion_value = if !option_value.is_empty() {
                    option_value
                } else if !shirabe_php_shim::str_starts_with(&option_token, "--") {
                    shirabe_php_shim::substr(&option_token, 2, None)
                } else {
                    String::new()
                };

                return Ok(());
            }
        }

        let previous_token = self.tokens[(self.current_index - 1) as usize].clone();
        if "-" == &previous_token[0..1]
            && !shirabe_php_shim::trim(&previous_token, Some("-")).is_empty()
        {
            // check if previous option accepted a value
            let previous_option = self.get_option_from_token(&previous_token);
            if let Some(previous_option) = &previous_option
                && previous_option.accept_value()
            {
                self.completion_type = Self::TYPE_OPTION_VALUE.to_string();
                self.completion_name = Some(previous_option.get_name().to_string());
                self.completion_value = relevant_token;

                return Ok(());
            }
        }

        // complete argument value
        self.completion_type = Self::TYPE_ARGUMENT_VALUE.to_string();

        let mut argument_name: Option<String> = None;
        let argument_names: Vec<String> = self
            .inner
            .inner
            .definition
            .get_arguments()
            .keys()
            .cloned()
            .collect();
        for current_argument_name in argument_names {
            // PHP's foreach assigns the key variable before the body runs, so on break
            // $argumentName still names the first argument that has no bound value.
            argument_name = Some(current_argument_name.clone());
            if !self
                .inner
                .inner
                .arguments
                .contains_key(&current_argument_name)
            {
                break;
            }

            let argument_value = self.inner.inner.arguments[&current_argument_name].clone();
            self.completion_name = Some(current_argument_name.clone());
            if let PhpMixed::List(argument_value) = &argument_value {
                self.completion_value = argument_value
                    .last()
                    .map(|v| v.to_string())
                    .unwrap_or_default();
            } else {
                self.completion_value = argument_value.to_string();
            }
        }

        if self.current_index >= self.tokens.len() as i64 {
            let argument_name = argument_name.unwrap_or_default();
            if !self.inner.inner.arguments.contains_key(&argument_name)
                || self
                    .inner
                    .inner
                    .definition
                    .get_argument(&PhpMixed::String(argument_name.clone()))
                    .unwrap()
                    .is_array()
            {
                self.completion_name = Some(argument_name);
                self.completion_value = String::new();
            } else {
                // we've reached the end
                self.completion_type = Self::TYPE_NONE.to_string();
                self.completion_name = None;
                self.completion_value = String::new();
            }
        }

        Ok(())
    }

    /// Returns the type of completion required.
    ///
    /// TYPE_ARGUMENT_VALUE when completing the value of an input argument
    /// TYPE_OPTION_VALUE   when completing the value of an input option
    /// TYPE_OPTION_NAME    when completing the name of an input option
    /// TYPE_NONE           when nothing should be completed
    pub fn get_completion_type(&self) -> String {
        self.completion_type.clone()
    }

    /// The name of the input option or argument when completing a value.
    ///
    /// Returns null when completing an option name.
    pub fn get_completion_name(&self) -> Option<String> {
        self.completion_name.clone()
    }

    /// The value already typed by the user (or empty string).
    pub fn get_completion_value(&self) -> String {
        self.completion_value.clone()
    }

    pub fn must_suggest_option_values_for(&self, option_name: &str) -> bool {
        Self::TYPE_OPTION_VALUE == self.get_completion_type()
            && Some(option_name.to_string()) == self.get_completion_name()
    }

    pub fn must_suggest_argument_values_for(&self, argument_name: &str) -> bool {
        Self::TYPE_ARGUMENT_VALUE == self.get_completion_type()
            && Some(argument_name.to_string()) == self.get_completion_name()
    }

    pub fn get_first_argument(&self) -> Option<String> {
        self.inner.get_first_argument()
    }

    /// PHP `CompletionInput::parseToken` (called back from `ArgvInput::base_bind`). Takes the
    /// embedded ArgvInput instead of `&mut self` because the parse loop already holds the
    /// exclusive borrow of it.
    fn parse_token(inner: &mut ArgvInput, token: &str, parse_options: bool) -> bool {
        match inner.parse_token(token, parse_options) {
            Ok(value) => return value,
            Err(_e) => {
                // suppress errors, completed input is almost never valid
            }
        }

        parse_options
    }

    fn get_option_from_token(&self, option_token: &str) -> Option<std::rc::Rc<InputOption>> {
        let option_name = shirabe_php_shim::ltrim(option_token, Some("-"));
        if option_name.is_empty() {
            return None;
        }

        if "-"
            == option_token
                .chars()
                .nth(1)
                .map(|c| c.to_string())
                .unwrap_or_else(|| " ".to_string())
        {
            // long option name
            return if self.inner.inner.definition.has_option(&option_name) {
                self.inner.inner.definition.get_option(&option_name).ok()
            } else {
                None
            };
        }

        // short option name
        let first = &option_name[0..1];
        if self.inner.inner.definition.has_shortcut(first) {
            self.inner
                .inner
                .definition
                .get_option_for_shortcut(first)
                .ok()
        } else {
            None
        }
    }

    /// The token of the cursor, or the last token if the cursor is at the end of the input.
    fn get_relevant_token(&self) -> String {
        let index = if self.is_cursor_free() {
            self.current_index - 1
        } else {
            self.current_index
        };
        self.tokens[index as usize].clone()
    }

    /// Whether the cursor is "free" (i.e. at the end of the input preceded by a space).
    fn is_cursor_free(&self) -> bool {
        let nr_of_tokens = self.tokens.len() as i64;
        if self.current_index > nr_of_tokens {
            // LogicException: recoverable usage as a "convenient fatal error"; panic.
            panic!("Current index is invalid, it must be the number of input tokens or one more.");
        }

        self.current_index >= nr_of_tokens
    }
}

/// PHP: `CompletionInput extends ArgvInput` — the inherited `InputInterface` surface,
/// forwarded to the embedded `ArgvInput`. `bind` dispatches to the specialized
/// `CompletionInput::bind` above, matching PHP's virtual dispatch.
impl crate::input::input_interface::InputInterface for CompletionInput {
    fn dup(
        &self,
    ) -> std::rc::Rc<std::cell::RefCell<dyn crate::input::input_interface::InputInterface>> {
        std::rc::Rc::new(std::cell::RefCell::new(self.clone()))
    }

    fn get_first_argument(&self) -> Option<String> {
        self.inner.get_first_argument()
    }

    fn __to_string(&self) -> String {
        self.to_string()
    }

    fn has_parameter_option(&self, values: PhpMixed, only_params: bool) -> bool {
        crate::input::input_interface::InputInterface::has_parameter_option(
            &self.inner,
            values,
            only_params,
        )
    }

    fn get_parameter_option(
        &self,
        values: PhpMixed,
        default: PhpMixed,
        only_params: bool,
    ) -> PhpMixed {
        crate::input::input_interface::InputInterface::get_parameter_option(
            &self.inner,
            values,
            default,
            only_params,
        )
    }

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        CompletionInput::bind(self, definition)
    }

    fn validate(&mut self) -> anyhow::Result<()> {
        crate::input::input_interface::InputInterface::validate(&mut self.inner)
    }

    fn get_arguments(&self) -> indexmap::IndexMap<String, PhpMixed> {
        crate::input::input_interface::InputInterface::get_arguments(&self.inner)
    }

    fn get_argument(&self, name: &str) -> anyhow::Result<PhpMixed> {
        crate::input::input_interface::InputInterface::get_argument(&self.inner, name)
    }

    fn set_argument(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()> {
        crate::input::input_interface::InputInterface::set_argument(&mut self.inner, name, value)
    }

    fn has_argument(&self, name: &str) -> bool {
        crate::input::input_interface::InputInterface::has_argument(&self.inner, name)
    }

    fn get_options(&self) -> indexmap::IndexMap<String, PhpMixed> {
        crate::input::input_interface::InputInterface::get_options(&self.inner)
    }

    fn get_option(&self, name: &str) -> anyhow::Result<PhpMixed> {
        crate::input::input_interface::InputInterface::get_option(&self.inner, name)
    }

    fn set_option(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()> {
        crate::input::input_interface::InputInterface::set_option(&mut self.inner, name, value)
    }

    fn has_option(&self, name: &str) -> bool {
        crate::input::input_interface::InputInterface::has_option(&self.inner, name)
    }

    fn is_interactive(&self) -> bool {
        crate::input::input_interface::InputInterface::is_interactive(&self.inner)
    }

    fn set_interactive(&mut self, interactive: bool) {
        crate::input::input_interface::InputInterface::set_interactive(&mut self.inner, interactive)
    }

    fn as_streamable(
        &self,
    ) -> Option<&dyn crate::input::streamable_input_interface::StreamableInputInterface> {
        crate::input::input_interface::InputInterface::as_streamable(&self.inner)
    }

    fn as_streamable_mut(
        &mut self,
    ) -> Option<&mut dyn crate::input::streamable_input_interface::StreamableInputInterface> {
        crate::input::input_interface::InputInterface::as_streamable_mut(&mut self.inner)
    }
}

impl std::fmt::Display for CompletionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut str = String::new();
        let mut last_i: i64 = 0;
        for (i, token) in self.tokens.iter().enumerate() {
            let i = i as i64;
            last_i = i;
            str += token;

            if self.current_index == i {
                str += "|";
            }

            str += " ";
        }

        if self.current_index > last_i {
            str += "|";
        }

        write!(f, "{}", shirabe_php_shim::rtrim(&str, None))
    }
}
