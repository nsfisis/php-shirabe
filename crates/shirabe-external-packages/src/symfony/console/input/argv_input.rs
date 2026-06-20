//! ref: composer/vendor/symfony/console/Input/ArgvInput.php

use crate::symfony::console::exception::runtime_exception::RuntimeException;
use crate::symfony::console::input::input::Input;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_interface::InputInterface;
use crate::symfony::console::input::streamable_input_interface::StreamableInputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

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
    tokens: Vec<String>,
    parsed: Vec<String>,
}

impl ArgvInput {
    pub fn new(
        argv: Option<Vec<String>>,
        definition: Option<InputDefinition>,
    ) -> anyhow::Result<Self> {
        // $argv = $argv ?? $_SERVER['argv'] ?? [];
        let mut argv = match argv {
            Some(argv) => argv,
            None => std::env::args().collect(),
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

    pub(crate) fn set_tokens(&mut self, tokens: Vec<String>) {
        self.tokens = tokens;
    }

    pub fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        self.inner.arguments = IndexMap::new();
        self.inner.options = IndexMap::new();
        self.inner.definition = definition.clone();

        self.parse()?;

        Ok(())
    }

    fn parse(&mut self) -> anyhow::Result<()> {
        let mut parse_options = true;
        self.parsed = self.tokens.clone();
        while !self.parsed.is_empty() {
            let token = self.parsed.remove(0);
            parse_options = self.parse_token(&token, parse_options)?;
        }
        Ok(())
    }

    pub(crate) fn parse_token(&mut self, token: &str, parse_options: bool) -> anyhow::Result<bool> {
        if parse_options && token.is_empty() {
            self.parse_argument(token)?;
        } else if parse_options && token == "--" {
            return Ok(false);
        } else if parse_options && shirabe_php_shim::str_starts_with(token, "--") {
            self.parse_long_option(token)?;
        } else if parse_options && token.as_bytes().first() == Some(&b'-') && token != "-" {
            self.parse_short_option(token)?;
        } else {
            self.parse_argument(token)?;
        }

        Ok(parse_options)
    }

    /// Parses a short option.
    fn parse_short_option(&mut self, token: &str) -> anyhow::Result<()> {
        let name = shirabe_php_shim::substr(token, 1, None);

        if shirabe_php_shim::strlen(&name) > 1 {
            let first = shirabe_php_shim::substr(&name, 0, Some(1));
            if self.inner.definition.has_shortcut(&first)
                && self
                    .inner
                    .definition
                    .get_option_for_shortcut(&first)?
                    .accept_value()
            {
                // an option with a value (with no space)
                self.add_short_option(
                    &first,
                    PhpMixed::String(shirabe_php_shim::substr(&name, 1, None)),
                )?;
            } else {
                self.parse_short_option_set(&name)?;
            }
        } else {
            self.add_short_option(&name, PhpMixed::Null)?;
        }

        Ok(())
    }

    /// Parses a short option set.
    fn parse_short_option_set(&mut self, name: &str) -> anyhow::Result<()> {
        let len = shirabe_php_shim::strlen(name);
        let mut i = 0;
        while i < len {
            let name_i = shirabe_php_shim::substr(name, i, Some(1));
            if !self.inner.definition.has_shortcut(&name_i) {
                let encoding = shirabe_php_shim::mb_detect_encoding(name, None, true);
                let bad = match encoding {
                    None => name_i.clone(),
                    Some(encoding) => {
                        shirabe_php_shim::mb_substr(name, i, Some(1), Some(&encoding))
                    }
                };
                return Err(RuntimeException(shirabe_php_shim::RuntimeException {
                    message: format!("The \"-{}\" option does not exist.", PhpMixed::String(bad),),
                    code: 0,
                })
                .into());
            }

            let option = self.inner.definition.get_option_for_shortcut(&name_i)?;
            if option.accept_value() {
                let value = if i == len - 1 {
                    PhpMixed::Null
                } else {
                    PhpMixed::String(shirabe_php_shim::substr(name, i + 1, None))
                };
                self.add_long_option(option.get_name(), value)?;

                break;
            } else {
                self.add_long_option(option.get_name(), PhpMixed::Null)?;
            }
            i += 1;
        }

        Ok(())
    }

    /// Parses a long option.
    fn parse_long_option(&mut self, token: &str) -> anyhow::Result<()> {
        let name = shirabe_php_shim::substr(token, 2, None);

        match shirabe_php_shim::strpos(&name, "=") {
            Some(pos) => {
                let pos = pos as i64;
                let value = shirabe_php_shim::substr(&name, pos + 1, None);
                if value.is_empty() {
                    self.parsed.insert(0, value.clone());
                }
                self.add_long_option(
                    &shirabe_php_shim::substr(&name, 0, Some(pos)),
                    PhpMixed::String(value),
                )?;
            }
            None => {
                self.add_long_option(&name, PhpMixed::Null)?;
            }
        }

        Ok(())
    }

    /// Parses an argument.
    fn parse_argument(&mut self, token: &str) -> anyhow::Result<()> {
        let c = self.inner.arguments.len() as i64;

        // if input is expecting another argument, add it
        if self.inner.definition.has_argument(&PhpMixed::Int(c)) {
            let arg = self.inner.definition.get_argument(&PhpMixed::Int(c))?;
            let value = if arg.is_array() {
                PhpMixed::List(vec![Box::new(PhpMixed::String(token.to_string()))])
            } else {
                PhpMixed::String(token.to_string())
            };
            self.inner
                .arguments
                .insert(arg.get_name().to_string(), value);

        // if last argument isArray(), append token to last argument
        } else if self.inner.definition.has_argument(&PhpMixed::Int(c - 1))
            && self
                .inner
                .definition
                .get_argument(&PhpMixed::Int(c - 1))?
                .is_array()
        {
            let arg = self.inner.definition.get_argument(&PhpMixed::Int(c - 1))?;
            if let Some(PhpMixed::List(list)) = self.inner.arguments.get_mut(arg.get_name()) {
                list.push(Box::new(PhpMixed::String(token.to_string())));
            }

        // unexpected argument
        } else {
            let mut all = self.inner.definition.get_arguments().clone();
            let mut symfony_command_name: Option<PhpMixed> = None;
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
                    Some(symfony_command_name)
                        if !matches!(symfony_command_name, PhpMixed::Null) =>
                    {
                        format!(
                            "Too many arguments to \"{}\" command, expected arguments \"{}\".",
                            symfony_command_name.clone(),
                            PhpMixed::String(shirabe_php_shim::implode("\" \"", &names)),
                        )
                    }
                    _ => format!(
                        "Too many arguments, expected arguments \"{}\".",
                        PhpMixed::String(shirabe_php_shim::implode("\" \"", &names)),
                    ),
                }
            } else if symfony_command_name
                .as_ref()
                .map(|n| !matches!(n, PhpMixed::Null))
                .unwrap_or(false)
            {
                format!(
                    "No arguments expected for \"{}\" command, got \"{}\".",
                    symfony_command_name.clone().unwrap(),
                    PhpMixed::String(token.to_string()),
                )
            } else {
                format!(
                    "No arguments expected, got \"{}\".",
                    PhpMixed::String(token.to_string()),
                )
            };

            return Err(
                RuntimeException(shirabe_php_shim::RuntimeException { message, code: 0 }).into(),
            );
        }

