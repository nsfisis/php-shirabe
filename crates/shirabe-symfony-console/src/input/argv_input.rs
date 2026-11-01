//! ref: composer/vendor/symfony/console/Input/ArgvInput.php

use crate::exception::RuntimeException;
use crate::input::{
    ArgumentName, Input, InputDefinition, InputInterface, InputValue, StreamableInputInterface,
};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    PhpResource, bconcat, implode, mb_detect_encoding, mb_substr, path_to_bytes, php_regex,
    preg_match, strlen, strpos, substr, utf8_text,
};

/// ArgvInput represents an input coming from the CLI arguments.
///
/// Usage:
///
/// ```php
/// $input = new ArgvInput();
/// ```
///
/// By default, the `$_SERVER['argv']` array is used for the input values.
///
/// This can be overridden by explicitly passing the input values in the constructor:
///
/// ```php
/// $input = new ArgvInput($_SERVER['argv']);
/// ```
///
/// If you pass it yourself, don't forget that the first element of the array
/// is the name of the running application.
///
/// When passing an argument to the constructor, be sure that it respects
/// the same rules as the argv one. It's almost always better to use the
/// `StringInput` when you want to provide your own input.
#[derive(Debug, Clone)]
pub struct ArgvInput {
    pub(crate) inner: Input,
    tokens: Vec<BString>,
    parsed: Vec<BString>,
}

impl ArgvInput {
    pub fn new(
        argv: Option<Vec<BString>>,
        definition: Option<InputDefinition>,
    ) -> anyhow::Result<Self> {
        // $argv = $argv ?? $_SERVER['argv'] ?? [];
        let mut argv = match argv {
            Some(argv) => argv,
            None => std::env::args_os()
                .map(|arg| BString::from(path_to_bytes(&arg)))
                .collect(),
        };

        // strip the application name
        if !argv.is_empty() {
            argv.remove(0);
        }

        let mut input = ArgvInput {
            inner: Input::new(None)?,
            tokens: argv,
            parsed: vec![],
        };

        // parent::__construct($definition)
        match definition {
            None => {}
            Some(definition) => {
                input.bind(&definition)?;
                input.inner.validate()?;
            }
        }

        Ok(input)
    }

    pub(crate) fn set_tokens(&mut self, tokens: Vec<BString>) {
        self.tokens = tokens;
    }

