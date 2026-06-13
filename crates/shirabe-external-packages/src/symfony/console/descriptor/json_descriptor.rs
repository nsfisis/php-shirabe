use crate::composer::pcre::preg::Preg;
use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::descriptor::application_description::ApplicationDescription;
use crate::symfony::console::descriptor::descriptor::Descriptor;
use crate::symfony::console::descriptor::descriptor_interface::DescriptorInterface;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// JSON descriptor.
///
/// @internal
#[derive(Debug, Default)]
pub struct JsonDescriptor {
    output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
}

impl JsonDescriptor {
    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        self.write_data(self.get_input_argument_data(argument)?, &options)?;
        Ok(())
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        self.write_data(self.get_input_option_data(option, false)?, &options)?;
        if option.is_negatable() {
            self.write_data(self.get_input_option_data(option, true)?, &options)?;
        }
        Ok(())
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        self.write_data(self.get_input_definition_data(definition)?, &options)?;
        Ok(())
    }

    fn describe_command(
        &mut self,
        command: &mut dyn Command,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let short = matches!(options.get("short"), Some(PhpMixed::Bool(true)));
        self.write_data(self.get_command_data(command, short)?, &options)?;
        Ok(())
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let described_namespace = match options.get("namespace") {
            Some(PhpMixed::String(s)) => Some(s.clone()),
            _ => None,
        };
        let mut description =
            ApplicationDescription::new(application.clone(), described_namespace.clone(), true);
        let mut commands: Vec<Box<PhpMixed>> = vec![];

        let short = matches!(options.get("short"), Some(PhpMixed::Bool(true)));
        let command_list: Vec<_> = description
            .get_commands()
            .values()
            .map(|c| c.borrow().clone_box())
            .collect();
        for mut command in command_list {
            commands.push(Box::new(PhpMixed::Array(
                self.get_command_data(command.as_mut(), short)?
                    .into_iter()
                    .map(|(k, v)| (k, Box::new(v)))
                    .collect(),
            )));
        }

        let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
        if "UNKNOWN" != application.borrow().get_name() {
            let mut application_data: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
            application_data.insert(
                "name".to_string(),
                Box::new(PhpMixed::String(application.borrow().get_name())),
            );
            if "UNKNOWN" != application.borrow().get_version() {
                application_data.insert(
                    "version".to_string(),
                    Box::new(PhpMixed::String(application.borrow().get_version())),
                );
            }
            data.insert("application".to_string(), PhpMixed::Array(application_data));
        }

        data.insert("commands".to_string(), PhpMixed::List(commands));

        if let Some(described_namespace) = described_namespace {
            data.insert(
                "namespace".to_string(),
                PhpMixed::String(described_namespace),
            );
        } else {
            data.insert(
                "namespaces".to_string(),
                PhpMixed::List(
                    description
                        .get_namespaces()
                        .into_values()
                        .map(|ns| {
                            Box::new(PhpMixed::Array(
                                ns.into_iter().map(|(k, v)| (k, Box::new(v))).collect(),
                            ))
                        })
                        .collect(),
                ),
            );
        }

        self.write_data(data, &options)?;
        Ok(())
    }

    /// Writes data as json.
    fn write_data(
        &self,
        data: IndexMap<String, PhpMixed>,
        options: &IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let flags = match options.get("json_encoding") {
            Some(PhpMixed::Int(f)) => *f,
            _ => 0,
        };

        self.write(
            &shirabe_php_shim::json_encode_ex(
                &PhpMixed::Array(data.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
                flags,
            )
            .unwrap_or_default(),
            false,
        );
        Ok(())
    }

    fn get_input_argument_data(
        &self,
        argument: &InputArgument,
    ) -> anyhow::Result<IndexMap<String, PhpMixed>> {
        let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
        data.insert(
            "name".to_string(),
            PhpMixed::String(argument.get_name().to_string()),
        );
        data.insert(
            "is_required".to_string(),
            PhpMixed::Bool(argument.is_required()),
        );
        data.insert("is_array".to_string(), PhpMixed::Bool(argument.is_array()));
        data.insert(
            "description".to_string(),
            PhpMixed::String(Preg::replace(
                "/\\s*[\\r\\n]\\s*/",
                " ",
                argument.get_description(),
            )?),
        );
        data.insert(
            "default".to_string(),
            if matches!(argument.get_default(), PhpMixed::Float(f) if f.is_infinite() && *f > 0.0) {
                PhpMixed::String("INF".to_string())
            } else {
                argument.get_default().clone()
            },
        );
        Ok(data)
    }

    fn get_input_option_data(
        &self,
        option: &InputOption,
        negated: bool,
    ) -> anyhow::Result<IndexMap<String, PhpMixed>> {
        let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
        if negated {
            data.insert(
                "name".to_string(),
                PhpMixed::String(format!("--no-{}", option.get_name())),
            );
            data.insert("shortcut".to_string(), PhpMixed::String(String::new()));
            data.insert("accept_value".to_string(), PhpMixed::Bool(false));
            data.insert("is_value_required".to_string(), PhpMixed::Bool(false));
            data.insert("is_multiple".to_string(), PhpMixed::Bool(false));
            data.insert(
                "description".to_string(),
                PhpMixed::String(format!("Negate the \"--{}\" option", option.get_name())),
            );
            data.insert("default".to_string(), PhpMixed::Bool(false));
        } else {
            data.insert(
                "name".to_string(),
                PhpMixed::String(format!("--{}", option.get_name())),
            );
            data.insert(
                "shortcut".to_string(),
                PhpMixed::String(if let Some(shortcut) = option.get_shortcut() {
                    format!("-{}", shirabe_php_shim::str_replace("|", "|-", shortcut))
                } else {
                    String::new()
                }),
            );
            data.insert(
                "accept_value".to_string(),
                PhpMixed::Bool(option.accept_value()),
            );
            data.insert(
                "is_value_required".to_string(),
                PhpMixed::Bool(option.is_value_required()),
            );
            data.insert("is_multiple".to_string(), PhpMixed::Bool(option.is_array()));
            data.insert(
                "description".to_string(),
                PhpMixed::String(Preg::replace(
                    "/\\s*[\\r\\n]\\s*/",
                    " ",
                    option.get_description(),
                )?),
            );
            data.insert(
                "default".to_string(),
                if matches!(option.get_default(), PhpMixed::Float(f) if f.is_infinite() && *f > 0.0)
                {
                    PhpMixed::String("INF".to_string())
                } else {
                    option.get_default().clone()
                },
            );
        }
        Ok(data)
    }

    fn get_input_definition_data(
        &self,
        definition: &InputDefinition,
    ) -> anyhow::Result<IndexMap<String, PhpMixed>> {
        let mut input_arguments: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        for (name, argument) in definition.get_arguments() {
            input_arguments.insert(
                name.clone(),
                Box::new(PhpMixed::Array(
                    self.get_input_argument_data(argument)?
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                )),
            );
        }

        let mut input_options: IndexMap<String, Box<PhpMixed>> = IndexMap::new();
        for (name, option) in definition.get_options() {
            input_options.insert(
                name.clone(),
                Box::new(PhpMixed::Array(
                    self.get_input_option_data(option, false)?
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                )),
            );
            if option.is_negatable() {
                input_options.insert(
                    format!("no-{}", name),
                    Box::new(PhpMixed::Array(
                        self.get_input_option_data(option, true)?
                            .into_iter()
                            .map(|(k, v)| (k, Box::new(v)))
                            .collect(),
                    )),
                );
            }
        }

        let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
        data.insert("arguments".to_string(), PhpMixed::Array(input_arguments));
        data.insert("options".to_string(), PhpMixed::Array(input_options));
        Ok(data)
    }

    fn get_command_data(
        &self,
        command: &mut dyn Command,
        short: bool,
    ) -> anyhow::Result<IndexMap<String, PhpMixed>> {
        let mut data: IndexMap<String, PhpMixed> = IndexMap::new();
        data.insert(
            "name".to_string(),
            match command.get_name() {
                Some(name) => PhpMixed::String(name),
                None => PhpMixed::Null,
            },
        );
        data.insert(
            "description".to_string(),
            PhpMixed::String(command.get_description()),
        );

        if short {
            data.insert(
                "usage".to_string(),
                PhpMixed::List(
                    command
                        .get_aliases()
                        .into_iter()
                        .map(|a| Box::new(PhpMixed::String(a)))
                        .collect(),
                ),
            );
        } else {
            command.merge_application_definition(false);

            let mut usage = vec![Box::new(PhpMixed::String(command.get_synopsis(false)))];
            usage.extend(
                command
                    .get_usages()
                    .into_iter()
                    .map(|u| Box::new(PhpMixed::String(u))),
            );
            usage.extend(
                command
                    .get_aliases()
                    .into_iter()
                    .map(|a| Box::new(PhpMixed::String(a))),
            );
            data.insert("usage".to_string(), PhpMixed::List(usage));
            data.insert(
                "help".to_string(),
                PhpMixed::String(command.get_processed_help()),
            );
            data.insert(
                "definition".to_string(),
                PhpMixed::Array(
                    self.get_input_definition_data(&command.get_definition())?
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                ),
            );
        }

        data.insert("hidden".to_string(), PhpMixed::Bool(command.is_hidden()));

        Ok(data)
    }
}

impl DescriptorInterface for JsonDescriptor {
    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: PhpMixed,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        Descriptor::describe(self, output, object, options)
    }
}

impl Descriptor for JsonDescriptor {
    fn output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> {
        self.output.clone().unwrap()
    }

    fn set_output(&mut self, output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>) {
        self.output = Some(output);
    }

    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_input_argument(self, argument, options)
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_input_option(self, option, options)
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_input_definition(self, definition, options)
    }

    fn describe_command(
        &mut self,
        command: &mut dyn Command,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_command(self, command, options)
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_application(self, application, options)
    }
}
