use crate::symfony::console::input::input_definition::InputDefinition;
use shirabe_php_shim::PhpMixed;

pub trait InputInterface: std::fmt::Debug + shirabe_php_shim::AsAny {
    /// Models PHP's `clone` operatior.
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>>;

    fn get_first_argument(&self) -> Option<String>;

    fn has_parameter_option(&self, values: PhpMixed, only_params: bool) -> bool;

    fn get_parameter_option(
        &self,
        values: PhpMixed,
        default: PhpMixed,
        only_params: bool,
    ) -> PhpMixed;

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()>;

    fn validate(&mut self) -> anyhow::Result<()>;

    fn get_arguments(&self) -> indexmap::IndexMap<String, PhpMixed>;

    fn get_argument(&self, name: &str) -> anyhow::Result<PhpMixed>;

    fn set_argument(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;

    fn has_argument(&self, name: &str) -> bool;

    fn get_options(&self) -> indexmap::IndexMap<String, PhpMixed>;

    fn get_option(&self, name: &str) -> anyhow::Result<PhpMixed>;

    fn set_option(&mut self, name: &str, value: PhpMixed) -> anyhow::Result<()>;

    fn has_option(&self, name: &str) -> bool;

    fn is_interactive(&self) -> bool;

    fn set_interactive(&mut self, interactive: bool);
}