    pub fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        self.base_bind(definition, ArgvInput::parse_token)
    }

    /// Shared body of `bind` (PHP `Input::bind`). PHP's `$this->parse()` ends in
    /// `$this->parseToken()`, which late-binds to `CompletionInput::parseToken`; `parseToken`
    /// is protected and not part of any trait, so the concrete implementation is threaded in
    /// as a callback taking the embedded ArgvInput.
    pub(crate) fn base_bind(
        &mut self,
        definition: &InputDefinition,
        parse_token: impl FnMut(&mut ArgvInput, &BStr, bool) -> anyhow::Result<bool>,
    ) -> anyhow::Result<()> {
        self.inner.arguments = IndexMap::new();
        self.inner.options = IndexMap::new();
        self.inner.definition = definition.clone();

        self.base_parse(parse_token)?;

        Ok(())
    }

    /// Shared body of `parse`; see `base_bind` for why `parse_token` is a parameter.
    fn base_parse(
        &mut self,
        mut parse_token: impl FnMut(&mut ArgvInput, &BStr, bool) -> anyhow::Result<bool>,
    ) -> anyhow::Result<()> {
        let mut parse_options = true;
        self.parsed = self.tokens.clone();
        while !self.parsed.is_empty() {
            let token = self.parsed.remove(0);
            parse_options = parse_token(self, token.as_ref(), parse_options)?;
        }
        Ok(())
    }

    pub(crate) fn parse_token(
        &mut self,
        token: &BStr,
        parse_options: bool,
    ) -> anyhow::Result<bool> {
        if parse_options && token.is_empty() {
            self.parse_argument(token)?;
        } else if parse_options && token == "--" {
            return Ok(false);
        } else if parse_options && token.starts_with(b"--") {
            self.parse_long_option(token)?;
        } else if parse_options && token.first() == Some(&b'-') && token != "-" {
            self.parse_short_option(token)?;
        } else {
            self.parse_argument(token)?;
        }

        Ok(parse_options)
    }

    /// Parses a short option.
    fn parse_short_option(&mut self, token: &BStr) -> anyhow::Result<()> {
        let name = substr(token, 1, None);

        if strlen(&name) > 1 {
            let first = substr(&name, 0, Some(1));
            if self.has_shortcut(first.as_ref())
                && self
                    .inner
                    .definition
                    .get_option_for_shortcut(defined_name(first.as_ref()))?
                    .accept_value()
            {
                // an option with a value (with no space)
                self.add_short_option(first.as_ref(), InputValue::String(substr(&name, 1, None)))?;
            } else {
                self.parse_short_option_set(name.as_ref())?;
            }
        } else {
            self.add_short_option(name.as_ref(), InputValue::Null)?;
        }

        Ok(())
    }

    /// Parses a short option set.
    fn parse_short_option_set(&mut self, name: &BStr) -> anyhow::Result<()> {
        let len = strlen(name);
        let mut i = 0;
        while i < len {
            let name_i = substr(name, i, Some(1));
            if !self.has_shortcut(name_i.as_ref()) {
                let encoding = mb_detect_encoding(name, None, true);
                let bad = match encoding {
                    None => name_i,
                    Some(encoding) => mb_substr(name, i, Some(1), Some(encoding.as_bstr())),
                };
                return Err(RuntimeException::new(format!(
                    "The \"-{}\" option does not exist.",
                    bad.display_lossy()
                ))
                .into());
            }

            let option = self
                .inner
                .definition
                .get_option_for_shortcut(defined_name(name_i.as_ref()))?;
            if option.accept_value() {
                let value = if i == len - 1 {
                    InputValue::Null
                } else {
                    InputValue::String(substr(name, i + 1, None))
                };
                self.add_long_option(BStr::new(option.get_name()), value)?;

                break;
            } else {
                self.add_long_option(BStr::new(option.get_name()), InputValue::Null)?;
            }
            i += 1;
        }

        Ok(())
    }

    /// Parses a long option.
    fn parse_long_option(&mut self, token: &BStr) -> anyhow::Result<()> {
        let name = substr(token, 2, None);

        match strpos(&name, "=") {
            Some(pos) => {
                let pos = pos as i64;
                let value = substr(&name, pos + 1, None);
                if value.is_empty() {
                    self.parsed.insert(0, value.clone());
                }
                self.add_long_option(
                    substr(&name, 0, Some(pos)).as_ref(),
                    InputValue::String(value),
                )?;
            }
            None => {
                self.add_long_option(name.as_ref(), InputValue::Null)?;
            }
        }

        Ok(())
    }

    /// Parses an argument.
    fn parse_argument(&mut self, token: &BStr) -> anyhow::Result<()> {
        let c = self.inner.arguments.len() as i64;

        // if input is expecting another argument, add it
        if self
            .inner
            .definition
            .has_argument(&ArgumentName::Position(c))
        {
            let arg = self
                .inner
                .definition
                .get_argument(&ArgumentName::Position(c))?;
            let value = if arg.is_array() {
                InputValue::Array(vec![BString::from(token)])
            } else {
                InputValue::String(BString::from(token))
            };
            self.inner
                .arguments
                .insert(arg.get_name().to_string(), value);

        // if last argument isArray(), append token to last argument
        } else if self
            .inner
            .definition
            .has_argument(&ArgumentName::Position(c - 1))
            && self
                .inner
                .definition
                .get_argument(&ArgumentName::Position(c - 1))?
                .is_array()
        {
            let arg = self
                .inner
                .definition
                .get_argument(&ArgumentName::Position(c - 1))?;
            if let Some(InputValue::Array(list)) = self.inner.arguments.get_mut(arg.get_name()) {
                list.push(BString::from(token));
            }

        // unexpected argument
        } else {
            let mut all = self.inner.definition.get_arguments().clone();
            let mut symfony_command_name: Option<InputValue> = None;
            let first_key = all.keys().next().cloned();
            if let Some(key) = &first_key {
                let input_argument = &all[key];
                if input_argument.get_name() == "command" {
                    symfony_command_name = self.inner.arguments.get("command").cloned();
                    all.shift_remove(key);
                }
            }

            let message = if !all.is_empty() {
                let names: Vec<String> = all.keys().cloned().collect();
                match &symfony_command_name {
                    Some(symfony_command_name) if !symfony_command_name.is_null() => {
                        format!(
                            "Too many arguments to \"{}\" command, expected arguments \"{}\".",
                            symfony_command_name.to_php_string().display_lossy(),
                            names.join("\" \""),
                        )
                    }
                    _ => format!(
                        "Too many arguments, expected arguments \"{}\".",
                        names.join("\" \""),
                    ),
                }
            } else if symfony_command_name
                .as_ref()
                .map(|n| !n.is_null())
                .unwrap_or(false)
            {
                format!(
                    "No arguments expected for \"{}\" command, got \"{}\".",
                    symfony_command_name
                        .unwrap()
                        .to_php_string()
                        .display_lossy(),
                    token.display_lossy(),
                )
            } else {
                format!("No arguments expected, got \"{}\".", token.display_lossy())
            };

            return Err(RuntimeException::new(message).into());
        }

        Ok(())
    }

    /// Adds a short option value.
    fn add_short_option(&mut self, shortcut: &BStr, value: InputValue) -> anyhow::Result<()> {
        if !self.has_shortcut(shortcut) {
            return Err(RuntimeException::new(format!(
                "The \"-{}\" option does not exist.",
                shortcut.display_lossy()
            ))
            .into());
        }

        self.add_long_option(
            BStr::new(
                self.inner
                    .definition
                    .get_option_for_shortcut(defined_name(shortcut))?
                    .get_name(),
            ),
            value,
        )
    }

    /// `InputDefinition::hasShortcut()` for a name taken from the command line.
    fn has_shortcut(&self, name: &BStr) -> bool {
        utf8_text(name).is_some_and(|name| self.inner.definition.has_shortcut(name))
    }

    /// Adds a long option value.
    fn add_long_option(&mut self, name: &BStr, mut value: InputValue) -> anyhow::Result<()> {
        // Option names are declared in code, so a name that is not UTF-8 cannot be defined.
        let Some(name) = utf8_text(name) else {
            return Err(RuntimeException::new(format!(
                "The \"--{}\" option does not exist.",
                name.display_lossy()
            ))
            .into());
        };

        if !self.inner.definition.has_option(name) {
            if !self.inner.definition.has_negation(name) {
                return Err(RuntimeException::new(format!(
                    "The \"--{}\" option does not exist.",
                    name
                ))
                .into());
            }

            let option_name = self.inner.definition.negation_to_name(name)?;
            if !value.is_null() {
                return Err(RuntimeException::new(format!(
                    "The \"--{}\" option does not accept a value.",
                    name
                ))
                .into());
            }
            self.inner
                .options
                .insert(option_name, InputValue::Bool(false));

            return Ok(());
        }

        let option = self.inner.definition.get_option(name)?;

        if !value.is_null() && !option.accept_value() {
            return Err(RuntimeException::new(format!(
                "The \"--{}\" option does not accept a value.",
                name
            ))
            .into());
        }

        // in_array($value, ['', null], true)
        let value_is_empty_or_null =
            matches!(&value, InputValue::String(s) if s.is_empty()) || value.is_null();
        if value_is_empty_or_null && option.accept_value() && !self.parsed.is_empty() {
            // if option accepts an optional or mandatory argument
            // let's see if there is one provided
            let next = self.parsed.remove(0);
            // (isset($next[0]) && '-' !== $next[0]) || in_array($next, ['', null], true)
            let next_first = next.as_bytes().first().copied();
            if (next_first.is_some() && next_first != Some(b'-')) || next.is_empty() {
                value = InputValue::String(next);
            } else {
                self.parsed.insert(0, next);
            }
        }

        if value.is_null() {
            if option.is_value_required() {
                return Err(RuntimeException::new(format!(
                    "The \"--{}\" option requires a value.",
                    name
                ))
                .into());
            }

            if !option.is_array() && !option.is_value_optional() {
                value = InputValue::Bool(true);
            }
        }

        if option.is_array() {
            // TODO(type-model): PHP appends the value as it stands, so a
            // VALUE_OPTIONAL|VALUE_IS_ARRAY option given no value collects a null;
            // `InputValue::Array` only holds strings.
            let InputValue::String(value) = value else {
                panic!("an array option cannot hold {:?}", value)
            };
            match self.inner.options.get_mut(name) {
                Some(InputValue::Array(list)) => {
                    list.push(value);
                }
                _ => {
                    self.inner
                        .options
                        .insert(name.to_string(), InputValue::Array(vec![value]));
                }
            }
        } else {
            self.inner.options.insert(name.to_string(), value);
        }

        Ok(())
    }

    pub fn get_first_argument(&self) -> Option<BString> {
        let mut is_option = false;
        for (i, token) in self.tokens.iter().enumerate() {
            if !token.is_empty() && token[0] == b'-' {
                if token.contains(&b'=') || self.tokens.get(i + 1).is_none() {
                    continue;
                }

                // If it's a long option, consider that everything after "--" is the option name.
                // Otherwise, use the last char (if it's a short option set, only the last one can take a value with space separator)
                let mut name = if token.get(1) == Some(&b'-') {
                    substr(token, 2, None)
                } else {
                    substr(token, -1, None)
                };
                let has_value = |name: &BStr| {
                    utf8_text(name).is_some_and(|name| self.inner.options.contains_key(name))
                };
                if !has_value(name.as_ref()) && !self.has_shortcut(name.as_ref()) {
                    // noop
                } else {
                    if !has_value(name.as_ref())
                        && let Ok(resolved) = self
                            .inner
                            .definition
                            .shortcut_to_name(defined_name(name.as_ref()))
                    {
                        name = BString::from(resolved);
                    }
                    if let Some(option_value) = self.inner.options.get(defined_name(name.as_ref()))
                        && self.tokens.get(i + 1).map(|t| t.as_bstr()) == option_value.as_string()
                    {
                        is_option = true;
                    }
                }

                continue;
            }

            if is_option {
                is_option = false;
                continue;
            }

            return Some(token.clone());
        }

        None
    }

    pub fn has_parameter_option(&self, values: &[&str], only_params: bool) -> bool {
        for token in &self.tokens {
            if only_params && token == "--" {
                return false;
            }
            for value in values {
                // Options with values:
                //   For long options, test for '--option=' at beginning
                //   For short options, test for '-o' at beginning
                let leading = if value.starts_with("--") {
                    format!("{}=", value)
                } else {
                    value.to_string()
                };
                if token == value || (!leading.is_empty() && token.starts_with(leading.as_bytes()))
                {
                    return true;
                }
            }
        }

        false
    }

    pub fn get_parameter_option(
        &self,
        values: &[&str],
        default: InputValue,
        only_params: bool,
    ) -> InputValue {
        let mut tokens = self.tokens.clone();

        while !tokens.is_empty() {
            let token = tokens.remove(0);
            if only_params && token == "--" {
                return default;
            }

            for value in values {
                if token == *value {
                    return match tokens.first() {
                        Some(_) => InputValue::String(tokens.remove(0)),
                        None => InputValue::Null,
                    };
                }
                // Options with values:
                //   For long options, test for '--option=' at beginning
                //   For short options, test for '-o' at beginning
                let leading = if value.starts_with("--") {
                    format!("{}=", value)
                } else {
                    value.to_string()
                };
                if !leading.is_empty() && token.starts_with(leading.as_bytes()) {
                    return InputValue::String(substr(&token, strlen(&leading), None));
                }
            }
        }

        default
    }

    /// Returns a stringified representation of the args passed to the command.
    pub fn __to_string(&self) -> BString {
        let tokens: Vec<BString> = self
            .tokens
            .iter()
            .map(|token| {
                if let Some(r#match) = preg_match(php_regex!("{^(-[^=]+=)(.+)}"), token) {
                    return bconcat!(
                        r#match.get(1).unwrap_or_default(),
                        self.inner
                            .escape_token(r#match.get(2).unwrap_or_default().as_bstr())
                    );
                }

                if !token.is_empty() && token[0] != b'-' {
                    return self.inner.escape_token(token.as_ref());
                }

                token.clone()
            })
            .collect();

        implode(" ", &tokens)
    }
}

