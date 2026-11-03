//! ref: composer/vendor/symfony/console/Input/InputInterface.php

use crate::input::{InputDefinition, InputValue, StreamableInputInterface};
use shirabe_php_shim::AsAny;

pub trait InputInterface: std::fmt::Debug + AsAny {
    /// Models PHP's `clone` operatior.
    fn dup(&self) -> std::rc::Rc<std::cell::RefCell<dyn InputInterface>>;

    fn get_first_argument(&self) -> Option<bstr::BString>;

    fn has_parameter_option_bytes(&self, values: &[&bstr::BStr], only_params: bool) -> bool;

    fn get_parameter_option_bytes(
        &self,
        values: &[&bstr::BStr],
        default: InputValue,
        only_params: bool,
    ) -> InputValue;

    fn bind(&mut self, definition: &InputDefinition) -> anyhow::Result<()>;

    fn validate(&mut self) -> anyhow::Result<()>;

    fn get_arguments(&self) -> indexmap::IndexMap<bstr::BString, InputValue>;

    fn get_argument_bytes(&self, name: &bstr::BStr) -> anyhow::Result<InputValue>;

    fn set_argument_bytes(&mut self, name: &bstr::BStr, value: InputValue) -> anyhow::Result<()>;

    fn has_argument_bytes(&self, name: &bstr::BStr) -> bool;

    fn get_options(&self) -> indexmap::IndexMap<bstr::BString, InputValue>;

    fn get_option_bytes(&self, name: &bstr::BStr) -> anyhow::Result<InputValue>;

    fn set_option_bytes(&mut self, name: &bstr::BStr, value: InputValue) -> anyhow::Result<()>;

    fn has_option_bytes(&self, name: &bstr::BStr) -> bool;

    fn is_interactive(&self) -> bool;

    fn set_interactive(&mut self, interactive: bool);

    /// PHP's `(string) $input` (the `__toString` magic method every Symfony input implements).
    fn __to_string(&self) -> bstr::BString;

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

// The overloads of the PHP methods taking a name (`getOption()`, `hasParameterOption()`, ...), which
// accept any byte string. They are generic, so they stay out of the `dyn InputInterface` vtable.
pub trait InputInterfaceByName: InputInterface {
    fn has_parameter_option(&self, values: &[impl AsRef<[u8]>], only_params: bool) -> bool {
        let values: Vec<&bstr::BStr> = values
            .iter()
            .map(|value| bstr::BStr::new(value.as_ref()))
            .collect();
        self.has_parameter_option_bytes(&values, only_params)
    }

    fn get_parameter_option(
        &self,
        values: &[impl AsRef<[u8]>],
        default: InputValue,
        only_params: bool,
    ) -> InputValue {
        let values: Vec<&bstr::BStr> = values
            .iter()
            .map(|value| bstr::BStr::new(value.as_ref()))
            .collect();
        self.get_parameter_option_bytes(&values, default, only_params)
    }

    fn get_argument(&self, name: impl AsRef<[u8]>) -> anyhow::Result<InputValue> {
        self.get_argument_bytes(bstr::BStr::new(name.as_ref()))
    }

    fn set_argument(&mut self, name: impl AsRef<[u8]>, value: InputValue) -> anyhow::Result<()> {
        self.set_argument_bytes(bstr::BStr::new(name.as_ref()), value)
    }

    fn has_argument(&self, name: impl AsRef<[u8]>) -> bool {
        self.has_argument_bytes(bstr::BStr::new(name.as_ref()))
    }

    fn get_option(&self, name: impl AsRef<[u8]>) -> anyhow::Result<InputValue> {
        self.get_option_bytes(bstr::BStr::new(name.as_ref()))
    }

    fn set_option(&mut self, name: impl AsRef<[u8]>, value: InputValue) -> anyhow::Result<()> {
        self.set_option_bytes(bstr::BStr::new(name.as_ref()), value)
    }

    fn has_option(&self, name: impl AsRef<[u8]>) -> bool {
        self.has_option_bytes(bstr::BStr::new(name.as_ref()))
    }
}

impl<T: InputInterface + ?Sized> InputInterfaceByName for T {}
