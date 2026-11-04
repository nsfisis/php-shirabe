//! ref: composer/vendor/symfony/console/Descriptor/JsonDescriptor.php sha1:3d3638a68954b0dd9dd156a7e4413e3b91b03b88

use crate::application::Application;
use crate::command::Command;
use crate::descriptor::{
    ApplicationDescription, DescribableObject, Descriptor, DescriptorInterface, DescriptorOptions,
    NamespaceDescription,
};
use crate::input::{InputArgument, InputDefinition, InputOption, InputValue};
use crate::output::OutputInterface;
use bstr::BString;
use indexmap::IndexMap;
use serde::Serialize;
use shirabe_php_shim::{bconcat, json_bytes, json_encode_ex, php_regex, preg_replace, str_replace};

/// JSON descriptor.
#[derive(Debug, Default)]
pub struct JsonDescriptor {
    output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
}

/// The array `getInputArgumentData()` returns.
#[derive(Debug, Serialize)]
struct InputArgumentData {
    #[serde(serialize_with = "json_bytes::str")]
    name: BString,
    is_required: bool,
    is_array: bool,
    #[serde(serialize_with = "json_bytes::str")]
    description: BString,
    #[serde(serialize_with = "input_value")]
    default: InputValue,
}

/// The array `getInputOptionData()` returns.
#[derive(Debug, Serialize)]
struct InputOptionData {
    #[serde(serialize_with = "json_bytes::str")]
    name: BString,
    #[serde(serialize_with = "json_bytes::str")]
    shortcut: BString,
    accept_value: bool,
    is_value_required: bool,
    is_multiple: bool,
    #[serde(serialize_with = "json_bytes::str")]
    description: BString,
    #[serde(serialize_with = "input_value")]
    default: InputValue,
}

/// The array `getInputDefinitionData()` returns.
#[derive(Debug, Serialize)]
struct InputDefinitionData {
    #[serde(serialize_with = "json_bytes::keyed")]
    arguments: IndexMap<BString, InputArgumentData>,
    #[serde(serialize_with = "json_bytes::keyed")]
    options: IndexMap<BString, InputOptionData>,
}

/// The array `getCommandData()` returns. `help` and `definition` are present unless the data is
/// short.
#[derive(Debug, Serialize)]
struct CommandData {
    #[serde(serialize_with = "json_bytes::option_str")]
    name: Option<BString>,
    #[serde(serialize_with = "json_bytes::str")]
    description: BString,
    #[serde(serialize_with = "json_bytes::str_vec")]
    usage: Vec<BString>,
    #[serde(
        serialize_with = "json_bytes::option_str",
        skip_serializing_if = "Option::is_none"
    )]
    help: Option<BString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    definition: Option<InputDefinitionData>,
    hidden: bool,
}

/// `$data['application']` in `describeApplication()`.
#[derive(Debug, Serialize)]
struct ApplicationData {
    #[serde(serialize_with = "json_bytes::str")]
    name: BString,
    #[serde(
        serialize_with = "json_bytes::option_str",
        skip_serializing_if = "Option::is_none"
    )]
    version: Option<BString>,
}

/// The array `describeApplication()` writes. It holds `namespace` when a namespace is described
/// and `namespaces` otherwise.
#[derive(Debug, Serialize)]
struct ApplicationDescriptionData {
    #[serde(skip_serializing_if = "Option::is_none")]
    application: Option<ApplicationData>,
    commands: Vec<CommandData>,
    #[serde(
        serialize_with = "json_bytes::option_str",
        skip_serializing_if = "Option::is_none"
    )]
    namespace: Option<BString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    namespaces: Option<Vec<NamespaceDescription>>,
}

fn input_value<S: serde::Serializer>(value: &InputValue, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        InputValue::Null => serializer.serialize_none(),
        InputValue::Bool(b) => serializer.serialize_bool(*b),
        InputValue::String(s) => json_bytes::str(s, serializer),
        InputValue::Array(items) => json_bytes::str_vec(items, serializer),
    }
}