/// The name as a key of an `InputDefinition`, for a name the definition already reported as known.
fn defined_name(name: &BStr) -> &str {
    utf8_text(name).expect("defined option names are UTF-8")
}

impl InputInterface for ArgvInput {
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>> {
        std::rc::Rc::new(std::cell::RefCell::new(self.clone()))
    }

    fn get_first_argument(&self) -> Option<BString> {
        ArgvInput::get_first_argument(self)
    }

    fn has_parameter_option(&self, values: &[&str], only_params: bool) -> bool {
        ArgvInput::has_parameter_option(self, values, only_params)
    }

    fn get_parameter_option(
        &self,
        values: &[&str],
        default: InputValue,
        only_params: bool,
    ) -> InputValue {
        ArgvInput::get_parameter_option(self, values, default, only_params)
    }

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        ArgvInput::bind(self, definition)
    }

    fn validate(&mut self) -> anyhow::Result<()> {
        self.inner.validate()
    }

    fn get_arguments(&self) -> IndexMap<String, InputValue> {
        self.inner.get_arguments()
    }

    fn get_argument(&self, name: &str) -> anyhow::Result<InputValue> {
        self.inner.get_argument(name)
    }

    fn set_argument(&mut self, name: &str, value: InputValue) -> anyhow::Result<()> {
        self.inner.set_argument(name, value)
    }

    fn has_argument(&self, name: &str) -> bool {
        self.inner.has_argument(name)
    }

    fn get_options(&self) -> IndexMap<String, InputValue> {
        self.inner.get_options()
    }

    fn get_option(&self, name: &str) -> anyhow::Result<InputValue> {
        self.inner.get_option(name)
    }

    fn set_option(&mut self, name: &str, value: InputValue) -> anyhow::Result<()> {
        self.inner.set_option(name, value)
    }

    fn has_option(&self, name: &str) -> bool {
        self.inner.has_option(name)
    }

    fn is_interactive(&self) -> bool {
        self.inner.is_interactive()
    }

    fn set_interactive(&mut self, interactive: bool) {
        self.inner.set_interactive(interactive)
    }

    fn __to_string(&self) -> BString {
        ArgvInput::__to_string(self)
    }

    fn as_streamable(&self) -> Option<&dyn StreamableInputInterface> {
        Some(self)
    }

    fn as_streamable_mut(&mut self) -> Option<&mut dyn StreamableInputInterface> {
        Some(self)
    }
}

