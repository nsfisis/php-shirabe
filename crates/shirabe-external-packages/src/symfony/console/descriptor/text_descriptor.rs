//! ref: composer/vendor/symfony/console/Descriptor/TextDescriptor.php

use crate::composer::pcre::preg::Preg;
use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::descriptor::application_description::ApplicationDescription;
use crate::symfony::console::descriptor::descriptor::Descriptor;
use crate::symfony::console::descriptor::descriptor_interface::{
    DescribableObject, DescriptorInterface,
};
use crate::symfony::console::formatter::output_formatter::OutputFormatter;
use crate::symfony::console::helper::helper::Helper;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// Text descriptor.
///
/// @internal
#[derive(Debug, Default)]
pub struct TextDescriptor {
    output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
}

/// Models PHP's `array<Command|string>` passed to `getColumnWidth`.
/// `PhpMixed` cannot hold console types, so a dedicated enum is used.
#[derive(Debug)]
enum CommandOrString {
    Command(std::rc::Rc<std::cell::RefCell<dyn Command>>),
    String(String),
}

impl TextDescriptor {
    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let default = if !argument.get_default().is_null()
            && (!matches!(
                argument.get_default(),
                PhpMixed::List(_) | PhpMixed::Array(_)
            ) || shirabe_php_shim::count(argument.get_default()) != 0)
        {
            format!(
                "<comment> [default: {}]</comment>",
                self.format_default_value(argument.get_default())?
            )
        } else {
            String::new()
        };

        let total_width = match options.get("total_width") {
            Some(PhpMixed::Int(w)) => *w,
            _ => Helper::width(argument.get_name()),
        };
        let spacing_width = total_width - shirabe_php_shim::strlen(argument.get_name());

        self.write_text(
            &format!(
                "  <info>{}</info>  {}{}{}",
                PhpMixed::String(argument.get_name().to_string()),
                PhpMixed::String(shirabe_php_shim::str_repeat(" ", spacing_width as usize)),
                // + 4 = 2 spaces before <info>, 2 spaces after </info>
                PhpMixed::String(Preg::replace(
                    "/\\s*[\\r\\n]\\s*/",
                    &format!(
                        "\n{}",
                        shirabe_php_shim::str_repeat(" ", (total_width + 4) as usize)
                    ),
                    argument.get_description(),
                )),
                PhpMixed::String(default),
            ),
            &options,
        );
        Ok(())
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let default = if option.accept_value()
            && !option.get_default().is_null()
            && (!matches!(option.get_default(), PhpMixed::List(_) | PhpMixed::Array(_))
                || shirabe_php_shim::count(option.get_default()) != 0)
        {
            format!(
                "<comment> [default: {}]</comment>",
                self.format_default_value(option.get_default())?
            )
        } else {
            String::new()
        };

        let mut value = String::new();
        if option.accept_value() {
            value = format!("={}", shirabe_php_shim::strtoupper(option.get_name()));

            if option.is_value_optional() {
                value = format!("[{}]", value);
            }
        }

        let total_width = match options.get("total_width") {
            Some(PhpMixed::Int(w)) => *w,
            _ => self.calculate_total_width_for_options(&[option]),
        };
        let synopsis = format!(
            "{}{}",
            if option.get_shortcut().is_some() {
                format!(
                    "-{}, ",
                    PhpMixed::String(option.get_shortcut().unwrap().to_string()),
                )
            } else {
                "    ".to_string()
            },
            if option.is_negatable() {
                format!(
                    "--{0}|--no-{0}",
                    PhpMixed::String(option.get_name().to_string()),
                )
            } else {
                format!(
                    "--{0}{1}",
                    PhpMixed::String(option.get_name().to_string()),
                    PhpMixed::String(value.clone()),
                )
            }
        );

        let spacing_width = total_width - Helper::width(&synopsis);

