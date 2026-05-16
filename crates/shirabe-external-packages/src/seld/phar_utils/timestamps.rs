#[derive(Debug)]
pub struct Timestamps {
    file: String,
}

impl Timestamps {
    pub fn new(file: &str) -> Self {
        todo!()
    }

    pub fn update_timestamps(&mut self, date: &str) -> anyhow::Result<()> {
        todo!()
    }

    pub fn save(&self, file: &str, format: i64) -> anyhow::Result<()> {
        todo!()
    }
}
