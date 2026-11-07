//! ref: composer/vendor/symfony/console/Descriptor/Descriptor.php sha1:4b4866d2edfab4974c000ab7b8cf46660aed0e32

use crate::application::Application;
use crate::command::Command;
use crate::descriptor::{DescribableObject, DescriptorInterface, DescriptorOptions};
use crate::input::{InputArgument, InputDefinition, InputOption};
use crate::output::{OUTPUT_NORMAL, OUTPUT_RAW, OutputInterface};

pub trait Descriptor: DescriptorInterface {
    fn output(&self) -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>;

    fn set_output(&mut self, output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>);

    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: DescriptorOptions,
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
                self.describe_command(&*command.borrow(), options)?;
            }
            DescribableObject::Application(application) => {
                self.describe_application(application, options)?;
            }
        }

        Ok(())
    }

    /// Writes content to output.
    fn write(&self, content: impl AsRef<[u8]>, decorated: bool) {
        self.output().borrow().write(
            &[bstr::BString::from(content.as_ref())],
            false,
            if decorated { OUTPUT_NORMAL } else { OUTPUT_RAW },
        );
    }

    /// Describes an InputArgument instance.
    fn describe_input_argument(
        &mut self,
        argument: &InputArgument,
        options: DescriptorOptions,
    ) -> anyhow::Result<()>;

    /// Describes an InputOption instance.
    fn describe_input_option(
        &mut self,
        option: &InputOption,
        options: DescriptorOptions,
    ) -> anyhow::Result<()>;

    /// Describes an InputDefinition instance.
    fn describe_input_definition(
        &mut self,
        definition: &InputDefinition,
        options: DescriptorOptions,
    ) -> anyhow::Result<()>;

    /// Describes a Command instance.
    fn describe_command(
        &mut self,
        command: &dyn Command,
        options: DescriptorOptions,
    ) -> anyhow::Result<()>;

    /// Describes an Application instance.
    fn describe_application(
        &mut self,
        application: std::rc::Rc<std::cell::RefCell<dyn Application>>,
        options: DescriptorOptions,
    ) -> anyhow::Result<()>;
}
