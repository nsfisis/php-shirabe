//! ref: composer/vendor/symfony/console/Input/ArrayInput.php

use crate::exception::InvalidArgumentException;
use crate::exception::InvalidOptionException;
use crate::input::ArgumentName;
use crate::input::Input;
use crate::input::InputDefinition;
use crate::input::InputInterface;
use crate::input::InputValue;
use crate::input::StreamableInputInterface;
use indexmap::IndexMap;

/// ArrayInput represents an input provided as an array.
///
/// Usage:
///
/// ```php
/// $input = new ArrayInput(['command' => 'foo:bar', 'foo' => 'bar', '--bar' => 'foobar']);
/// ```
///
/// PHP arrays can mix integer and string keys; `parameters` preserves both the
/// key type and the insertion order.
#[derive(Debug, Clone)]
pub struct ArrayInput {
    inner: Input,
    parameters: Vec<(ParameterName, InputValue)>,
}

impl ArrayInput {
    pub fn new(
        parameters: Vec<(ParameterName, InputValue)>,
        definition: Option<InputDefinition>,
    ) -> anyhow::Result<Self> {
        let mut array_input = ArrayInput {
            inner: Input::new(None)?,
            parameters,
        };

        // parent::__construct($definition)
        match definition {
            None => {}
            Some(definition) => {
                array_input.bind(&definition)?;
                array_input.inner.validate()?;
            }
        }

        Ok(array_input)
    }

    pub fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        self.inner.arguments = IndexMap::new();
        self.inner.options = IndexMap::new();
        self.inner.definition = definition.clone();

        self.parse()?;

