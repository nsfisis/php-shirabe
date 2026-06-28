//! ref: composer/src/Composer/Console/Input/InputArgument.php

use shirabe_external_packages::symfony::console::input::InputArgument as BaseInputArgument;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct InputArgument {
    inner: BaseInputArgument,
}

impl InputArgument {
    pub const REQUIRED: i64 = 1;
    pub const OPTIONAL: i64 = 2;
    pub const IS_ARRAY: i64 = 4;

    pub fn new(
        name: &str,
        mode: Option<i64>,
        description: &str,
        default: Option<PhpMixed>,
        // TODO(cli-completion): suggested_values closure / list dropped along with completion support
    ) -> anyhow::Result<Self> {
        let inner = BaseInputArgument::new(
            name.to_string(),
            mode,
            description.to_string(),
            default.unwrap_or(PhpMixed::Null),
        )?;
        Ok(Self { inner })
    }

    /// Unwraps to the underlying Symfony `InputArgument` (used when forwarding a Composer-typed
    /// definition to the Symfony command state).
    pub(crate) fn to_base(&self) -> BaseInputArgument {
        self.inner.clone()
    }
}
