use crate::symfony::console::output::output_interface::OutputInterface;
use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

/// Descriptor interface.
pub trait DescriptorInterface {
    fn describe(
        &mut self,
        output: std::rc::Rc<std::cell::RefCell<dyn OutputInterface>>,
        object: PhpMixed,
        options: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<()>;
}
