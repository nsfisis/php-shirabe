//! ref: composer/vendor/symfony/console/Input/InputDefinition.php

use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
use crate::symfony::console::exception::logic_exception::LogicException;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_option::InputOption;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// A InputDefinition represents a set of valid command line arguments and options.
///
/// `InputArgument` and `InputOption` are stored behind `Rc` to model PHP's
/// shared object references; this also lets the definition be cloned cheaply
/// (PHP `bind` assigns the definition by reference).
#[derive(Debug, Clone)]
pub struct InputDefinition {
    arguments: IndexMap<String, std::rc::Rc<InputArgument>>,
    required_count: i64,
    last_array_argument: Option<std::rc::Rc<InputArgument>>,
    last_optional_argument: Option<std::rc::Rc<InputArgument>>,
    options: IndexMap<String, std::rc::Rc<InputOption>>,
    negations: IndexMap<String, String>,
    shortcuts: IndexMap<String, String>,
}

/// A definition entry is either an InputArgument or an InputOption.
#[derive(Debug)]
pub enum DefinitionItem {
    InputArgument(InputArgument),
    InputOption(InputOption),
}

impl InputDefinition {
    pub fn new(definition: Vec<DefinitionItem>) -> anyhow::Result<Self> {
        let mut input_definition = InputDefinition {
            arguments: IndexMap::new(),
            required_count: 0,
            last_array_argument: None,
            last_optional_argument: None,
            options: IndexMap::new(),
            negations: IndexMap::new(),
            shortcuts: IndexMap::new(),
        };
        input_definition.set_definition(definition)?;
        Ok(input_definition)
    }

    /// Builds an option-only definition that shares the given options by
    /// reference, mirroring `new InputDefinition($definition->getOptions())`.
    /// `InputOption` is not `Clone` and lives behind `Rc`, so the options are
    /// reused rather than reconstructed by value.
    pub fn from_options(options: Vec<std::rc::Rc<InputOption>>) -> anyhow::Result<Self> {
        let mut input_definition = InputDefinition {
            arguments: IndexMap::new(),
            required_count: 0,
            last_array_argument: None,
            last_optional_argument: None,
            options: IndexMap::new(),
            negations: IndexMap::new(),
            shortcuts: IndexMap::new(),
        };
        for option in options {
            input_definition.add_option_rc(option)?;
        }
        Ok(input_definition)
    }

    /// Sets the definition of the input.
    pub fn set_definition(&mut self, definition: Vec<DefinitionItem>) -> anyhow::Result<()> {
        let mut arguments = vec![];
        let mut options = vec![];
        for item in definition {
            match item {
                DefinitionItem::InputOption(option) => {
                    options.push(option);
                }
                DefinitionItem::InputArgument(argument) => {
                    arguments.push(argument);
                }
            }
        }

        self.set_arguments(arguments)?;
        self.set_options(options)?;

        Ok(())
    }

    /// Sets the InputArgument objects.
    pub fn set_arguments(&mut self, arguments: Vec<InputArgument>) -> anyhow::Result<()> {
        self.arguments = IndexMap::new();
        self.required_count = 0;
        self.last_optional_argument = None;
        self.last_array_argument = None;
        self.add_arguments(Some(arguments))?;
        Ok(())
    }

    /// Adds an array of InputArgument objects.
    pub fn add_arguments(&mut self, arguments: Option<Vec<InputArgument>>) -> anyhow::Result<()> {
        if let Some(arguments) = arguments {
            for argument in arguments {
                self.add_argument(argument)?;
            }
        }
        Ok(())
    }

