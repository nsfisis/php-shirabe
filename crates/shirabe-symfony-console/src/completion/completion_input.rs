//! ref: composer/vendor/symfony/console/Completion/CompletionInput.php sha1:10f8872f338f08bf6c7c4bf8e03d9eddd2db542b

use crate::input::{
    ArgumentName, ArgvInput, InputDefinition, InputInterface, InputOption, InputValue,
    StreamableInputInterface,
};
use bstr::{BStr, BString, ByteSlice};
use shirabe_php_shim::{
    Stringable, explode_with_limit, ltrim2, php_regex, php_str, preg_match_all, rtrim, substr,
    trim2,
};

/// An input specialized for shell completion.
///
/// This input allows unfinished option names or values and exposes what kind of
/// completion is expected.
#[derive(Debug, Clone)]
pub struct CompletionInput {
    inner: ArgvInput,
    tokens: Vec<BString>,
    current_index: i64,
    completion_type: BString,
    completion_name: Option<BString>,
    completion_value: BString,
}

impl CompletionInput {
    pub const TYPE_ARGUMENT_VALUE: &'static BStr = php_str(b"argument_value");
    pub const TYPE_OPTION_VALUE: &'static BStr = php_str(b"option_value");
    pub const TYPE_OPTION_NAME: &'static BStr = php_str(b"option_name");
    pub const TYPE_NONE: &'static BStr = php_str(b"none");

    /// Converts a terminal string into tokens.
    ///
    /// This is required for shell completions without COMP_WORDS support.
    pub fn from_string(input_str: impl AsRef<[u8]>, current_index: i64) -> anyhow::Result<Self> {
        let tokens = preg_match_all(
            php_regex!("/(?<=^|\\s)(['\"]?)(.+?)(?<!\\\\)\\1(?=$|\\s)/"),
            input_str.as_ref(),
        );

        Self::from_tokens(
            tokens
                .map(|token| {
                    BString::from(
                        token
                            .get(0)
                            .expect("group 0 participates whenever the pattern matches"),
                    )
                })
                .collect(),
            current_index,
        )
    }

