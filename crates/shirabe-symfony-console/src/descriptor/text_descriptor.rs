//! ref: composer/vendor/symfony/console/Descriptor/TextDescriptor.php sha1:3100d8bd2d91d6b105b8a4e9f6f4963bb8faa096

use crate::Application;
use crate::command::Command;
use crate::descriptor::{
    ApplicationDescription, DescribableObject, Descriptor, DescriptorInterface, DescriptorOptions,
};
use crate::formatter::OutputFormatter;
use crate::helper::Helper;
use crate::input::{InputArgument, InputDefinition, InputOption};
use crate::output::OutputInterface;
use bstr::{BString, ByteSlice as _};
use shirabe_php_shim::{
    JSON_UNESCAPED_SLASHES, JSON_UNESCAPED_UNICODE, PhpMixed, STR_PAD_RIGHT, bformat, count,
    implode, json_encode_ex, preg_replace, str_pad, str_repeat, str_replace, strip_tags, strlen,
};

/// Text descriptor.
#[derive(Debug, Default)]
pub struct TextDescriptor {
    output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
}

/// Models PHP's `array<Command|string>` passed to `getColumnWidth`.
/// `PhpMixed` cannot hold console types, so a dedicated enum is used.
#[derive(Debug)]
enum CommandOrString {
    Command(std::rc::Rc<std::cell::RefCell<dyn Command>>),
    String(BString),
}

impl TextDescriptor {
    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        let default_value = argument.get_default().to_php_mixed();
        let default = if !default_value.is_null()
            && (!matches!(default_value, PhpMixed::List(_) | PhpMixed::Array(_))
                || count(&default_value) != 0)
        {
            bformat!(
                "<comment> [default: {}]</comment>",
                self.format_default_value(&default_value)?
            )
        } else {
            BString::default()
        };

        let total_width = options
            .total_width
            .unwrap_or_else(|| Helper::width(argument.get_name()));
        let spacing_width = total_width - strlen(argument.get_name());

        self.write_text(
            bformat!(
                "  <info>{}</info>  {}{}{default}",
                argument.get_name(),
                str_repeat(" ", spacing_width as usize),
                preg_replace(
                    "/\\s*[\\r\\n]\\s*/",
                    bformat!("\n{}", str_repeat(" ", (total_width + 4) as usize)),
                    argument.get_description(),
                )
            ),
            &options,
        );
        Ok(())
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        let default_value = option.get_default().to_php_mixed();
        let default = if option.accept_value()
            && !default_value.is_null()
            && (!matches!(default_value, PhpMixed::List(_) | PhpMixed::Array(_))
                || count(&default_value) != 0)
        {
            bformat!(
                "<comment> [default: {}]</comment>",
                self.format_default_value(&default_value)?
            )
        } else {
            BString::default()
        };

        let mut value = BString::default();
        if option.accept_value() {
            value = bformat!("={}", option.get_name().to_ascii_uppercase());

            if option.is_value_optional() {
                value = bformat!("[{value}]");
            }
        }

        let total_width = options
            .total_width
            .unwrap_or_else(|| self.calculate_total_width_for_options(&[option]));
        let synopsis = bformat!(
            "{}{}",
            match option.get_shortcut() {
                Some(shortcut) => bformat!("-{shortcut}, "),
                None => BString::from("    "),
            },
            if option.is_negatable() {
                bformat!("--{}|--no-{}", option.get_name(), option.get_name())
            } else {
                bformat!("--{}{value}", option.get_name())
            }
        );

        let spacing_width = total_width - Helper::width(&synopsis);