impl StreamableInputInterface for ArgvInput {
    fn set_stream(&mut self, stream: PhpResource) {
        self.inner.set_stream(stream)
    }

    fn get_stream(&self) -> Option<PhpResource> {
        self.inner.get_stream()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{DefinitionItem, InputArgument, InputOption};

    fn definition() -> InputDefinition {
        InputDefinition::new(vec![
            DefinitionItem::InputArgument(
                InputArgument::new(
                    "name".to_string(),
                    Some(InputArgument::OPTIONAL),
                    String::new(),
                    InputValue::Null,
                )
                .unwrap(),
            ),
            DefinitionItem::InputOption(
                InputOption::new(
                    "foo",
                    Some("f"),
                    Some(InputOption::VALUE_REQUIRED),
                    String::new(),
                    InputValue::Null,
                )
                .unwrap(),
            ),
        ])
        .unwrap()
    }

    fn argv(tokens: &[&[u8]]) -> Vec<BString> {
        let mut argv = vec![BString::from("cli")];
        argv.extend(tokens.iter().map(|token| BString::from(*token)));
        argv
    }

    #[test]
    fn values_that_are_not_utf8_keep_their_bytes() {
        let input =
            ArgvInput::new(Some(argv(&[b"--foo=\xff", b"\xfe"])), Some(definition())).unwrap();

        assert_eq!(
            input.inner.get_option("foo").unwrap(),
            InputValue::String(BString::from(b"\xff".as_slice()))
        );
        assert_eq!(
            input.inner.get_argument("name").unwrap(),
            InputValue::String(BString::from(b"\xfe".as_slice()))
        );
    }

    #[test]
    fn option_names_that_are_not_utf8_do_not_exist() {
        for token in [b"--\xff".as_slice(), b"-\xff".as_slice()] {
            let error = ArgvInput::new(Some(argv(&[token])), Some(definition())).unwrap_err();
            assert!(error.to_string().contains("option does not exist"));
        }
    }
}
