//! ref: composer/vendor/symfony/console/Descriptor/MarkdownDescriptor.php sha1:5fe59dff3f093090358e23d35f7f69ffdaba4227

use crate::Application;
use crate::command::Command;
use crate::descriptor::{
    ApplicationDescription, DescribableObject, Descriptor, DescriptorInterface, DescriptorOptions,
};
use crate::helper::Helper;
use crate::input::{InputArgument, InputDefinition, InputOption};
use crate::output::{OUTPUT_NORMAL, OUTPUT_RAW, OutputInterface};
use bstr::{BString, ByteSlice as _};
use shirabe_php_shim::{bformat, implode, preg_replace, str_repeat, str_replace, var_export};

/// Markdown descriptor.
#[derive(Debug, Default)]
pub struct MarkdownDescriptor {
    output: Option<std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>>,
}

impl MarkdownDescriptor {
    pub fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: DescriptorOptions,
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
        _options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        let name = if !argument.get_name().is_empty() {
            BString::from(argument.get_name())
        } else {
            BString::from("<none>")
        };
        self.write(
            bformat!(
                "#### `{name}`\n\n{}* Is required: {}\n* Is array: {}\n* Default: `{}`",
                if !argument.get_description().is_empty() {
                    bformat!(
                        "{}\n\n",
                        preg_replace("/\\s*[\\r\\n]\\s*/", "\n", argument.get_description())
                    )
                } else {
                    BString::default()
                },
                if argument.is_required() { "yes" } else { "no" },
                if argument.is_array() { "yes" } else { "no" },
                str_replace(
                    "\n",
                    "",
                    var_export(&argument.get_default().to_php_mixed(), true),
                )
            ),
            true,
        );
        Ok(())
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        _options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        let mut name = bformat!("--{}", option.get_name());
        if option.is_negatable() {
            name = bformat!("{name}|--no-{}", option.get_name());
        }
        if let Some(shortcut) = option.get_shortcut() {
            name = bformat!("{name}|-{}", str_replace("|", "|-", shortcut));
        }

        self.write(
            bformat!("#### `{name}`\n\n{}* Accept value: {}\n* Is value required: {}\n* Is multiple: {}\n* Is negatable: {}\n* Default: `{}`", if !option.get_description().is_empty() {
                    bformat!("{}\n\n", preg_replace("/\\s*[\\r\\n]\\s*/", "\n", option.get_description()))
                } else {
                    BString::default()
                }, if option.accept_value() { "yes" } else { "no" }, if option.is_value_required() {
                    "yes"
                } else {
                    "no"
                }, if option.is_array() { "yes" } else { "no" }, if option.is_negatable() { "yes" } else { "no" }, str_replace(
                    "\n",
                    "",
                    var_export(&option.get_default().to_php_mixed(), true),
                )),
            true,
        );
        Ok(())
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        _options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        let show_arguments = !definition.get_arguments().is_empty();
        if show_arguments {
            self.write("### Arguments", true);
            for argument in definition.get_arguments().values() {
                self.write("\n\n", true);
                // describeInputArgument returns null; the guarded write never runs.
                self.describe_input_argument(argument, DescriptorOptions::default())?;
            }
        }

        if !definition.get_options().is_empty() {
            if show_arguments {
                self.write("\n\n", true);
            }

            self.write("### Options", true);
            for option in definition.get_options().values() {
                self.write("\n\n", true);
                // describeInputOption returns null; the guarded write never runs.
                self.describe_input_option(option, DescriptorOptions::default())?;
            }
        }
        Ok(())
    }

    fn describe_command(
        &mut self,
        command: &dyn Command,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        if options.short {
            self.write(
                bformat!(
                    "`{}`\n{}\n\n{}### Usage\n\n{}",
                    command.get_name().unwrap_or_default(),
                    str_repeat(
                        "-",
                        (Helper::width(command.get_name().unwrap_or_default()) + 2) as usize
                    ),
                    if !command.get_description().is_empty() {
                        bformat!("{}\n\n", command.get_description())
                    } else {
                        BString::default()
                    },
                    command
                        .get_aliases()
                        .iter()
                        .fold(BString::default(), |carry, usage| {
                            bformat!("{carry}* `{usage}`\n")
                        })
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
            bformat!(
                "`{}`\n{}\n\n{}### Usage\n\n{}",
                command.get_name().unwrap_or_default(),
                str_repeat(
                    "-",
                    (Helper::width(command.get_name().unwrap_or_default()) + 2) as usize
                ),
                if !command.get_description().is_empty() {
                    bformat!("{}\n\n", command.get_description())
                } else {
                    BString::default()
                },
                usages.iter().fold(BString::default(), |carry, usage| {
                    bformat!("{carry}* `{usage}`\n")
                })
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
            self.describe_input_definition(&definition, DescriptorOptions::default())?;
        }
        Ok(())
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        let mut description =
            ApplicationDescription::new(application.clone(), options.namespace.clone(), false);
        let title = self.get_application_title(&*application.borrow());

        self.write(
            bformat!(
                "{title}\n{}",
                str_repeat("=", Helper::width(&title) as usize)
            ),
            true,
        );

        for namespace in description.get_namespaces()?.values() {
            if ApplicationDescription::GLOBAL_NAMESPACE != namespace.id {
                self.write("\n\n", true);
                self.write(bformat!("**{}:**", namespace.id), true);
            }

            self.write("\n\n", true);
            self.write(
                implode(
                    "\n",
                    &namespace
                        .commands
                        .iter()
                        .map(|command_name| {
                            Ok(bformat!(
                                "* [`{command_name}`](#{})",
                                str_replace(
                                    ":",
                                    "",
                                    description
                                        .get_command(command_name.as_bstr())?
                                        .borrow()
                                        .get_name()
                                        .unwrap_or_default(),
                                )
                            ))
                        })
                        .collect::<anyhow::Result<Vec<BString>>>()?,
                ),
                true,
            );
        }

        for command in description.get_commands()?.values() {
            let command = command.borrow();
            self.write("\n\n", true);
            // describeCommand returns null; the guarded write never runs.
            self.describe_command(&*command, options.clone())?;
        }
        Ok(())
    }

    fn get_application_title(&self, application: &dyn Application) -> BString {
        if "UNKNOWN" != application.get_name() {
            if "UNKNOWN" != application.get_version() {
                return bformat!("{} {}", application.get_name(), application.get_version());
            }

            return application.get_name();
        }

        "Console Tool".into()
    }
}

impl DescriptorInterface for MarkdownDescriptor {
    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: DescriptorOptions,
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

    fn write(&self, content: impl AsRef<[u8]>, decorated: bool) {
        // PHP overrides write() only to flip the default of $decorated to true;
        // it still delegates to parent::write.
        let _ = decorated;
        self.output().borrow().write(
            &[bstr::BString::from(content.as_ref())],
            false,
            if decorated { OUTPUT_NORMAL } else { OUTPUT_RAW },
        );
    }

    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_input_argument(self, argument, options)
    }

    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_input_option(self, option, options)
    }

    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_input_definition(self, definition, options)
    }

    fn describe_command(
        &mut self,
        command: &dyn Command,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_command(self, command, options)
    }

    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: DescriptorOptions,
    ) -> anyhow::Result<()> {
        MarkdownDescriptor::describe_application(self, application, options)
    }
}
