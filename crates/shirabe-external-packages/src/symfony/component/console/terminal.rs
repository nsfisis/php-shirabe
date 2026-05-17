#[derive(Debug)]
pub struct Terminal;

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}

impl Terminal {
    pub fn new() -> Self {
        todo!()
    }

    pub fn get_width(&self) -> i64 {
        todo!()
    }

    pub fn get_height(&self) -> i64 {
        todo!()
    }
}
