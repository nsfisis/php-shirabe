//! ref: composer/vendor/symfony/console/Input/ArrayInput.php

use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::invalid_option_exception::InvalidOptionException;
use crate::symfony::console::input::input::Input;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_interface::InputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// ArrayInput represents an input provided as an array.
///
/// Usage:
///
///     $input = new ArrayInput(['command' => 'foo:bar', 'foo' => 'bar', '--bar' => 'foobar']);
///
/// PHP arrays can mix integer and string keys; `parameters` preserves both the
/// key type (`PhpMixed::Int` / `PhpMixed::String`) and the insertion order.
#[derive(Debug, Clone)]
pub struct ArrayInput {
    pub(crate) inner: Input,
    parameters: Vec<(PhpMixed, PhpMixed)>,
}

impl ArrayInput {
    pub fn new(
        parameters: Vec<(PhpMixed, PhpMixed)>,
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

    pub fn get_first_argument(&self) -> Option<PhpMixed> {
        for (param, value) in &self.parameters {
            // $param && \is_string($param) && '-' === $param[0]
            if let PhpMixed::String(param) = param {
                if !param.is_empty() && param.as_bytes()[0] == b'-' {
                    continue;
                }
            }

            return Some(value.clone());
        }

        None
    }

    pub fn has_parameter_option(&self, values: PhpMixed, only_params: bool) -> bool {
        let values = to_array(values);

        for (k, v) in &self.parameters {
            // if (!\is_int($k)) { $v = $k; }
            let v: PhpMixed = match k {
                PhpMixed::Int(_) => v.clone(),
                _ => k.clone(),
            };

            if only_params && matches!(&v, PhpMixed::String(s) if s == "--") {
                return false;
            }

            if values.iter().any(|x| x == &v) {
                return true;
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

        for (k, v) in &self.parameters {
            // $onlyParams && ('--' === $k || (\is_int($k) && '--' === $v))
            if only_params {
                let k_is_double_dash = matches!(k, PhpMixed::String(s) if s == "--");
                let int_v_double_dash =
                    matches!(k, PhpMixed::Int(_)) && matches!(v, PhpMixed::String(s) if s == "--");
                if k_is_double_dash || int_v_double_dash {
                    return default;
                }
            }

            match k {
                PhpMixed::Int(_) => {
                    if values.iter().any(|x| x == v) {
                        return PhpMixed::Bool(true);
                    }
                }
                _ => {
                    if values.iter().any(|x| x == k) {
                        return v.clone();
                    }
                }
            }
        }

        default
    }

    /// Returns a stringified representation of the args passed to the command.
    pub fn to_string(&self) -> String {
        let mut params: Vec<String> = vec![];
        for (param, val) in &self.parameters {
            // $param && \is_string($param) && '-' === $param[0]
            let is_option_key =
                matches!(param, PhpMixed::String(s) if !s.is_empty() && s.as_bytes()[0] == b'-');
            if is_option_key {
                let param = param.as_string().unwrap();
                let glue = if param.as_bytes().get(1) == Some(&b'-') {
                    "="
                } else {
                    " "
                };
                if let PhpMixed::List(list) = val {
                    for v in list {
                        let v = shirabe_php_shim::php_to_string(v);
                        params.push(format!(
                            "{}{}",
                            param,
                            if v != "" {
                                format!("{}{}", glue, self.inner.escape_token(&v))
                            } else {
                                String::new()
                            }
                        ));
                    }
                } else {
                    let val = shirabe_php_shim::php_to_string(val);
                    params.push(format!(
                        "{}{}",
                        param,
                        if val != "" {
                            format!("{}{}", glue, self.inner.escape_token(&val))
                        } else {
                            String::new()
                        }
                    ));
                }
            } else if let PhpMixed::List(list) = val {
                let escaped: Vec<String> = list
                    .iter()
                    .map(|v| self.inner.escape_token(&shirabe_php_shim::php_to_string(v)))
                    .collect();
                params.push(shirabe_php_shim::implode(" ", &escaped));
            } else {
                params.push(
                    self.inner
                        .escape_token(&shirabe_php_shim::php_to_string(val)),
                );
            }
        }

        shirabe_php_shim::implode(" ", &params)
    }

    fn parse(&mut self) -> anyhow::Result<()> {
        // Clone to avoid borrowing self while mutating; PHP iterates over a copy semantically.
        let parameters = self.parameters.clone();
        for (key, value) in parameters {
            let key = shirabe_php_shim::php_to_string(&key);
            if key == "--" {
                return Ok(());
            }
            if shirabe_php_shim::str_starts_with(&key, "--") {
                self.add_long_option(&shirabe_php_shim::substr(&key, 2, None), value)?;
            } else if shirabe_php_shim::str_starts_with(&key, "-") {
                self.add_short_option(&shirabe_php_shim::substr(&key, 1, None), value)?;
            } else {
                self.add_argument(&PhpMixed::String(key), value)?;
            }
        }

        Ok(())
    }

    /// Adds a short option value.
    fn add_short_option(&mut self, shortcut: &str, value: PhpMixed) -> anyhow::Result<()> {
        if !self.inner.definition.has_shortcut(shortcut) {
            return Err(InvalidOptionException(InvalidArgumentException(
                shirabe_php_shim::InvalidArgumentException {
                    message: format!(
                        "The \"-{}\" option does not exist.",
                        PhpMixed::String(shortcut.to_string()),
                    ),
                    code: 0,
                },
            ))
            .into());
        }

        self.add_long_option(
            &self
                .inner
                .definition
                .get_option_for_shortcut(shortcut)?
                .get_name()
                .to_string(),
            value,
        )
    }

    /// Adds a long option value.
    fn add_long_option(&mut self, name: &str, mut value: PhpMixed) -> anyhow::Result<()> {
        if !self.inner.definition.has_option(name) {
            if !self.inner.definition.has_negation(name) {
                return Err(InvalidOptionException(InvalidArgumentException(
                    shirabe_php_shim::InvalidArgumentException {
                        message: format!(
                            "The \"--{}\" option does not exist.",
                            PhpMixed::String(name.to_string()),
                        ),
                        code: 0,
                    },
                ))
                .into());
            }

            let option_name = self.inner.definition.negation_to_name(name)?;
            self.inner
                .options
                .insert(option_name, PhpMixed::Bool(false));

            return Ok(());
        }

        let option = self.inner.definition.get_option(name)?;

        if matches!(value, PhpMixed::Null) {
            if option.is_value_required() {
                return Err(InvalidOptionException(InvalidArgumentException(
                    shirabe_php_shim::InvalidArgumentException {
                        message: format!(
                            "The \"--{}\" option requires a value.",
                            PhpMixed::String(name.to_string()),
                        ),
                        code: 0,
                    },
                ))
                .into());
            }

            if !option.is_value_optional() {
                value = PhpMixed::Bool(true);
            }
        }

        self.inner.options.insert(name.to_string(), value);

        Ok(())
    }

    /// Adds an argument value.
    fn add_argument(&mut self, name: &PhpMixed, value: PhpMixed) -> anyhow::Result<()> {
        if !self.inner.definition.has_argument(name) {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!("The \"{}\" argument does not exist.", name.clone(),),
                    code: 0,
                })
                .into(),
            );
        }

        self.inner
            .arguments
            .insert(shirabe_php_shim::php_to_string(name), value);

        Ok(())
    }
}

impl InputInterface for ArrayInput {
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>> {
        std::rc::Rc::new(std::cell::RefCell::new(self.clone()))
    }

    fn get_first_argument(&self) -> Option<String> {
        ArrayInput::get_first_argument(self).map(|v| shirabe_php_shim::php_to_string(&v))
    }

    fn has_parameter_option(&self, values: PhpMixed, only_params: bool) -> bool {
        ArrayInput::has_parameter_option(self, values, only_params)
    }

    fn get_parameter_option(
        &self,
        values: PhpMixed,
        default: PhpMixed,
        only_params: bool,
    ) -> PhpMixed {
        ArrayInput::get_parameter_option(self, values, default, only_params)
    }

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        ArrayInput::bind(self, definition)
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

/// PHP `(array) $values` cast: a string becomes a single-element array.
fn to_array(values: PhpMixed) -> Vec<PhpMixed> {
    match values {
        PhpMixed::List(list) => list.into_iter().map(|v| *v).collect(),
        PhpMixed::Array(array) => array.into_iter().map(|(_, v)| *v).collect(),
        PhpMixed::Null => vec![],
        other => vec![other],
    }
}