        self.write_text(
            &format!(
                "  <info>{}</info>  {}{}{}{}",
                PhpMixed::String(synopsis),
                PhpMixed::String(shirabe_php_shim::str_repeat(" ", spacing_width as usize)),
                // + 4 = 2 spaces before <info>, 2 spaces after </info>
                PhpMixed::String(Preg::replace(
                    "/\\s*[\\r\\n]\\s*/",
                    &format!(
                        "\n{}",
                        shirabe_php_shim::str_repeat(" ", (total_width + 4) as usize)
                    ),
                    option.get_description(),
                )),
                PhpMixed::String(default),
                PhpMixed::String(if option.is_array() {
                    "<comment> (multiple values allowed)</comment>".to_string()
                } else {
                    String::new()
                }),
            ),
            &options,
        );
        Ok(())
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        let mut total_width = self.calculate_total_width_for_options(
            &definition
                .get_options()
                .values()
                .map(|o| o.as_ref())
                .collect::<Vec<_>>(),
        );
        for argument in definition.get_arguments().values() {
            total_width = std::cmp::max(total_width, Helper::width(argument.get_name()));
        }

        if !definition.get_arguments().is_empty() {
            self.write_text("<comment>Arguments:</comment>", &options);
            self.write_text("\n", &IndexMap::new());
            for argument in definition.get_arguments().values() {
                let mut merged = options.clone();
                merged.insert("total_width".to_string(), PhpMixed::Int(total_width));
                self.describe_input_argument(argument, merged)?;
                self.write_text("\n", &IndexMap::new());
            }
        }

        if !definition.get_arguments().is_empty() && !definition.get_options().is_empty() {
            self.write_text("\n", &IndexMap::new());
        }