        Ok(())
    }

    /// Adds a short option value.
    fn add_short_option(&mut self, shortcut: &str, value: PhpMixed) -> anyhow::Result<()> {
        if !self.inner.definition.has_shortcut(shortcut) {
            return Err(RuntimeException(shirabe_php_shim::RuntimeException {
                message: format!(
                    "The \"-{}\" option does not exist.",
                    PhpMixed::String(shortcut.to_string()),
                ),
                code: 0,
            })
            .into());
        }

        self.add_long_option(
            self.inner
                .definition
                .get_option_for_shortcut(shortcut)?
                .get_name(),
            value,
        )
    }

    /// Adds a long option value.
    fn add_long_option(&mut self, name: &str, mut value: PhpMixed) -> anyhow::Result<()> {
        if !self.inner.definition.has_option(name) {
            if !self.inner.definition.has_negation(name) {
                return Err(RuntimeException(shirabe_php_shim::RuntimeException {
                    message: format!(
                        "The \"--{}\" option does not exist.",
                        PhpMixed::String(name.to_string()),
                    ),
                    code: 0,
                })
                .into());
            }

            let option_name = self.inner.definition.negation_to_name(name)?;
            if !matches!(value, PhpMixed::Null) {
                return Err(RuntimeException(shirabe_php_shim::RuntimeException {
                    message: format!(
                        "The \"--{}\" option does not accept a value.",
                        PhpMixed::String(name.to_string()),
                    ),
                    code: 0,
                })
                .into());
            }
            self.inner
                .options
                .insert(option_name, PhpMixed::Bool(false));

            return Ok(());
        }

        let option = self.inner.definition.get_option(name)?;

        if !matches!(value, PhpMixed::Null) && !option.accept_value() {
            return Err(RuntimeException(shirabe_php_shim::RuntimeException {
                message: format!(
                    "The \"--{}\" option does not accept a value.",
                    PhpMixed::String(name.to_string()),
                ),
                code: 0,
            })
            .into());
        }

        // in_array($value, ['', null], true)
        let value_is_empty_or_null = matches!(&value, PhpMixed::String(s) if s.is_empty())
            || matches!(value, PhpMixed::Null);
        if value_is_empty_or_null && option.accept_value() && !self.parsed.is_empty() {
            // if option accepts an optional or mandatory argument
            // let's see if there is one provided
            let next = self.parsed.remove(0);
            // (isset($next[0]) && '-' !== $next[0]) || in_array($next, ['', null], true)
            let next_first = next.as_bytes().first().copied();
            if (next_first.is_some() && next_first != Some(b'-')) || next.is_empty() {
                value = PhpMixed::String(next);
            } else {
                self.parsed.insert(0, next);
            }
        }

        if matches!(value, PhpMixed::Null) {
            if option.is_value_required() {
                return Err(RuntimeException(shirabe_php_shim::RuntimeException {
                    message: format!(
                        "The \"--{}\" option requires a value.",
                        PhpMixed::String(name.to_string()),
                    ),
                    code: 0,
                })
                .into());
            }

            if !option.is_array() && !option.is_value_optional() {
                value = PhpMixed::Bool(true);
            }
        }

        if option.is_array() {
            match self.inner.options.get_mut(name) {
                Some(PhpMixed::List(list)) => {
                    list.push(Box::new(value));
                }
                _ => {
                    self.inner
                        .options
                        .insert(name.to_string(), PhpMixed::List(vec![Box::new(value)]));
                }
            }
        } else {
            self.inner.options.insert(name.to_string(), value);
        }

        Ok(())
    }

    pub fn get_first_argument(&self) -> Option<String> {
        let mut is_option = false;
        for (i, token) in self.tokens.iter().enumerate() {
            if !token.is_empty() && token.as_bytes()[0] == b'-' {
                if shirabe_php_shim::str_contains(token, "=") || self.tokens.get(i + 1).is_none() {
                    continue;
                }

                // If it's a long option, consider that everything after "--" is the option name.
                // Otherwise, use the last char (if it's a short option set, only the last one can take a value with space separator)
                let mut name = if token.as_bytes().get(1) == Some(&b'-') {
                    shirabe_php_shim::substr(token, 2, None)
                } else {
                    shirabe_php_shim::substr(token, -1, None)
                };
                if !self.inner.options.contains_key(&name)
                    && !self.inner.definition.has_shortcut(&name)
                {
                    // noop
                } else {
                    if !self.inner.options.contains_key(&name)
                        && let Ok(resolved) = self.inner.definition.shortcut_to_name(&name)
                    {
                        name = resolved;
                    }
                    if let Some(option_value) = self.inner.options.get(&name)
                        && self.tokens.get(i + 1).map(|t| t.as_str()) == option_value.as_string()
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

    pub fn has_parameter_option(&self, values: PhpMixed, only_params: bool) -> bool {
        let values = to_array(values);

        for token in &self.tokens {
            if only_params && token == "--" {
                return false;
            }
            for value in &values {
                // Options with values:
                //   For long options, test for '--option=' at beginning
                //   For short options, test for '-o' at beginning
                let leading = if shirabe_php_shim::str_starts_with(value, "--") {
                    format!("{}=", value)
                } else {
                    value.clone()
                };
                if token == value
                    || (!leading.is_empty() && shirabe_php_shim::str_starts_with(token, &leading))
                {
                    return true;
                }
            }
        }

        false
    }

    pub fn get_parameter_option(
        &self,
        values: PhpMixed,
        default: PhpMixed,
        only_params: bool,
    ) -> PhpMixed {
        let values = to_array(values);
        let mut tokens = self.tokens.clone();

        while !tokens.is_empty() {
            let token = tokens.remove(0);
            if only_params && token == "--" {
                return default;
            }

            for value in &values {
                if &token == value {
                    return match tokens.first() {
                        Some(_) => PhpMixed::String(tokens.remove(0)),
                        None => PhpMixed::Null,
                    };
                }
                // Options with values:
                //   For long options, test for '--option=' at beginning
                //   For short options, test for '-o' at beginning
                let leading = if shirabe_php_shim::str_starts_with(value, "--") {
                    format!("{}=", value)
                } else {
                    value.clone()
                };
                if !leading.is_empty() && shirabe_php_shim::str_starts_with(&token, &leading) {
                    return PhpMixed::String(shirabe_php_shim::substr(
                        &token,
                        shirabe_php_shim::strlen(&leading),
                        None,
                    ));
                }
            }
        }

        default
    }
}

/// Returns a stringified representation of the args passed to the command.
impl std::fmt::Display for ArgvInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tokens: Vec<String> = self
            .tokens
            .iter()
            .map(|token| {
                let mut r#match: Vec<Option<String>> = Vec::new();
                if shirabe_php_shim::preg_match("{^(-[^=]+=)(.+)}", token, &mut r#match) {
                    return format!(
                        "{}{}",
                        r#match[1].as_deref().unwrap_or(""),
                        self.inner.escape_token(r#match[2].as_deref().unwrap_or(""))
                    );
                }

                if !token.is_empty() && token.as_bytes()[0] != b'-' {
                    return self.inner.escape_token(token);
                }

                token.clone()
            })
            .collect();

        write!(f, "{}", shirabe_php_shim::implode(" ", &tokens))
    }
}

impl InputInterface for ArgvInput {
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>> {
        std::rc::Rc::new(std::cell::RefCell::new(self.clone()))
    }

    fn get_first_argument(&self) -> Option<String> {
        ArgvInput::get_first_argument(self)
    }

    fn has_parameter_option(&self, values: PhpMixed, only_params: bool) -> bool {
        ArgvInput::has_parameter_option(self, values, only_params)
    }

    fn get_parameter_option(
        &self,
        values: PhpMixed,
        default: PhpMixed,
        only_params: bool,
    ) -> PhpMixed {
        ArgvInput::get_parameter_option(self, values, default, only_params)
    }

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        ArgvInput::bind(self, definition)
    }

    fn validate(&mut self) -> anyhow::Result<()> {
        self.inner.validate()
    }

    fn get_arguments(&self) -> IndexMap<String, PhpMixed> {
        self.inner.get_arguments()
    }

    fn get_argument(&self, name: &str) -> anyhow::Result<PhpMixed> {
        self.inner.get_argument(name)
    }

    fn set_argument(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()> {
        self.inner.set_argument(name, value)
    }

    fn has_argument(&self, name: &str) -> bool {
        self.inner.has_argument(name)
    }

    fn get_options(&self) -> IndexMap<String, PhpMixed> {
        self.inner.get_options()
    }

    fn get_option(&self, name: &str) -> anyhow::Result<PhpMixed> {
        self.inner.get_option(name)
    }

    fn set_option(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()> {
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
}

impl StreamableInputInterface for ArgvInput {
    fn set_stream(&mut self, stream: PhpMixed) {
        self.inner.set_stream(stream)
    }

    fn get_stream(&self) -> Option<PhpMixed> {
        self.inner.get_stream()
    }
}

/// PHP `(array) $values` cast: a string becomes a single-element array.
fn to_array(values: PhpMixed) -> Vec<String> {
    match values {
        PhpMixed::List(list) => list
            .into_iter()
            .map(|v| shirabe_php_shim::php_to_string(&v))
            .collect(),
        PhpMixed::Array(array) => array
            .into_iter()
            .map(|(_, v)| shirabe_php_shim::php_to_string(&v))
            .collect(),
        PhpMixed::Null => vec![],
        other => vec![shirabe_php_shim::php_to_string(&other)],
    }
}
