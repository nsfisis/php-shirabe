use crate::PhpMixed;
use indexmap::IndexMap;

#[derive(Debug)]
pub struct ZipArchive {
    pub num_files: i64,
}

impl Default for ZipArchive {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipArchive {
    pub fn new() -> Self {
        todo!()
    }

    pub fn open(&mut self, _filename: &str, _flags: i64) -> Result<(), i64> {
        todo!()
    }

    pub fn close(&self) -> bool {
        todo!()
    }

    pub fn count(&self) -> i64 {
        todo!()
    }

    pub fn stat_index(&self, _index: i64) -> Option<IndexMap<String, PhpMixed>> {
        todo!()
    }

    pub fn extract_to(&self, _path: &str) -> bool {
        todo!()
    }

    pub fn locate_name(&self, _name: &str) -> Option<i64> {
        todo!()
    }

    pub fn get_from_index(&self, _index: i64) -> Option<String> {
        todo!()
    }

    pub fn get_name_index(&self, _index: i64) -> String {
        todo!()
    }

    pub fn get_stream(&self, _name: &str) -> Option<PhpMixed> {
        todo!()
    }

    pub fn add_empty_dir(&self, _local_name: &str) -> bool {
        todo!()
    }

    pub fn add_file(&self, _filepath: &str, _local_name: &str) -> bool {
        todo!()
    }

    pub fn set_external_attributes_name(&self, _name: &str, _opsys: i64, _attr: i64) -> bool {
        todo!()
    }

    pub fn get_status_string(&self) -> String {
        todo!()
    }
}

impl ZipArchive {
    pub const CREATE: i64 = 1;
    pub const OPSYS_UNIX: i64 = 3;
    pub const ER_SEEK: i64 = 4;
    pub const ER_READ: i64 = 5;
    pub const ER_NOENT: i64 = 9;
    pub const ER_EXISTS: i64 = 10;
    pub const ER_OPEN: i64 = 11;
    pub const ER_MEMORY: i64 = 14;
    pub const ER_INVAL: i64 = 18;
    pub const ER_NOZIP: i64 = 19;
    pub const ER_INCONS: i64 = 21;
}
