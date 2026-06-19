//! ref: composer/vendor/symfony/console/Descriptor/Descriptor.php

use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::descriptor::descriptor_interface::{
    DescribableObject, DescriptorInterface,
};
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// @internal
pub trait Descriptor: DescriptorInterface {
    fn output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>;

    fn set_output(&mut self, output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>);

    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        self.set_output(output);

        // PHP dispatches via `$object instanceof ...`; the explicit `DescribableObject` enum makes
        // that dispatch a `match`.
        match object {
            DescribableObject::InputArgument(argument) => {
                self.describe_input_argument(&argument, options)?;
            }
            DescribableObject::InputOption(option) => {
                self.describe_input_option(&option, options)?;
            }
            DescribableObject::InputDefinition(definition) => {
                self.describe_input_definition(&definition, options)?;
            }
            DescribableObject::Command(command) => {
                self.describe_command(&mut *command.borrow_mut(), options)?;
            }
            DescribableObject::Application(application) => {
                self.describe_application(application, options)?;
            }
        }

        Ok(())
    }

    /// Writes content to output.
    fn write(&self, content: &str, decorated: bool) {
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

    /// Describes an InputArgument instance.
    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()>;

    /// Describes an InputOption instance.
    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()>;

    /// Describes an InputDefinition instance.
    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()>;

    /// Describes a Command instance.
    fn describe_command(
        &mut self,
        command: &mut dyn Command,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()>;

    /// Describes an Application instance.
    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()>;
}