        if !definition.get_options().is_empty() {
            let mut later_options: Vec<&InputOption> = vec![];

            self.write_text("<comment>Options:</comment>", &options);
            for option in definition.get_options().values() {
                if shirabe_php_shim::strlen(option.get_shortcut().unwrap_or("")) > 1 {
                    later_options.push(option.as_ref());
                    continue;
                }
                self.write_text("\n", &IndexMap::new());
                let mut merged = options.clone();
                merged.insert("total_width".to_string(), PhpMixed::Int(total_width));
                self.describe_input_option(option, merged)?;
            }
            for option in later_options {
                self.write_text("\n", &IndexMap::new());
                let mut merged = options.clone();
                merged.insert("total_width".to_string(), PhpMixed::Int(total_width));
                self.describe_input_option(option, merged)?;
            }
        }
        Ok(())
    }

    fn describe_command(
        &mut self,
        command: &mut dyn Command,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        command.merge_application_definition(false);

        let description = command.get_description();
        if !description.is_empty() {
            self.write_text("<comment>Description:</comment>", &options);
            self.write_text("\n", &IndexMap::new());
            self.write_text(&format!("  {}", description), &IndexMap::new());
            self.write_text("\n\n", &IndexMap::new());
        }

        self.write_text("<comment>Usage:</comment>", &options);
        let mut usages = vec![command.get_synopsis(true)];
        usages.extend(command.get_aliases());
        usages.extend(command.get_usages());
        for usage in usages {
            self.write_text("\n", &IndexMap::new());
            self.write_text(&format!("  {}", OutputFormatter::escape(&usage)?), &options);
        }
        self.write_text("\n", &IndexMap::new());

        let definition = command.get_definition().clone();
        if !definition.get_options().is_empty() || !definition.get_arguments().is_empty() {
            self.write_text("\n", &IndexMap::new());
            self.describe_input_definition(&definition, options.clone())?;
            self.write_text("\n", &IndexMap::new());
        }

        let help = command.get_processed_help();
        if !help.is_empty() && help != description {
            self.write_text("\n", &IndexMap::new());
            self.write_text("<comment>Help:</comment>", &options);
            self.write_text("\n", &IndexMap::new());
            self.write_text(
                &format!("  {}", shirabe_php_shim::str_replace("\n", "\n  ", &help)),
                &options,
            );
            self.write_text("\n", &IndexMap::new());
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
            ApplicationDescription::new(application.clone(), described_namespace.clone(), false);

        if matches!(options.get("raw_text"), Some(v) if shirabe_php_shim::php_truthy(v)) {
            let width = self.get_column_width(
                &description
                    .get_commands()
                    .values()
                    .map(|c| CommandOrString::Command(c.clone()))
                    .collect::<Vec<_>>(),
            );

            let command_list: Vec<_> = description.get_commands().values().cloned().collect();
            for command in &command_list {
                let command = command.borrow();
                self.write_text(
                    &format!(
                        "{:<w$} {}",
                        PhpMixed::String(command.get_name().unwrap_or_default()),
                        PhpMixed::String(command.get_description()),
                        w = width as usize,
                    ),
                    &options,
                );
                self.write_text("\n", &IndexMap::new());
            }
        } else {
            let help = application.borrow().get_help();
            if !help.is_empty() {
                self.write_text(&format!("{}\n\n", help), &options);
            }

            self.write_text("<comment>Usage:</comment>\n", &options);
            self.write_text("  command [options] [arguments]\n\n", &options);

            // PHP: new InputDefinition($application->getDefinition()->getOptions()).
            // `InputOption` is not Clone and lives behind `Rc`, so the option-only
            // definition cannot be reconstructed by value yet.
            let definition: InputDefinition =
                todo!("new InputDefinition($application->getDefinition()->getOptions())");
            self.describe_input_definition(&definition, options.clone())?;

            self.write_text("\n", &IndexMap::new());
            self.write_text("\n", &IndexMap::new());

            let mut commands = description.get_commands().clone();
            let namespaces = description.get_namespaces();
            if described_namespace.is_some() && !namespaces.is_empty() {
                // make sure all alias commands are included when describing a specific namespace
                let described_namespace_info = namespaces.values().next().unwrap();
                if let Some(PhpMixed::List(names)) = described_namespace_info.get("commands") {
                    let names: Vec<String> = names
                        .iter()
                        .filter_map(|n| n.as_string().map(|s| s.to_string()))
                        .collect();
                    for name in names {
                        let command = description.get_command(&name)?;
                        commands.insert(name, command);
                    }
                }
            }

            // calculate max. width based on available commands per namespace
            let width = self.get_column_width(&{
                let command_keys: Vec<String> = commands.keys().cloned().collect();
                let mut merged: Vec<CommandOrString> = vec![];
                for namespace in namespaces.values() {
                    if let Some(PhpMixed::List(ns_commands)) = namespace.get("commands") {
                        for c in ns_commands {
                            if let PhpMixed::String(name) = c
                                && command_keys.contains(name)
                            {
                                merged.push(CommandOrString::String(name.clone()));
                            }
                        }
                    }
                }
                merged
            });

            if let Some(ref described_namespace) = described_namespace {
                self.write_text(
                    &format!(
                        "<comment>Available commands for the \"{}\" namespace:</comment>",
                        PhpMixed::String(described_namespace.clone()),
                    ),
                    &options,
                );
            } else {
                self.write_text("<comment>Available commands:</comment>", &options);
            }

            for namespace in namespaces.values() {
                let ns_commands: Vec<String> = match namespace.get("commands") {
                    Some(PhpMixed::List(names)) => names
                        .iter()
                        .filter_map(|n| match n {
                            PhpMixed::String(name) if commands.contains_key(name) => {
                                Some(name.clone())
                            }
                            _ => None,
                        })
                        .collect(),
                    _ => vec![],
                };

                if ns_commands.is_empty() {
                    continue;
                }

                let namespace_id = match namespace.get("id") {
                    Some(PhpMixed::String(s)) => s.clone(),
                    _ => String::new(),
                };

                if described_namespace.is_none()
                    && ApplicationDescription::GLOBAL_NAMESPACE != namespace_id
                {
                    self.write_text("\n", &IndexMap::new());
                    self.write_text(&format!(" <comment>{}</comment>", namespace_id), &options);
                }

                for name in ns_commands {
                    self.write_text("\n", &IndexMap::new());
                    let spacing_width = width - Helper::width(&name);
                    let command = commands.get(&name).unwrap().clone();
                    let command = command.borrow();
                    let command_aliases = if command.get_name().as_deref() == Some(name.as_str()) {
                        self.get_command_aliases_text(&*command)
                    } else {
                        String::new()
                    };
                    self.write_text(
                        &format!(
                            "  <info>{}</info>{}{}",
                            PhpMixed::String(name.clone()),
                            PhpMixed::String(shirabe_php_shim::str_repeat(
                                " ",
                                spacing_width as usize,
                            )),
                            PhpMixed::String(format!(
                                "{}{}",
                                command_aliases,
                                command.get_description()
                            )),
                        ),
                        &options,
                    );
                }
            }

            self.write_text("\n", &IndexMap::new());
        }
        Ok(())
    }

    fn write_text(&self, content: &str, options: &IndexMap<String, PhpMixed>) {
        let raw_text =
            matches!(options.get("raw_text"), Some(v) if shirabe_php_shim::php_truthy(v));
        let content = if raw_text {
            shirabe_php_shim::strip_tags(content)
        } else {
            content.to_string()
        };
        let decorated = match options.get("raw_output") {
            Some(v) => !shirabe_php_shim::php_truthy(v),
            None => true,
        };
        self.write(&content, decorated);
    }

    /// Formats command aliases to show them in the command description.
    fn get_command_aliases_text(&self, command: &dyn Command) -> String {
        let mut text = String::new();
        let aliases = command.get_aliases();

        if !aliases.is_empty() {
            text = format!("[{}] ", aliases.join("|"));
        }

        text
    }

    /// Formats input option/argument default value.
    fn format_default_value(&self, default: &PhpMixed) -> anyhow::Result<String> {
        if matches!(default, PhpMixed::Float(f) if f.is_infinite() && *f > 0.0) {
            return Ok("INF".to_string());
        }

        let default = match default {
            PhpMixed::String(s) => PhpMixed::String(OutputFormatter::escape(s)?),
            PhpMixed::Array(arr) => {
                let mut arr = arr.clone();
                for (_key, value) in arr.iter_mut() {
                    if let PhpMixed::String(s) = &*value {
                        *value = PhpMixed::String(OutputFormatter::escape(s)?);
                    }
                }
                PhpMixed::Array(arr)
            }
            PhpMixed::List(list) => {
                let mut list = list.clone();
                for value in list.iter_mut() {
                    if let PhpMixed::String(s) = &*value {
                        *value = PhpMixed::String(OutputFormatter::escape(s)?);
                    }
                }
                PhpMixed::List(list)
            }
            other => other.clone(),
        };

        Ok(shirabe_php_shim::str_replace(
            "\\\\",
            "\\",
            &shirabe_php_shim::json_encode_ex(
                &default,
                shirabe_php_shim::JSON_UNESCAPED_SLASHES | shirabe_php_shim::JSON_UNESCAPED_UNICODE,
            )
            .unwrap_or_default(),
        ))
    }

    /// @param array<Command|string> $commands
    fn get_column_width(&self, commands: &[CommandOrString]) -> i64 {
        let mut widths: Vec<i64> = vec![];

        for command in commands {
            // case $command instanceof Command
            match command {
                CommandOrString::Command(command) => {
                    let command = command.borrow();
                    widths.push(Helper::width(command.get_name().as_deref().unwrap_or("")));
                    for alias in command.get_aliases() {
                        widths.push(Helper::width(&alias));
                    }
                }
                CommandOrString::String(s) => {
                    widths.push(Helper::width(s));
                }
            }
        }

        if !widths.is_empty() {
            widths.into_iter().max().unwrap() + 2
        } else {
            0
        }
    }

    /// @param InputOption[] $options
    fn calculate_total_width_for_options(&self, options: &[&InputOption]) -> i64 {
        let mut total_width: i64 = 0;
        for option in options {
            // "-" + shortcut + ", --" + name
            let mut name_length = 1
                + Helper::width(option.get_shortcut().unwrap_or("")).max(1)
                + 4
                + Helper::width(option.get_name());
            if option.is_negatable() {
                name_length += 6 + Helper::width(option.get_name()); // |--no- + name
            } else if option.accept_value() {
                let mut value_length = 1 + Helper::width(option.get_name()); // = + value
                value_length += if option.is_value_optional() { 2 } else { 0 }; // [ + ]

                name_length += value_length;
            }
            total_width = std::cmp::max(total_width, name_length);
        }

        total_width
    }
}

impl DescriptorInterface for TextDescriptor {
    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        Descriptor::describe(self, output, object, options)
    }
}

impl Descriptor for TextDescriptor {
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
        TextDescriptor::describe_input_argument(self, argument, options)
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        TextDescriptor::describe_input_option(self, option, options)
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        TextDescriptor::describe_input_definition(self, definition, options)
    }

    fn describe_command(
        &mut self,
        command: &mut dyn Command,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        TextDescriptor::describe_command(self, command, options)
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        TextDescriptor::describe_application(self, application, options)
    }
}
