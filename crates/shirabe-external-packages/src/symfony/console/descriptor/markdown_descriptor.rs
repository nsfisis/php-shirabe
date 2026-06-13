//! ref: composer/vendor/symfony/console/Descriptor/MarkdownDescriptor.php

use crate::composer::pcre::preg::Preg;
use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::descriptor::application_description::ApplicationDescription;
use crate::symfony::console::descriptor::descriptor::Descriptor;
use crate::symfony::console::descriptor::descriptor_interface::DescriptorInterface;
use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// Markdown descriptor.
///
/// @internal
#[derive(Debug, Default)]
pub struct MarkdownDescriptor {
    output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
}

impl MarkdownDescriptor {
    pub fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: PhpMixed,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let decorated = output.borrow().is_decorated();
        output.borrow().set_decorated(false);

        Descriptor::describe(self, output.clone(), object, options)?;

        output.borrow().set_decorated(decorated);
        Ok(())
    }

    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        _options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let name = if !argument.get_name().is_empty() {
            argument.get_name().to_string()
        } else {
            "<none>".to_string()
        };
        self.write(
            &format!(
                "#### `{}`\n\n{}* Is required: {}\n* Is array: {}\n* Default: `{}`",
                name,
                if !argument.get_description().is_empty() {
                    format!(
                        "{}\n\n",
                        Preg::replace("/\\s*[\\r\\n]\\s*/", "\n", argument.get_description())?
                    )
                } else {
                    String::new()
                },
                if argument.is_required() { "yes" } else { "no" },
                if argument.is_array() { "yes" } else { "no" },
                shirabe_php_shim::str_replace(
                    "\n",
                    "",
                    &shirabe_php_shim::var_export(argument.get_default(), true),
                ),
            ),
            true,
        );
        Ok(())
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        _options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let mut name = format!("--{}", option.get_name());
        if option.is_negatable() {
            name += &format!("|--no-{}", option.get_name());
        }
        if let Some(shortcut) = option.get_shortcut() {
            name += &format!("|-{}", shirabe_php_shim::str_replace("|", "|-", shortcut));
        }

        self.write(
            &format!(
                "#### `{}`\n\n{}* Accept value: {}\n* Is value required: {}\n* Is multiple: {}\n* Is negatable: {}\n* Default: `{}`",
                name,
                if !option.get_description().is_empty() {
                    format!(
                        "{}\n\n",
                        Preg::replace("/\\s*[\\r\\n]\\s*/", "\n", option.get_description())?
                    )
                } else {
                    String::new()
                },
                if option.accept_value() { "yes" } else { "no" },
                if option.is_value_required() { "yes" } else { "no" },
                if option.is_array() { "yes" } else { "no" },
                if option.is_negatable() { "yes" } else { "no" },
                shirabe_php_shim::str_replace(
                    "\n",
                    "",
                    &shirabe_php_shim::var_export(option.get_default(), true),
                ),
            ),
            true,
        );
        Ok(())
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        _options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let show_arguments = definition.get_arguments().len() > 0;
        if show_arguments {
            self.write("### Arguments", true);
            for argument in definition.get_arguments().values() {
                self.write("\n\n", true);
                // describeInputArgument returns null; the guarded write never runs.
                self.describe_input_argument(argument, IndexMap::new())?;
            }
        }

        if definition.get_options().len() > 0 {
            if show_arguments {
                self.write("\n\n", true);
            }

            self.write("### Options", true);
            for option in definition.get_options().values() {
                self.write("\n\n", true);
                // describeInputOption returns null; the guarded write never runs.
                self.describe_input_option(option, IndexMap::new())?;
            }
        }
        Ok(())
    }

    fn describe_command(
        &mut self,
        command: &mut dyn Command,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        if matches!(options.get("short"), Some(PhpMixed::Bool(true))) {
            self.write(
                &format!(
                    "`{}`\n{}\n\n{}### Usage\n\n{}",
                    command.get_name().unwrap_or_default(),
                    shirabe_php_shim::str_repeat(
                        "-",
                        (Helper::width(command.get_name().as_deref().unwrap_or("")) + 2) as usize
                    ),
                    if !command.get_description().is_empty() {
                        format!("{}\n\n", command.get_description())
                    } else {
                        String::new()
                    },
                    command
                        .get_aliases()
                        .iter()
                        .fold(String::new(), |carry, usage| {
                            format!("{}* `{}`\n", carry, usage)
                        }),
                ),
                true,
            );

            return Ok(());
        }

        command.merge_application_definition(false);

        let mut usages = vec![command.get_synopsis(false)];
        usages.extend(command.get_aliases());
        usages.extend(command.get_usages());
        self.write(
            &format!(
                "`{}`\n{}\n\n{}### Usage\n\n{}",
                command.get_name().unwrap_or_default(),
                shirabe_php_shim::str_repeat(
                    "-",
                    (Helper::width(command.get_name().as_deref().unwrap_or("")) + 2) as usize
                ),
                if !command.get_description().is_empty() {
                    format!("{}\n\n", command.get_description())
                } else {
                    String::new()
                },
                usages.iter().fold(String::new(), |carry, usage| {
                    format!("{}* `{}`\n", carry, usage)
                }),
            ),
            true,
        );

        let help = command.get_processed_help();
        if !help.is_empty() {
            self.write("\n", true);
            self.write(&help, true);
        }

        let definition = command.get_definition().clone();
        if !definition.get_options().is_empty() || !definition.get_arguments().is_empty() {
            self.write("\n\n", true);
            self.describe_input_definition(&definition, IndexMap::new())?;
        }
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
            ApplicationDescription::new(application.clone(), described_namespace, false);
        let title = self.get_application_title(&*application.borrow());

        self.write(
            &format!(
                "{}\n{}",
                title,
                shirabe_php_shim::str_repeat("=", Helper::width(&title) as usize)
            ),
            true,
        );

        for namespace in description.get_namespaces().values() {
            let namespace_id = match namespace.get("id") {
                Some(PhpMixed::String(s)) => s.clone(),
                _ => String::new(),
            };
            if ApplicationDescription::GLOBAL_NAMESPACE != namespace_id {
                self.write("\n\n", true);
                self.write(&format!("**{}:**", namespace_id), true);
            }

            self.write("\n\n", true);
            let command_names: Vec<String> = match namespace.get("commands") {
                Some(PhpMixed::List(names)) => names
                    .iter()
                    .filter_map(|n| n.as_string().map(|s| s.to_string()))
                    .collect(),
                _ => vec![],
            };
            self.write(
                &command_names
                    .iter()
                    .map(|command_name| {
                        Ok(format!(
                            "* [`{}`](#{})",
                            PhpMixed::String(command_name.clone()),
                            PhpMixed::String(shirabe_php_shim::str_replace(
                                ":",
                                "",
                                &description
                                    .get_command(command_name)?
                                    .borrow()
                                    .get_name()
                                    .unwrap_or_default(),
                            )),
                        ))
                    })
                    .collect::<anyhow::Result<Vec<String>>>()?
                    .join("\n"),
                true,
            );
        }

        let command_list: Vec<_> = description
            .get_commands()
            .values()
            .map(|c| c.borrow().clone_box())
            .collect();
        for mut command in command_list {
            self.write("\n\n", true);
            // describeCommand returns null; the guarded write never runs.
            self.describe_command(command.as_mut(), options.clone())?;
        }
        Ok(())
    }

    fn get_application_title(&self, application: &dyn Application) -> String {
        if "UNKNOWN" != application.get_name() {
            if "UNKNOWN" != application.get_version() {
                return format!(
                    "{} {}",
                    PhpMixed::String(application.get_name()),
                    PhpMixed::String(application.get_version()),
                );
            }

            return application.get_name();
        }

        "Console Tool".to_string()
    }
}

impl DescriptorInterface for MarkdownDescriptor {
    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: PhpMixed,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe(self, output, object, options)
    }
}

impl Descriptor for MarkdownDescriptor {
    fn output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> {
        self.output.clone().unwrap()
    }

    fn set_output(&mut self, output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>) {
        self.output = Some(output);
    }

    /// {@inheritdoc}
    fn write(&self, content: &str, decorated: bool) {
        // PHP overrides write() only to flip the default of $decorated to true;
        // it still delegates to parent::write.
        let _ = decorated;
        self.output().borrow().write(
            &[content.to_string()],
            false,
            if decorated {
                crate::symfony::console::output::output_interface::OUTPUT_NORMAL
            } else {
                crate::symfony::console::output::output_interface::OUTPUT_RAW
            },
        );
    }

    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_input_argument(self, argument, options)
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_input_option(self, option, options)
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_input_definition(self, definition, options)
    }

    fn describe_command(
        &mut self,
        command: &mut dyn Command,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_command(self, command, options)
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_application(self, application, options)
    }
}