        self.write_text(
            bformat!(
                "  <info>{synopsis}</info>  {}{}{default}{}",
                str_repeat(" ", spacing_width as usize),
                preg_replace(
                    "/\\s*[\\r\\n]\\s*/",
                    bformat!("\n{}", str_repeat(" ", (total_width + 4) as usize)),
                    option.get_description(),
                ),
                if option.is_array() {
                    "<comment> (multiple values allowed)</comment>"
                } else {
                    ""
                }
            ),
            &options,
        );
        Ok(())
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: DescriptorOptions,
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
            self.write_text("\n", &DescriptorOptions::default());
            for argument in definition.get_arguments().values() {
                let merged = DescriptorOptions {
                    total_width: Some(total_width),
                    ..options.clone()
                };
                self.describe_input_argument(argument, merged)?;
                self.write_text("\n", &DescriptorOptions::default());
            }
        }

        if !definition.get_arguments().is_empty() && !definition.get_options().is_empty() {
            self.write_text("\n", &DescriptorOptions::default());
        }

        if !definition.get_options().is_empty() {
            let mut later_options: Vec<&InputOption> = vec![];

            self.write_text("<comment>Options:</comment>", &options);
            for option in definition.get_options().values() {
                if strlen(option.get_shortcut().unwrap_or_default()) > 1 {
                    later_options.push(option.as_ref());
                    continue;
                }
                self.write_text("\n", &DescriptorOptions::default());
                let merged = DescriptorOptions {
                    total_width: Some(total_width),
                    ..options.clone()
                };
                self.describe_input_option(option, merged)?;
            }
            for option in later_options {
                self.write_text("\n", &DescriptorOptions::default());
                let merged = DescriptorOptions {
                    total_width: Some(total_width),
                    ..options.clone()
                };
                self.describe_input_option(option, merged)?;
            }
        }
        Ok(())
    }

    fn describe_command(
        &mut self,
        command: &dyn Command,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        command.merge_application_definition(false);

        let description = command.get_description();
        if !description.is_empty() {
            self.write_text("<comment>Description:</comment>", &options);
            self.write_text("\n", &DescriptorOptions::default());
            self.write_text(bformat!("  {description}"), &DescriptorOptions::default());
            self.write_text("\n\n", &DescriptorOptions::default());
        }

        self.write_text("<comment>Usage:</comment>", &options);
        let mut usages = vec![command.get_synopsis(true)];
        usages.extend(command.get_aliases());
        usages.extend(command.get_usages());
        for usage in usages {
            self.write_text("\n", &DescriptorOptions::default());
            self.write_text(bformat!("  {}", OutputFormatter::escape(&usage)?), &options);
        }
        self.write_text("\n", &DescriptorOptions::default());

        let definition = command.get_definition().clone();
        if !definition.get_options().is_empty() || !definition.get_arguments().is_empty() {
            self.write_text("\n", &DescriptorOptions::default());
            self.describe_input_definition(&definition, options.clone())?;
            self.write_text("\n", &DescriptorOptions::default());
        }

        let help = command.get_processed_help();
        if !help.is_empty() && help != description {
            self.write_text("\n", &DescriptorOptions::default());
            self.write_text("<comment>Help:</comment>", &options);
            self.write_text("\n", &DescriptorOptions::default());
            self.write_text(bformat!("  {}", str_replace("\n", "\n  ", &help)), &options);
            self.write_text("\n", &DescriptorOptions::default());
        }
        Ok(())
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        let described_namespace = options.namespace.clone();
        let mut description =
            ApplicationDescription::new(application.clone(), described_namespace.clone(), false);

        if options.raw_text {
            let width = self.get_column_width(
                &description
                    .get_commands()?
                    .values()
                    .map(|c| CommandOrString::Command(c.clone()))
                    .collect::<Vec<_>>(),
            );

            let command_list: Vec<_> = description.get_commands()?.values().cloned().collect();
            for command in &command_list {
                let command = command.borrow();
                self.write_text(
                    bformat!(
                        "{} {}",
                        str_pad(
                            command.get_name().unwrap_or_default(),
                            width as usize,
                            " ",
                            STR_PAD_RIGHT,
                        ),
                        command.get_description()
                    ),
                    &options,
                );
                self.write_text("\n", &DescriptorOptions::default());
            }
        } else {
            let help = application.borrow().get_help();
            if !help.is_empty() {
                self.write_text(bformat!("{help}\n\n"), &options);
            }

            self.write_text("<comment>Usage:</comment>\n", &options);
            self.write_text("  command [options] [arguments]\n\n", &options);

            let app_definition = application.borrow_mut().get_definition();
            let options_only: Vec<std::rc::Rc<InputOption>> = app_definition
                .borrow()
                .get_options()
                .values()
                .cloned()
                .collect();
            let definition = InputDefinition::from_options(options_only)?;
            self.describe_input_definition(&definition, options.clone())?;

            self.write_text("\n", &DescriptorOptions::default());
            self.write_text("\n", &DescriptorOptions::default());

            let mut commands = description.get_commands()?.clone();
            let namespaces = description.get_namespaces()?;
            if described_namespace.is_some() && !namespaces.is_empty() {
                // make sure all alias commands are included when describing a specific namespace
                let described_namespace_info = namespaces.values().next().unwrap();
                for name in &described_namespace_info.commands {
                    let command = description.get_command(name.as_bstr())?;
                    commands.insert(name.clone(), command);
                }
            }

            // calculate max. width based on available commands per namespace
            let width = self.get_column_width(&{
                let command_keys: Vec<BString> = commands.keys().cloned().collect();
                let mut merged: Vec<CommandOrString> = vec![];
                for namespace in namespaces.values() {
                    for name in &namespace.commands {
                        if command_keys.iter().any(|key| name == key) {
                            merged.push(CommandOrString::String(name.clone()));
                        }
                    }
                }
                merged
            });

            if let Some(ref described_namespace) = described_namespace {
                self.write_text(
                    bformat!("<comment>Available commands for the \"{described_namespace}\" namespace:</comment>"),
                    &options,
                );
            } else {
                self.write_text("<comment>Available commands:</comment>", &options);
            }

            for namespace in namespaces.values() {
                let ns_commands: Vec<BString> = namespace
                    .commands
                    .iter()
                    .filter(|name| commands.contains_key(*name))
                    .cloned()
                    .collect();

                if ns_commands.is_empty() {
                    continue;
                }

                let namespace_id = &namespace.id;

                if described_namespace.is_none()
                    && ApplicationDescription::GLOBAL_NAMESPACE != namespace_id
                {
                    self.write_text("\n", &DescriptorOptions::default());
                    self.write_text(bformat!(" <comment>{namespace_id}</comment>"), &options);
                }

                for name in ns_commands {
                    self.write_text("\n", &DescriptorOptions::default());
                    let spacing_width = width - Helper::width(&name);
                    let command = commands.get(&name).unwrap().clone();
                    let command = command.borrow();
                    let command_aliases = if command.get_name().as_ref() == Some(&name) {
                        self.get_command_aliases_text(&*command)
                    } else {
                        BString::default()
                    };
                    self.write_text(
                        bformat!(
                            "  <info>{name}</info>{}{command_aliases}{}",
                            str_repeat(" ", spacing_width as usize),
                            command.get_description()
                        ),
                        &options,
                    );
                }
            }

            self.write_text("\n", &DescriptorOptions::default());
        }
        Ok(())
    }

    fn write_text(&self, content: impl AsRef<[u8]>, options: &DescriptorOptions) {
        let content = if options.raw_text {
            strip_tags(content)
        } else {
            BString::from(content.as_ref())
        };
        let decorated = match options.raw_output {
            Some(raw_output) => !raw_output,
            None => true,
        };
        self.write(&content, decorated);
    }

    /// Formats command aliases to show them in the command description.
    fn get_command_aliases_text(&self, command: &dyn Command) -> BString {
        let mut text = BString::default();
        let aliases = command.get_aliases();

        if !aliases.is_empty() {
            text = bformat!("[{}] ", implode("|", &aliases));
        }

        text
    }

    /// Formats input option/argument default value.
    fn format_default_value(&self, default: &PhpMixed) -> anyhow::Result<BString> {
        if matches!(default, PhpMixed::Float(f) if f.is_infinite() && *f > 0.0) {
            return Ok("INF".into());
        }

        let default = match default {
            PhpMixed::String(s) => PhpMixed::string(OutputFormatter::escape(s)?),
            PhpMixed::Array(arr) => {
                let mut arr = arr.clone();
                for (_key, value) in arr.iter_mut() {
                    if let PhpMixed::String(s) = &*value {
                        *value = PhpMixed::string(OutputFormatter::escape(s)?);
                    }
                }
                PhpMixed::Array(arr)
            }
            PhpMixed::List(list) => {
                let mut list = list.clone();
                for value in list.iter_mut() {
                    if let PhpMixed::String(s) = &*value {
                        *value = PhpMixed::string(OutputFormatter::escape(s)?);
                    }
                }
                PhpMixed::List(list)
            }
            other => other.clone(),
        };

        Ok(str_replace(
            "\\\\",
            "\\",
            json_encode_ex(&default, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE)
                .unwrap_or_default(),
        ))
    }

    fn get_column_width(&self, commands: &[CommandOrString]) -> i64 {
        let mut widths: Vec<i64> = vec![];

        for command in commands {
            // case $command instanceof Command
            match command {
                CommandOrString::Command(command) => {
                    let command = command.borrow();
                    widths.push(Helper::width(command.get_name().unwrap_or_default()));
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

    fn calculate_total_width_for_options(&self, options: &[&InputOption]) -> i64 {
        let mut total_width: i64 = 0;
        for option in options {
            // "-" + shortcut + ", --" + name
            let mut name_length = 1
                + Helper::width(option.get_shortcut().unwrap_or_default()).max(1)
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
        options: DescriptorOptions,
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
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        TextDescriptor::describe_input_argument(self, argument, options)
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        TextDescriptor::describe_input_option(self, option, options)
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        TextDescriptor::describe_input_definition(self, definition, options)
    }

    fn describe_command(
        &mut self,
        command: &dyn Command,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        TextDescriptor::describe_command(self, command, options)
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        TextDescriptor::describe_application(self, application, options)
    }
}