impl JsonDescriptor {
    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        self.write_data(&self.get_input_argument_data(argument), &options)?;
        Ok(())
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        self.write_data(&self.get_input_option_data(option, false), &options)?;
        if option.is_negatable() {
            self.write_data(&self.get_input_option_data(option, true), &options)?;
        }
        Ok(())
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        self.write_data(&self.get_input_definition_data(definition), &options)?;
        Ok(())
    }

    fn describe_command(
        &mut self,
        command: &dyn Command,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        self.write_data(&self.get_command_data(command, options.short), &options)?;
        Ok(())
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        let described_namespace = options.namespace.clone();
        let mut description =
            ApplicationDescription::new(application.clone(), described_namespace.clone(), true);
        let mut commands: Vec<CommandData> = vec![];

        for command in description.get_commands()?.values() {
            let command = command.borrow();
            commands.push(self.get_command_data(&*command, options.short));
        }

        let mut application_data = None;
        if "UNKNOWN" != application.borrow().get_name() {
            application_data = Some(ApplicationData {
                name: application.borrow().get_name(),
                version: if "UNKNOWN" != application.borrow().get_version() {
                    Some(application.borrow().get_version())
                } else {
                    None
                },
            });
        }

        let mut data = ApplicationDescriptionData {
            application: application_data,
            commands,
            namespace: None,
            namespaces: None,
        };

        if let Some(described_namespace) = described_namespace {
            data.namespace = Some(described_namespace);
        } else {
            data.namespaces = Some(description.get_namespaces()?.into_values().collect());
        }

        self.write_data(&data, &options)?;
        Ok(())
    }

    /// Writes data as json.
    fn write_data(&self, data: &impl Serialize, options: &DescriptorOptions) -> anyhow::Result<()> {
        let flags = options.json_encoding;

        self.write(json_encode_ex(data, flags).unwrap_or_default(), false);
        Ok(())
    }

    fn get_input_argument_data(&self, argument: &InputArgument) -> InputArgumentData {
        InputArgumentData {
            name: argument.get_name().into(),
            is_required: argument.is_required(),
            is_array: argument.is_array(),
            description: preg_replace(
                php_regex!("/\\s*[\\r\\n]\\s*/"),
                " ",
                argument.get_description(),
            ),
            // TODO(type-model): PHP compares the default against `INF` and serializes it as the
            // string "INF"; `InputValue` has no float variant to hold one.
            default: argument.get_default().clone(),
        }
    }

    fn get_input_option_data(&self, option: &InputOption, negated: bool) -> InputOptionData {
        if negated {
            InputOptionData {
                name: bconcat!("--no-", option.get_name()),
                shortcut: BString::default(),
                accept_value: false,
                is_value_required: false,
                is_multiple: false,
                description: bconcat!("Negate the \"--", option.get_name(), "\" option"),
                default: InputValue::Bool(false),
            }
        } else {
            InputOptionData {
                name: bconcat!("--", option.get_name()),
                shortcut: if let Some(shortcut) = option.get_shortcut() {
                    bconcat!("-", str_replace("|", "|-", shortcut))
                } else {
                    BString::default()
                },
                accept_value: option.accept_value(),
                is_value_required: option.is_value_required(),
                is_multiple: option.is_array(),
                description: preg_replace(
                    php_regex!("/\\s*[\\r\\n]\\s*/"),
                    " ",
                    option.get_description(),
                ),
                // TODO(type-model): PHP compares the default against `INF` and serializes it as
                // the string "INF"; `InputValue` has no float variant to hold one.
                default: option.get_default().clone(),
            }
        }
    }

    fn get_input_definition_data(&self, definition: &InputDefinition) -> InputDefinitionData {
        let mut input_arguments: IndexMap<BString, InputArgumentData> = IndexMap::new();
        for (name, argument) in definition.get_arguments() {
            input_arguments.insert(name.clone(), self.get_input_argument_data(argument));
        }

        let mut input_options: IndexMap<BString, InputOptionData> = IndexMap::new();
        for (name, option) in definition.get_options() {
            input_options.insert(name.clone(), self.get_input_option_data(option, false));
            if option.is_negatable() {
                input_options.insert(
                    bconcat!("no-", name),
                    self.get_input_option_data(option, true),
                );
            }
        }

        InputDefinitionData {
            arguments: input_arguments,
            options: input_options,
        }
    }

    fn get_command_data(&self, command: &dyn Command, short: bool) -> CommandData {
        let mut data = CommandData {
            name: command.get_name(),
            description: command.get_description(),
            usage: vec![],
            help: None,
            definition: None,
            hidden: false,
        };

        if short {
            data.usage = command.get_aliases();
        } else {
            command.merge_application_definition(false);

            let mut usage = vec![command.get_synopsis(false)];
            usage.extend(command.get_usages());
            usage.extend(command.get_aliases());
            data.usage = usage;
            data.help = Some(command.get_processed_help());
            data.definition = Some(self.get_input_definition_data(&command.get_definition()));
        }

        data.hidden = command.is_hidden();

        data
    }
}

impl DescriptorInterface for JsonDescriptor {
    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: DescriptorOptions,
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
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_input_argument(self, argument, options)
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_input_option(self, option, options)
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_input_definition(self, definition, options)
    }

    fn describe_command(
        &mut self,
        command: &dyn Command,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_command(self, command, options)
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        JsonDescriptor::describe_application(self, application, options)
    }
}
