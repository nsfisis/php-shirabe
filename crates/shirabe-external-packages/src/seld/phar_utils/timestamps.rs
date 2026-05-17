#[derive(Debug)]
pub struct Timestamps {
    file: String,
}

impl Timestamps {
    pub fn new(_file: &str) -> Self {
        todo!()
    }

    pub fn update_timestamps(&mut self, _date: &str) -> anyhow::Result<()> {
        todo!()
    }

    pub fn save(&self, _file: &str, _format: i64) -> anyhow::Result<()> {
        todo!()
    }
}
