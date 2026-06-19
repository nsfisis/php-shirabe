//! ref: composer/vendor/symfony/console/Descriptor/DescriptorInterface.php

use crate::symfony::console::application::Application;
use crate::symfony::console::command::command::Command;
use crate::symfony::console::input::input_argument::InputArgument;
use crate::symfony::console::input::input_definition::InputDefinition;
use crate::symfony::console::input::input_option::InputOption;
use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// The set of objects the descriptors know how to describe.
pub enum DescribableObject {
    InputArgument(InputArgument),
    InputOption(InputOption),
    InputDefinition(InputDefinition),
    Command(std::rc::Rc<std::cell::RefCell<dyn Command>>),
    Application(std::rc::Rc<std::cell::RefCell<dyn Application>>),
}

/// Descriptor interface.
pub trait DescriptorInterface {
    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: DescribableObject,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()>;
}
