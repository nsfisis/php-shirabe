//! ref: composer/vendor/symfony/console/Input/Input.php

use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::runtime_exception::RuntimeException;
use crate::symfony::console::input::input_definition::InputDefinition;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// Input is the base class for all concrete Input classes.
///
/// Three concrete classes are provided by default:
///
///  * `ArgvInput`: The input comes from the CLI arguments (argv)
///  * `StringInput`: The input is provided as a string
///  * `ArrayInput`: The input is provided as an array
#[derive(Debug, Clone)]
pub struct Input {
    pub(crate) definition: InputDefinition,
    pub(crate) stream: PhpMixed,
    pub(crate) options: IndexMap<String, PhpMixed>,
    pub(crate) arguments: IndexMap<String, PhpMixed>,
    pub(crate) interactive: bool,
}

impl Input {
    pub fn new(definition: Option<InputDefinition>) -> anyhow::Result<Self> {
        let mut input = Input {
            definition: InputDefinition::new(vec![])?,
            stream: PhpMixed::Null,
            options: IndexMap::new(),
            arguments: IndexMap::new(),
            interactive: true,
        };

        match definition {
            None => {
                input.definition = InputDefinition::new(vec![])?;
            }
            Some(definition) => {
                input.bind(&definition)?;
                input.validate()?;
            }
        }

        Ok(input)
    }

    pub fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()> {
        self.arguments = IndexMap::new();
        self.options = IndexMap::new();
        self.definition = definition.clone();

        self.parse()?;

        Ok(())
    }

    /// Processes command line arguments.
    ///
    /// This is abstract in PHP; concrete subclasses provide their own `parse`.
    /// Since `Input` is embedded via `inner` in the subclasses, the subclass
    /// drives the parsing instead.
    fn parse(&mut self) -> anyhow::Result<()> {
        unreachable!("Input::parse is abstract and overridden by subclasses")
    }

    pub fn validate(&mut self) -> anyhow::Result<()> {
        let definition = &self.definition;
        let given_arguments = &self.arguments;

        let missing_arguments: Vec<String> = shirabe_php_shim::array_filter(
            &shirabe_php_shim::array_keys(definition.get_arguments()),
            |argument: &String| {
                !given_arguments.contains_key(argument)
                    && definition
                        .get_argument(&PhpMixed::String(argument.clone()))
                        .map(|a| a.is_required())
                        .unwrap_or(false)
            },
        );

        if missing_arguments.len() > 0 {
            return Err(RuntimeException(shirabe_php_shim::RuntimeException {
                message: format!(
                    "Not enough arguments (missing: \"{}\").",
                    PhpMixed::String(shirabe_php_shim::implode(", ", &missing_arguments,)),
                ),
                code: 0,
            })
            .into());
        }

        Ok(())
    }

    pub fn is_interactive(&self) -> bool {
        self.interactive
    }

    pub fn set_interactive(&mut self, interactive: bool) {
        self.interactive = interactive;
    }

    pub fn get_arguments(&self) -> IndexMap<String, PhpMixed> {
        shirabe_php_shim::array_merge_map(
            self.definition.get_argument_defaults(),
            self.arguments.clone(),
        )
    }

    pub fn get_argument(&self, name: &str) -> anyhow::Result<PhpMixed> {
        if !self
            .definition
            .has_argument(&PhpMixed::String(name.to_string()))
        {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!(
                        "The \"{}\" argument does not exist.",
                        PhpMixed::String(name.to_string()),
                    ),
                    code: 0,
                })
                .into(),
            );
        }

        Ok(match self.arguments.get(name) {
            Some(value) => value.clone(),
            None => self
                .definition
                .get_argument(&PhpMixed::String(name.to_string()))?
                .get_default()
                .clone(),
        })
    }

    pub fn set_argument(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()> {
        if !self
            .definition
            .has_argument(&PhpMixed::String(name.to_string()))
        {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!(
                        "The \"{}\" argument does not exist.",
                        PhpMixed::String(name.to_string()),
                    ),
                    code: 0,
                })
                .into(),
            );
        }

        self.arguments.insert(name.to_string(), value);

        Ok(())
    }

    pub fn has_argument(&self, name: &str) -> bool {
        self.definition
            .has_argument(&PhpMixed::String(name.to_string()))
    }

    pub fn get_options(&self) -> IndexMap<String, PhpMixed> {
        shirabe_php_shim::array_merge_map(
            self.definition.get_option_defaults(),
            self.options.clone(),
        )
    }

    pub fn get_option(&self, name: &str) -> anyhow::Result<PhpMixed> {
        if self.definition.has_negation(name) {
            let value = self.get_option(&self.definition.negation_to_name(name)?)?;
            if matches!(value, PhpMixed::Null) {
                return Ok(value);
            }

            return Ok(PhpMixed::Bool(!value.as_bool().unwrap_or(false)));
        }

        if !self.definition.has_option(name) {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!(
                        "The \"{}\" option does not exist.",
                        PhpMixed::String(name.to_string()),
                    ),
                    code: 0,
                })
                .into(),
            );
        }

        Ok(if self.options.contains_key(name) {
            self.options[name].clone()
        } else {
            self.definition.get_option(name)?.get_default().clone()
        })
    }

    pub fn set_option(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()> {
        if self.definition.has_negation(name) {
            let negated = self.definition.negation_to_name(name)?;
            self.options
                .insert(negated, PhpMixed::Bool(!value.as_bool().unwrap_or(false)));

            return Ok(());
        } else if !self.definition.has_option(name) {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!(
                        "The \"{}\" option does not exist.",
                        PhpMixed::String(name.to_string()),
                    ),
                    code: 0,
                })
                .into(),
            );
        }

        self.options.insert(name.to_string(), value);

        Ok(())
    }

    pub fn has_option(&self, name: &str) -> bool {
        self.definition.has_option(name) || self.definition.has_negation(name)
    }

    /// Escapes a token through escapeshellarg if it contains unsafe chars.
    pub fn escape_token(&self, token: &str) -> String {
        let mut matches: Vec<Option<String>> = vec![];
        if shirabe_php_shim::preg_match("{^[\\w-]+$}", token, &mut matches) {
            token.to_string()
        } else {
            shirabe_php_shim::escapeshellarg(token)
        }
    }

    pub fn set_stream(&mut self, stream: PhpMixed) {
        self.stream = stream;
    }

    pub fn get_stream(&self) -> Option<PhpMixed> {
        match &self.stream {
            PhpMixed::Null => None,
            other => Some(other.clone()),
        }
    }
}