        Ok(())
    }

    pub fn get_first_argument(&self) -> Option<InputValue> {
        for (param, value) in &self.parameters {
            // $param && \is_string($param) && '-' === $param[0]
            if let ParameterName::Name(param) = param
                && !param.is_empty()
                && param.as_bytes()[0] == b'-'
            {
                continue;
            }

            return Some(value.clone());
        }

        None
    }

    pub fn has_parameter_option(&self, values: &[&str], only_params: bool) -> bool {
        for (k, v) in &self.parameters {
            // if (!\is_int($k)) { $v = $k; }
            let v = match k {
                ParameterName::Index(_) => v.as_string(),
                ParameterName::Name(k) => Some(k.as_str()),
            };

            if only_params && v == Some("--") {
                return false;
            }

            if v.is_some_and(|v| values.contains(&v)) {
                return true;
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
        for (k, v) in &self.parameters {
            // $onlyParams && ('--' === $k || (\is_int($k) && '--' === $v))
            if only_params {
                let k_is_double_dash = matches!(k, ParameterName::Name(k) if k == "--");
                let int_v_double_dash =
                    matches!(k, ParameterName::Index(_)) && v.as_string() == Some("--");
                if k_is_double_dash || int_v_double_dash {
                    return default;
                }
            }

            match k {
                ParameterName::Index(_) => {
                    if v.as_string().is_some_and(|v| values.contains(&v)) {
                        return InputValue::Bool(true);
                    }
                }
                ParameterName::Name(k) => {
                    if values.contains(&k.as_str()) {
                        return v.clone();
                    }
                }
            }
        }

        default
    }

    fn parse(&mut self) -> anyhow::Result<()> {
        // Clone to avoid borrowing self while mutating; PHP iterates over a copy semantically.
        let parameters = self.parameters.clone();
        for (key, value) in parameters {
            // PHP compares the key as a string even when it is an int.
            let key_string = key.to_string();
            if key_string == "--" {
                return Ok(());
            }
            if key_string.starts_with("--") {
                self.add_long_option(&shirabe_php_shim::substr(&key_string, 2, None), value)?;
            } else if key_string.starts_with("-") {
                self.add_short_option(&shirabe_php_shim::substr(&key_string, 1, None), value)?;
            } else {
                self.add_argument(&key.to_argument_name(), value)?;
            }
        }

        Ok(())
    }

    /// Adds a short option value.
    fn add_short_option(&mut self, shortcut: &str, value: InputValue) -> anyhow::Result<()> {
        if !self.inner.definition.has_shortcut(shortcut) {
            return Err(InvalidOptionException::new(format!(
                "The \"-{}\" option does not exist.",
                shortcut
            ))
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
    fn add_long_option(&mut self, name: &str, mut value: InputValue) -> anyhow::Result<()> {
        if !self.inner.definition.has_option(name) {
            if !self.inner.definition.has_negation(name) {
                return Err(InvalidOptionException::new(format!(
                    "The \"--{}\" option does not exist.",
                    name
                ))
                .into());
            }

            let option_name = self.inner.definition.negation_to_name(name)?;
            self.inner
                .options
                .insert(option_name, InputValue::Bool(false));

            return Ok(());
        }

        let option = self.inner.definition.get_option(name)?;

        if value.is_null() {
            if option.is_value_required() {
                return Err(InvalidOptionException::new(format!(
                    "The \"--{}\" option requires a value.",
                    name
                ))
                .into());
            }

            if !option.is_value_optional() {
                value = InputValue::Bool(true);
            }
        }

        self.inner.options.insert(name.to_string(), value);

        Ok(())
    }

    /// Adds an argument value.
    fn add_argument(&mut self, name: &ArgumentName, value: InputValue) -> anyhow::Result<()> {
        if !self.inner.definition.has_argument(name) {
            return Err(InvalidArgumentException::new(format!(
                "The \"{}\" argument does not exist.",
                name
            ))
            .into());
        }

        self.inner.arguments.insert(name.to_string(), value);

        Ok(())
    }
}

/// Returns a stringified representation of the args passed to the command.
impl std::fmt::Display for ArrayInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut params: Vec<String> = vec![];
        for (param, val) in &self.parameters {
            // $param && \is_string($param) && '-' === $param[0]
            if let ParameterName::Name(param) = param
                && !param.is_empty()
                && param.as_bytes()[0] == b'-'
            {
                let glue = if param.as_bytes().get(1) == Some(&b'-') {
                    "="
                } else {
                    " "
                };
                if let InputValue::Array(list) = val {
                    for v in list {
                        params.push(format!(
                            "{}{}",
                            param,
                            if !v.is_empty() {
                                format!("{}{}", glue, self.inner.escape_token(v))
                            } else {
                                String::new()
                            }
                        ));
                    }
                } else {
                    let val = val.to_php_string();
                    params.push(format!(
                        "{}{}",
                        param,
                        if !val.is_empty() {
                            format!("{}{}", glue, self.inner.escape_token(&val))
                        } else {
                            String::new()
                        }
                    ));
                }
            } else if let InputValue::Array(list) = val {
                let escaped: Vec<String> =
                    list.iter().map(|v| self.inner.escape_token(v)).collect();
                params.push(shirabe_php_shim::implode(" ", &escaped));
            } else {
                params.push(self.inner.escape_token(&val.to_php_string()));
            }
        }

        write!(f, "{}", shirabe_php_shim::implode(" ", &params))
    }
}

impl InputInterface for ArrayInput {
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>> {
        std::rc::Rc::new(std::cell::RefCell::new(self.clone()))
    }

    fn get_first_argument(&self) -> Option<String> {
        ArrayInput::get_first_argument(self).map(|v| v.to_php_string())
    }

    fn has_parameter_option(&self, values: &[&str], only_params: bool) -> bool {
        ArrayInput::has_parameter_option(self, values, only_params)
    }

    fn get_parameter_option(
        &self,
        values: &[&str],
        default: InputValue,
        only_params: bool,
    ) -> InputValue {
        ArrayInput::get_parameter_option(self, values, default, only_params)
    }

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        ArrayInput::bind(self, definition)
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

    fn __to_string(&self) -> String {
        self.to_string()
    }

    fn as_streamable(&self) -> Option<&dyn StreamableInputInterface> {
        Some(self)
    }
}

impl StreamableInputInterface for ArrayInput {
    fn set_stream(&mut self, stream: shirabe_php_shim::PhpResource) {
        self.inner.set_stream(stream)
    }

    fn get_stream(&self) -> Option<shirabe_php_shim::PhpResource> {
        self.inner.get_stream()
    }
}

/// The `string|int` key of an [`ArrayInput`] parameter: a named argument, a named option
/// (`--foo` / `-f`), or the position of a bare token.
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterName {
    Name(String),
    Index(i64),
}

impl ParameterName {
    pub fn of(name: &str) -> Self {
        Self::Name(name.to_string())
    }

    fn to_argument_name(&self) -> ArgumentName {
        match self {
            Self::Name(name) => ArgumentName::Name(name.clone()),
            Self::Index(index) => ArgumentName::Position(*index),
        }
    }
}

impl std::fmt::Display for ParameterName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(name) => write!(f, "{}", name),
            Self::Index(index) => write!(f, "{}", index),
        }
    }
}