    /// Create an input based on an COMP_WORDS token list.
    ///
    /// `tokens` is the set of split tokens (e.g. COMP_WORDS or argv),
    /// `current_index` the index of the cursor (e.g. COMP_CWORD).
    pub fn from_tokens(tokens: Vec<BString>, current_index: i64) -> anyhow::Result<Self> {
        let mut input = Self {
            inner: ArgvInput::new(Some(tokens.clone()), None)?,
            tokens: vec![],
            current_index: 0,
            completion_type: BString::default(),
            completion_name: None,
            completion_value: BString::default(),
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
        if relevant_token[0..1] == *b"-" {
            // the current token is an input option: complete either option name or option value
            let parts = explode_with_limit("=", &relevant_token, 2);
            let option_token = parts.first().cloned().unwrap_or_default();
            let option_value = parts.get(1).cloned().unwrap_or_default();

            let option = self.get_option_from_token(option_token.as_ref());
            if option.is_none() && !self.is_cursor_free() {
                self.completion_type = Self::TYPE_OPTION_NAME.to_owned();
                self.completion_value = relevant_token;

                return Ok(());
            }

            if let Some(option) = &option
                && option.accept_value()
            {
                self.completion_type = Self::TYPE_OPTION_VALUE.to_owned();
                self.completion_name = Some(option.get_name().to_owned());
                self.completion_value = if !option_value.is_empty() {
                    option_value
                } else if !option_token.starts_with("--".as_bytes()) {
                    substr(&option_token, 2, None)
                } else {
                    BString::default()
                };

                return Ok(());
            }
        }

        let previous_token = self.tokens[(self.current_index - 1) as usize].clone();
        if previous_token[0..1] == *b"-" && !trim2(&previous_token, "-").is_empty() {
            // check if previous option accepted a value
            let previous_option = self.get_option_from_token(previous_token.as_ref());
            if let Some(previous_option) = &previous_option
                && previous_option.accept_value()
            {
                self.completion_type = Self::TYPE_OPTION_VALUE.to_owned();
                self.completion_name = Some(previous_option.get_name().to_owned());
                self.completion_value = relevant_token;

                return Ok(());
            }
        }

        // complete argument value
        self.completion_type = Self::TYPE_ARGUMENT_VALUE.to_owned();

        let mut argument_name: Option<BString> = None;
        let argument_names: Vec<BString> = self
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
                .contains_key(current_argument_name.as_bstr())
            {
                break;
            }

            let argument_value =
                self.inner.inner.arguments[current_argument_name.as_bstr()].clone();
            self.completion_name = Some(current_argument_name.clone());
            if let InputValue::Array(argument_value) = &argument_value {
                self.completion_value = argument_value.last().cloned().unwrap_or_default();
            } else {
                self.completion_value = argument_value.to_php_string();
            }
        }

        if self.current_index >= self.tokens.len() as i64 {
            let argument_name = argument_name.unwrap_or_default();
            if !self
                .inner
                .inner
                .arguments
                .contains_key(argument_name.as_bstr())
                || self
                    .inner
                    .inner
                    .definition
                    .get_argument(&ArgumentName::of(&argument_name))
                    .unwrap()
                    .is_array()
            {
                self.completion_name = Some(argument_name);
                self.completion_value = BString::default();
            } else {
                // we've reached the end
                self.completion_type = Self::TYPE_NONE.to_owned();
                self.completion_name = None;
                self.completion_value = BString::default();
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
    pub fn get_completion_type(&self) -> BString {
        self.completion_type.clone()
    }

    /// The name of the input option or argument when completing a value.
    ///
    /// Returns null when completing an option name.
    pub fn get_completion_name(&self) -> Option<BString> {
        self.completion_name.clone()
    }

    /// The value already typed by the user (or empty string).
    pub fn get_completion_value(&self) -> BString {
        self.completion_value.clone()
    }

    pub fn must_suggest_option_values_for(&self, option_name: impl AsRef<[u8]>) -> bool {
        Self::TYPE_OPTION_VALUE == self.get_completion_type()
            && Some(BString::from(option_name.as_ref())) == self.get_completion_name()
    }

    pub fn must_suggest_argument_values_for(&self, argument_name: impl AsRef<[u8]>) -> bool {
        Self::TYPE_ARGUMENT_VALUE == self.get_completion_type()
            && Some(BString::from(argument_name.as_ref())) == self.get_completion_name()
    }

    pub fn get_first_argument(&self) -> Option<BString> {
        self.inner.get_first_argument()
    }

    /// PHP `CompletionInput::parseToken` (called back from `ArgvInput::base_bind`).
    fn parse_token(inner: &mut ArgvInput, token: &BStr, parse_options: bool) -> bool {
        match inner.parse_token(token, parse_options) {
            Ok(value) => return value,
            Err(_e) => {
                // suppress errors, completed input is almost never valid
            }
        }

        parse_options
    }

    fn get_option_from_token(&self, option_token: &BStr) -> Option<std::rc::Rc<InputOption>> {
        let option_name = ltrim2(option_token, "-");
        if option_name.is_empty() {
            return None;
        }

        if option_token.get(1).copied().unwrap_or(b' ') == b'-' {
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
    fn get_relevant_token(&self) -> BString {
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

impl Stringable for CompletionInput {
    fn to_php_string(&self) -> BString {
        let mut str = BString::default();
        let mut last_i: i64 = 0;
        for (i, token) in self.tokens.iter().enumerate() {
            let i = i as i64;
            last_i = i;
            str.extend_from_slice(token);

            if self.current_index == i {
                str.push(b'|');
            }

            str.push(b' ');
        }

        if self.current_index > last_i {
            str.push(b'|');
        }

        rtrim(&str)
    }
}

/// PHP: `CompletionInput extends ArgvInput` — the inherited `InputInterface` surface,
/// forwarded to the embedded `ArgvInput`. `bind` dispatches to the specialized
/// `CompletionInput::bind` above, matching PHP's virtual dispatch.
impl InputInterface for CompletionInput {
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>> {
        std::rc::Rc::new(std::cell::RefCell::new(self.clone()))
    }

    fn get_first_argument(&self) -> Option<BString> {
        self.inner.get_first_argument()
    }

    fn has_parameter_option_bytes(&self, values: &[&BStr], only_params: bool) -> bool {
        InputInterface::has_parameter_option_bytes(&self.inner, values, only_params)
    }

    fn get_parameter_option_bytes(
        &self,
        values: &[&BStr],
        default: InputValue,
        only_params: bool,
    ) -> InputValue {
        InputInterface::get_parameter_option_bytes(&self.inner, values, default, only_params)
    }

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        CompletionInput::bind(self, definition)
    }

    fn validate(&mut self) -> anyhow::Result<()> {
        InputInterface::validate(&mut self.inner)
    }

    fn get_arguments(&self) -> indexmap::IndexMap<BString, InputValue> {
        InputInterface::get_arguments(&self.inner)
    }

    fn get_argument_bytes(&self, name: &BStr) -> anyhow::Result<InputValue> {
        InputInterface::get_argument_bytes(&self.inner, name)
    }

    fn set_argument_bytes(&mut self, name: &BStr, value: InputValue) -> anyhow::Result<()> {
        InputInterface::set_argument_bytes(&mut self.inner, name, value)
    }

    fn has_argument_bytes(&self, name: &BStr) -> bool {
        InputInterface::has_argument_bytes(&self.inner, name)
    }

    fn get_options(&self) -> indexmap::IndexMap<BString, InputValue> {
        InputInterface::get_options(&self.inner)
    }

    fn get_option_bytes(&self, name: &BStr) -> anyhow::Result<InputValue> {
        InputInterface::get_option_bytes(&self.inner, name)
    }

    fn set_option_bytes(&mut self, name: &BStr, value: InputValue) -> anyhow::Result<()> {
        InputInterface::set_option_bytes(&mut self.inner, name, value)
    }

    fn has_option_bytes(&self, name: &BStr) -> bool {
        InputInterface::has_option_bytes(&self.inner, name)
    }

    fn is_interactive(&self) -> bool {
        InputInterface::is_interactive(&self.inner)
    }

    fn set_interactive(&mut self, interactive: bool) {
        InputInterface::set_interactive(&mut self.inner, interactive)
    }

    fn as_streamable(&self) -> Option<&dyn StreamableInputInterface> {
        InputInterface::as_streamable(&self.inner)
    }

    fn as_streamable_mut(&mut self) -> Option<&mut dyn StreamableInputInterface> {
        InputInterface::as_streamable_mut(&mut self.inner)
    }
}
