#[derive(Debug)]
pub struct RarEntry;

impl RarEntry {
    pub fn extract(&self, _path: impl AsRef<std::path::Path>) -> bool {
        todo!()
    }
}

#[derive(Debug)]
pub struct RarArchive;

impl RarArchive {
    pub fn open(_file: impl AsRef<std::path::Path>) -> Option<Self> {
        todo!()
    }

    pub fn get_entries(&self) -> Option<Vec<RarEntry>> {
        todo!()
    }

    pub fn close(&self) {
        todo!()
    }
}
