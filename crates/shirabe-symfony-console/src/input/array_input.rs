//! ref: composer/vendor/symfony/console/Input/ArrayInput.php sha1:fd998331964d455cd04be3352a737dfdab13099c

use crate::exception::{InvalidArgumentException, InvalidOptionException};
use crate::input::{
    ArgumentName, Input, InputDefinition, InputInterface, InputValue, StreamableInputInterface,
};
use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{PhpResource, bconcat, implode};

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
                && param[0] == b'-'
            {
                continue;
            }

            return Some(value.clone());
        }

        None
    }

    pub fn has_parameter_option(&self, values: &[impl AsRef<[u8]>], only_params: bool) -> bool {
        for (k, v) in &self.parameters {
            // if (!\is_int($k)) { $v = $k; }
            let v = match k {
                ParameterName::Index(_) => v.as_string(),
                ParameterName::Name(k) => Some(k.as_bstr()),
            };

            if only_params && v.is_some_and(|v| v == "--") {
                return false;
            }

            if v.is_some_and(|v| values.iter().any(|value| v.as_bytes() == value.as_ref())) {
                return true;
            }
        }

        false
    }

    pub fn get_parameter_option(
        &self,
        values: &[impl AsRef<[u8]>],
        default: InputValue,
        only_params: bool,
    ) -> InputValue {
        for (k, v) in &self.parameters {
            // $onlyParams && ('--' === $k || (\is_int($k) && '--' === $v))
            if only_params {
                let k_is_double_dash = matches!(k, ParameterName::Name(k) if k == "--");
                let int_v_double_dash = matches!(k, ParameterName::Index(_))
                    && v.as_string().is_some_and(|v| v == "--");
                if k_is_double_dash || int_v_double_dash {
                    return default;
                }
            }

            match k {
                ParameterName::Index(_) => {
                    if v.as_string()
                        .is_some_and(|v| values.iter().any(|value| v.as_bytes() == value.as_ref()))
                    {
                        return InputValue::Bool(true);
                    }
                }
                ParameterName::Name(k) => {
                    if values.iter().any(|value| k.as_slice() == value.as_ref()) {
                        return v.clone();
                    }
                }
            }
        }

        default
    }

    fn parse(&mut self) -> anyhow::Result<()> {
        let parameters = self.parameters.clone();
        for (key, value) in parameters {
            // PHP compares the key as a string even when it is an int.
            let key_string = key.to_key();
            if key_string == "--" {
                return Ok(());
            }
            if let Some(name) = key_string.strip_prefix(b"--") {
                self.add_long_option(name.as_bstr(), value)?;
            } else if let Some(shortcut) = key_string.strip_prefix(b"-") {
                self.add_short_option(shortcut.as_bstr(), value)?;
            } else {
                self.add_argument(&key, value)?;
            }
        }

        Ok(())
    }

    /// Adds a short option value.
    fn add_short_option(&mut self, shortcut: &BStr, value: InputValue) -> anyhow::Result<()> {
        if !self.inner.definition.has_shortcut(shortcut) {
            return Err(InvalidOptionException::new(bconcat!(
                "The \"-",
                shortcut,
                "\" option does not exist."
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
    fn add_long_option(&mut self, name: &BStr, mut value: InputValue) -> anyhow::Result<()> {
        if !self.inner.definition.has_option(name) {
            if !self.inner.definition.has_negation(name) {
                return Err(InvalidOptionException::new(bconcat!(
                    "The \"--",
                    name,
                    "\" option does not exist."
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
                return Err(InvalidOptionException::new(bconcat!(
                    "The \"--",
                    name,
                    "\" option requires a value."
                ))
                .into());
            }

            if !option.is_value_optional() {
                value = InputValue::Bool(true);
            }
        }

        self.inner.options.insert(name.to_owned(), value);

        Ok(())
    }

    /// Adds an argument value.
    fn add_argument(&mut self, name: &ParameterName, value: InputValue) -> anyhow::Result<()> {
        let key = name.to_key();
        if !self.inner.definition.has_argument(&name.to_argument_name()) {
            return Err(InvalidArgumentException::new(bconcat!(
                "The \"",
                key,
                "\" argument does not exist."
            ))
            .into());
        }

        self.inner.arguments.insert(key, value);

        Ok(())
    }

    /// Returns a stringified representation of the args passed to the command.
    pub fn __to_string(&self) -> BString {
        let mut params: Vec<BString> = vec![];
        for (param, val) in &self.parameters {
            // $param && \is_string($param) && '-' === $param[0]
            if let ParameterName::Name(param) = param
                && !param.is_empty()
                && param[0] == b'-'
            {
                let glue = if param.get(1) == Some(&b'-') {
                    "="
                } else {
                    " "
                };
                if let InputValue::Array(list) = val {
                    for v in list {
                        params.push(bconcat!(
                            param,
                            if !v.is_empty() {
                                bconcat!(glue, self.inner.escape_token(v.as_ref()))
                            } else {
                                BString::default()
                            }
                        ));
                    }
                } else {
                    let val = val.to_php_string();
                    params.push(bconcat!(
                        param,
                        if !val.is_empty() {
                            bconcat!(glue, self.inner.escape_token(val.as_ref()))
                        } else {
                            BString::default()
                        }
                    ));
                }
            } else if let InputValue::Array(list) = val {
                let escaped: Vec<BString> = list
                    .iter()
                    .map(|v| self.inner.escape_token(v.as_ref()))
                    .collect();
                params.push(implode(" ", &escaped));
            } else {
                params.push(self.inner.escape_token(val.to_php_string().as_ref()));
            }
        }

        implode(" ", &params)
    }
}

impl InputInterface for ArrayInput {
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>> {
        std::rc::Rc::new(std::cell::RefCell::new(self.clone()))
    }

    fn get_first_argument(&self) -> Option<BString> {
        ArrayInput::get_first_argument(self).map(|v| v.to_php_string())
    }

    fn has_parameter_option_bytes(&self, values: &[&BStr], only_params: bool) -> bool {
        ArrayInput::has_parameter_option(self, values, only_params)
    }

    fn get_parameter_option_bytes(
        &self,
        values: &[&BStr],
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

    fn get_arguments(&self) -> IndexMap<BString, InputValue> {
        self.inner.get_arguments()
    }

    fn get_argument_bytes(&self, name: &BStr) -> anyhow::Result<InputValue> {
        self.inner.get_argument(name)
    }

    fn set_argument_bytes(&mut self, name: &BStr, value: InputValue) -> anyhow::Result<()> {
        self.inner.set_argument(name, value)
    }

    fn has_argument_bytes(&self, name: &BStr) -> bool {
        self.inner.has_argument(name)
    }

    fn get_options(&self) -> IndexMap<BString, InputValue> {
        self.inner.get_options()
    }

    fn get_option_bytes(&self, name: &BStr) -> anyhow::Result<InputValue> {
        self.inner.get_option(name)
    }

    fn set_option_bytes(&mut self, name: &BStr, value: InputValue) -> anyhow::Result<()> {
        self.inner.set_option(name, value)
    }

    fn has_option_bytes(&self, name: &BStr) -> bool {
        self.inner.has_option(name)
    }

    fn is_interactive(&self) -> bool {
        self.inner.is_interactive()
    }

    fn set_interactive(&mut self, interactive: bool) {
        self.inner.set_interactive(interactive)
    }

    fn __to_string(&self) -> BString {
        ArrayInput::__to_string(self)
    }

    fn as_streamable(&self) -> Option<&dyn StreamableInputInterface> {
        Some(self)
    }
}

impl StreamableInputInterface for ArrayInput {
    fn set_stream(&mut self, stream: PhpResource) {
        self.inner.set_stream(stream)
    }

    fn get_stream(&self) -> Option<PhpResource> {
        self.inner.get_stream()
    }
}

/// The `string|int` key of an [`ArrayInput`] parameter: a named argument, a named option
/// (`--foo` / `-f`), or the position of a bare token.
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterName {
    Name(BString),
    Index(i64),
}

impl ParameterName {
    pub fn of(name: impl AsRef<[u8]>) -> Self {
        Self::Name(BString::from(name.as_ref()))
    }

    /// The key as PHP's array holds it, an int compared as a string.
    fn to_key(&self) -> BString {
        match self {
            Self::Name(name) => name.clone(),
            Self::Index(index) => bconcat!(*index),
        }
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
            Self::Name(name) => write!(f, "{}", name.display_lossy()),
            Self::Index(index) => write!(f, "{}", index),
        }
    }
}
