use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::descriptor::descriptor_interface::DescriptorInterface;
use crate::symfony::console::exception::invalid_argument_exception::InvalidArgumentException;
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
        object: PhpMixed,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()> {
        self.set_output(output);

        // PHP dispatches via `$object instanceof ...`. The concrete runtime type of
        // `object` must be recovered to route to the correct describe* method.
        match true {
            // case $object instanceof InputArgument:
            _ if todo!("$object instanceof InputArgument") => {
                let argument: InputArgument = todo!("downcast object to InputArgument");
                self.describe_input_argument(&argument, options)?;
            }
            // case $object instanceof InputOption:
            _ if todo!("$object instanceof InputOption") => {
                let option: InputOption = todo!("downcast object to InputOption");
                self.describe_input_option(&option, options)?;
            }
            // case $object instanceof InputDefinition:
            _ if todo!("$object instanceof InputDefinition") => {
                let definition: InputDefinition = todo!("downcast object to InputDefinition");
                self.describe_input_definition(&definition, options)?;
            }
            // case $object instanceof Command:
            _ if todo!("$object instanceof Command") => {
                let mut command: Box<dyn Command> = todo!("downcast object to Command");
                self.describe_command(command.as_mut(), options)?;
            }
            // case $object instanceof Application:
            _ if todo!("$object instanceof Application") => {
                let application: std::rc::Rc<std::cell::RefCell<dyn Application>> =
                    todo!("downcast object to Application");
                self.describe_application(application, options)?;
            }
            _ => {
                return Err(
                    InvalidArgumentException(shirabe_php_shim::InvalidArgumentException {
                        message: format!(
                            "Object of type \"{}\" is not describable.",
                            shirabe_php_shim::get_debug_type(&object)
                        ),
                        code: 0,
                    })
                    .into(),
                );
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
