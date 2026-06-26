#[derive(Debug)]
pub struct RarEntry;

impl RarEntry {
    pub fn extract(&self, _path: &str) -> bool {
        todo!()
    }
}

#[derive(Debug)]
pub struct RarArchive;

impl RarArchive {
    pub fn open(_file: &str) -> Option<Self> {
        todo!()
    }

    pub fn get_entries(&self) -> Option<Vec<RarEntry>> {
        todo!()
    }

    pub fn close(&self) {
        todo!()
    }
}
