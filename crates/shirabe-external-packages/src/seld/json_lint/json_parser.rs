use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct JsonParser;

impl JsonParser {
    pub const DETECT_KEY_CONFLICTS: u32 = 1;

    pub fn new() -> Self {
        todo!()
    }

    pub fn parse(&self, json: &str, flags: u32) -> anyhow::Result<PhpMixed> {
        todo!()
    }
}
