use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct InputArgument;

impl InputArgument {
    pub const REQUIRED: i64 = 1;
    pub const OPTIONAL: i64 = 2;
    pub const IS_ARRAY: i64 = 4;

    pub fn new(
        _name: &str,
        _mode: Option<i64>,
        _description: &str,
        _default: Option<PhpMixed>,
    ) -> Self {
        todo!()
    }

    pub fn get_name(&self) -> String {
        todo!()
    }

    pub fn is_required(&self) -> bool {
        todo!()
    }

    pub fn is_array(&self) -> bool {
        todo!()
    }

    pub fn get_default(&self) -> Option<PhpMixed> {
        todo!()
    }
}
