//! ref: composer/src/Composer/Console/Input/InputOption.php

use anyhow::Result;
use shirabe_external_packages::symfony::console::input::InputOption as BaseInputOption;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct InputOption {
    inner: BaseInputOption,
}

impl InputOption {
    pub const VALUE_NONE: i64 = 1;
    pub const VALUE_REQUIRED: i64 = 2;
    pub const VALUE_OPTIONAL: i64 = 4;
    pub const VALUE_IS_ARRAY: i64 = 8;
    pub const VALUE_NEGATABLE: i64 = 16;

    pub fn new(
        name: &str,
        shortcut: Option<PhpMixed>,
        mode: Option<i64>,
        description: &str,
        default: Option<PhpMixed>,
        // TODO(cli-completion): suggested_values closure / list dropped along with completion support
    ) -> Result<Self> {
        let shortcut_str = shortcut.as_ref().and_then(|s| s.as_string());
        let default_mixed = default.unwrap_or(PhpMixed::Null);
        let inner = BaseInputOption::new(name, shortcut_str, mode, description, default_mixed);
        Ok(Self { inner })
    }
}
