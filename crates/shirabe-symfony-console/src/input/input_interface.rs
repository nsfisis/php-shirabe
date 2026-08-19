//! ref: composer/vendor/symfony/console/Input/InputInterface.php

use crate::input::InputDefinition;
use crate::input::InputValue;
use crate::input::StreamableInputInterface;

pub trait InputInterface: std::fmt::Debug + shirabe_php_shim::AsAny {
    /// Models PHP's `clone` operatior.
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>>;

    fn get_first_argument(&self) -> Option<String>;

    fn has_parameter_option(&self, values: &[&str], only_params: bool) -> bool;

    fn get_parameter_option(
        &self,
        values: &[&str],
        default: InputValue,
        only_params: bool,
    ) -> InputValue;

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()>;

    fn validate(&mut self) -> anyhow::Result<()>;

    fn get_arguments(&self) -> indexmap::IndexMap<String, InputValue>;

    fn get_argument(&self, name: &str) -> anyhow::Result<InputValue>;

    fn set_argument(&mut self, name: &str, value: InputValue) -> anyhow::Result<()>;

    fn has_argument(&self, name: &str) -> bool;

    fn get_options(&self) -> indexmap::IndexMap<String, InputValue>;

    fn get_option(&self, name: &str) -> anyhow::Result<InputValue>;

    fn set_option(&mut self, name: &str, value: InputValue) -> anyhow::Result<()>;

    fn has_option(&self, name: &str) -> bool;

    fn is_interactive(&self) -> bool;

    fn set_interactive(&mut self, interactive: bool);

    /// PHP's `(string) $input` (the `__toString` magic method every Symfony input implements);
    /// implementors forward to their `Display` impl.
    fn __to_string(&self) -> String;

    /// Models PHP's `$input instanceof StreamableInputInterface` check. Streamable inputs override
    /// this to return `Some(self)`; everything else falls back to `None`.
    fn as_streamable(&self) -> Option<&dyn StreamableInputInterface> {
        None
    }

    /// Mutable counterpart of `as_streamable`, needed to call `set_stream`/`set_interactive`.
    fn as_streamable_mut(&mut self) -> Option<&mut dyn StreamableInputInterface> {
        None
    }
}