    pub fn add_argument(&mut self, argument: InputArgument) -> anyhow::Result<()> {
        let argument = std::rc::Rc::new(argument);

        if self.arguments.contains_key(argument.get_name()) {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: format!(
                    "An argument with name \"{}\" already exists.",
                    argument.get_name(),
                ),
                code: 0,
            })
            .into());
        }

        if let Some(last_array_argument) = &self.last_array_argument {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: format!(
                    "Cannot add a required argument \"{}\" after an array argument \"{}\".",
                    argument.get_name(),
                    last_array_argument.get_name(),
                ),
                code: 0,
            })
            .into());
        }

        if argument.is_required()
            && let Some(last_optional_argument) = &self.last_optional_argument
        {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: format!(
                    "Cannot add a required argument \"{}\" after an optional one \"{}\".",
                    argument.get_name(),
                    last_optional_argument.get_name(),
                ),
                code: 0,
            })
            .into());
        }

        if argument.is_array() {
            self.last_array_argument = Some(std::rc::Rc::clone(&argument));
        }

        if argument.is_required() {
            self.required_count += 1;
        } else {
            self.last_optional_argument = Some(std::rc::Rc::clone(&argument));
        }

        self.arguments
            .insert(argument.get_name().to_string(), argument);

        Ok(())
    }

    /// Returns an InputArgument by name or by position.
    pub fn get_argument(&self, name: &PhpMixed) -> anyhow::Result<std::rc::Rc<InputArgument>> {
        if !self.has_argument(name) {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!("The \"{}\" argument does not exist.", name.clone()),
                    code: 0,
                })
                .into(),
            );
        }

        match name {
            PhpMixed::Int(index) => {
                let arguments: Vec<std::rc::Rc<InputArgument>> =
                    self.arguments.values().cloned().collect();
                Ok(std::rc::Rc::clone(&arguments[*index as usize]))
            }
            _ => {
                let key = shirabe_php_shim::php_to_string(name);
                Ok(std::rc::Rc::clone(&self.arguments[&key]))
            }
        }
    }

    /// Returns true if an InputArgument object exists by name or position.
    pub fn has_argument(&self, name: &PhpMixed) -> bool {
        match name {
            PhpMixed::Int(index) => {
                let arguments: Vec<std::rc::Rc<InputArgument>> =
                    self.arguments.values().cloned().collect();
                *index >= 0 && (*index as usize) < arguments.len()
            }
            _ => {
                let key = shirabe_php_shim::php_to_string(name);
                self.arguments.contains_key(&key)
            }
        }
    }

    /// Gets the array of InputArgument objects.
    pub fn get_arguments(&self) -> &IndexMap<String, std::rc::Rc<InputArgument>> {
        &self.arguments
    }

    /// Returns the number of InputArguments.
    pub fn get_argument_count(&self) -> i64 {
        if self.last_array_argument.is_some() {
            i64::MAX
        } else {
            self.arguments.len() as i64
        }
    }

    /// Returns the number of required InputArguments.
    pub fn get_argument_required_count(&self) -> i64 {
        self.required_count
    }

    pub fn get_argument_defaults(&self) -> IndexMap<String, PhpMixed> {
        let mut values = IndexMap::new();
        for argument in self.arguments.values() {
            values.insert(
                argument.get_name().to_string(),
                argument.get_default().clone(),
            );
        }

        values
    }

    /// Sets the InputOption objects.
    pub fn set_options(&mut self, options: Vec<InputOption>) -> anyhow::Result<()> {
        self.options = IndexMap::new();
        self.shortcuts = IndexMap::new();
        self.negations = IndexMap::new();
        self.add_options(options)?;
        Ok(())
    }

    /// Adds an array of InputOption objects.
    pub fn add_options(&mut self, options: Vec<InputOption>) -> anyhow::Result<()> {
        for option in options {
            self.add_option(option)?;
        }
        Ok(())
    }

    pub fn add_option(&mut self, option: InputOption) -> anyhow::Result<()> {
        self.add_option_rc(std::rc::Rc::new(option))
    }

    /// Adds an option that is already shared behind `Rc`, mirroring PHP passing
    /// `InputOption` objects by reference.
    pub fn add_option_rc(&mut self, option: std::rc::Rc<InputOption>) -> anyhow::Result<()> {
        if let Some(existing) = self.options.get(option.get_name())
            && !option.equals(existing)
        {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: format!("An option named \"{}\" already exists.", option.get_name()),
                code: 0,
            })
            .into());
        }
        if self.negations.contains_key(option.get_name()) {
            return Err(LogicException(shirabe_php_shim::LogicException {
                message: format!("An option named \"{}\" already exists.", option.get_name()),
                code: 0,
            })
            .into());
        }

        if let Some(shortcut) = option.get_shortcut() {
            for shortcut in shirabe_php_shim::explode("|", shortcut) {
                if let Some(existing_name) = self.shortcuts.get(&shortcut)
                    && !option.equals(&self.options[existing_name])
                {
                    return Err(LogicException(shirabe_php_shim::LogicException {
                        message: format!(
                            "An option with shortcut \"{}\" already exists.",
                            shortcut.clone(),
                        ),
                        code: 0,
                    })
                    .into());
                }
            }
        }

        self.options
            .insert(option.get_name().to_string(), std::rc::Rc::clone(&option));
        if let Some(shortcut) = option.get_shortcut() {
            for shortcut in shirabe_php_shim::explode("|", shortcut) {
                self.shortcuts
                    .insert(shortcut, option.get_name().to_string());
            }
        }

        if option.is_negatable() {
            let negated_name = format!("no-{}", option.get_name());
            if self.options.contains_key(&negated_name) {
                return Err(LogicException(shirabe_php_shim::LogicException {
                    message: format!(
                        "An option named \"{}\" already exists.",
                        negated_name.clone(),
                    ),
                    code: 0,
                })
                .into());
            }
            self.negations
                .insert(negated_name, option.get_name().to_string());
        }

        Ok(())
    }

    /// Returns an InputOption by name.
    pub fn get_option(&self, name: &str) -> anyhow::Result<std::rc::Rc<InputOption>> {
        if !self.has_option(name) {
            return Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!("The \"--{}\" option does not exist.", name),
                    code: 0,
                })
                .into(),
            );
        }

        Ok(std::rc::Rc::clone(&self.options[name]))
    }

    /// Returns true if an InputOption object exists by name.
    ///
    /// This method can't be used to check if the user included the option when
    /// executing the command (use getOption() instead).
    pub fn has_option(&self, name: &str) -> bool {
        self.options.contains_key(name)
    }

    /// Gets the array of InputOption objects.
    pub fn get_options(&self) -> &IndexMap<String, std::rc::Rc<InputOption>> {
        &self.options
    }

    /// Returns true if an InputOption object exists by shortcut.
    pub fn has_shortcut(&self, name: &str) -> bool {
        self.shortcuts.contains_key(name)
    }

    /// Returns true if an InputOption object exists by negated name.
    pub fn has_negation(&self, name: &str) -> bool {
        self.negations.contains_key(name)
    }

    /// Gets an InputOption by shortcut.
    pub fn get_option_for_shortcut(
        &self,
        shortcut: &str,
    ) -> anyhow::Result<std::rc::Rc<InputOption>> {
        self.get_option(&self.shortcut_to_name(shortcut)?)
    }

    pub fn get_option_defaults(&self) -> IndexMap<String, PhpMixed> {
        let mut values = IndexMap::new();
        for option in self.options.values() {
            values.insert(option.get_name().to_string(), option.get_default().clone());
        }

        values
    }

    /// Returns the InputOption name given a shortcut.
    pub fn shortcut_to_name(&self, shortcut: &str) -> anyhow::Result<String> {
        match self.shortcuts.get(shortcut) {
            None => Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!("The \"-{}\" option does not exist.", shortcut),
                    code: 0,
                })
                .into(),
            ),
            Some(name) => Ok(name.clone()),
        }
    }

    /// Returns the InputOption name given a negation.
    pub fn negation_to_name(&self, negation: &str) -> anyhow::Result<String> {
        match self.negations.get(negation) {
            None => Err(
                InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                    message: format!("The \"--{}\" option does not exist.", negation),
                    code: 0,
                })
                .into(),
            ),
            Some(name) => Ok(name.clone()),
        }
    }

    /// Gets the synopsis.
    pub fn get_synopsis(&self, short: bool) -> String {
        let mut elements: Vec<String> = vec![];

        if short && !self.get_options().is_empty() {
            elements.push("[options]".to_string());
        } else if !short {
            for option in self.get_options().values() {
                let mut value = String::new();
                if option.accept_value() {
                    value = format!(
                        " {}{}{}",
                        if option.is_value_optional() {
                            "[".to_string()
                        } else {
                            String::new()
                        },
                        shirabe_php_shim::strtoupper(option.get_name()),
                        if option.is_value_optional() {
                            "]".to_string()
                        } else {
                            String::new()
                        },
                    );
                }

                let shortcut = match option.get_shortcut() {
                    Some(shortcut) => {
                        format!("-{}|", shortcut)
                    }
                    None => String::new(),
                };
                let negation = if option.is_negatable() {
                    format!("|--no-{}", option.get_name())
                } else {
                    String::new()
                };
                elements.push(format!(
                    "[{}--{}{}{}]",
                    shortcut,
                    option.get_name(),
                    value,
                    negation,
                ));
            }
        }

        if !elements.is_empty() && !self.get_arguments().is_empty() {
            elements.push("[--]".to_string());
        }

        let mut tail = String::new();
        for argument in self.get_arguments().values() {
            let mut element = format!("<{}>", argument.get_name());
            if argument.is_array() {
                element.push_str("...");
            }

            if !argument.is_required() {
                element = format!("[{}", element);
                tail.push(']');
            }

            elements.push(element);
        }

        format!("{}{}", shirabe_php_shim::implode(" ", &elements), tail)
    }
}
