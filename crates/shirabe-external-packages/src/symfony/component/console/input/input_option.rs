use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct InputOption;

impl InputOption {
    pub const VALUE_NONE: i64 = 1;
    pub const VALUE_REQUIRED: i64 = 2;
    pub const VALUE_OPTIONAL: i64 = 4;
    pub const VALUE_IS_ARRAY: i64 = 8;
    pub const VALUE_NEGATABLE: i64 = 16;

    pub fn new(
        name: &str,
        shortcut: Option<&str>,
        mode: Option<i64>,
        description: &str,
        default: PhpMixed,
    ) -> Self {
        todo!()
    }

    pub fn get_name(&self) -> String {
        todo!()
    }

    pub fn accept_value(&self) -> bool {
        todo!()
    }

    pub fn is_value_required(&self) -> bool {
        todo!()
    }

    pub fn is_value_optional(&self) -> bool {
        todo!()
    }

    pub fn is_array(&self) -> bool {
        todo!()
    }

    pub fn get_default(&self) -> PhpMixed {
        todo!()
    }
}
