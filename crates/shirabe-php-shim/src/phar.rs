use crate::PhpMixed;
use indexmap::IndexMap;

#[derive(Debug)]
pub struct Phar {
    path: String,
}

impl Phar {
    pub const ZIP: i64 = 1;
    pub const TAR: i64 = 2;
    pub const GZ: i64 = 4096;
    pub const BZ2: i64 = 8192;

    pub fn new(_a: String) -> Self {
        todo!()
    }

    pub fn extract_to(&self, _a: &str, _b: Option<()>, _c: bool) {
        todo!()
    }

    pub fn running(_return_full: bool) -> String {
        todo!()
    }
}

impl Phar {
    pub const SHA512: i64 = 16;

    pub fn new_phar(_filename: String, _flags: i64, _alias: &str) -> Self {
        todo!()
    }

    pub fn set_signature_algorithm(&mut self, _algo: i64) {
        todo!()
    }

    pub fn start_buffering(&mut self) {
        todo!()
    }

    pub fn stop_buffering(&mut self) {
        todo!()
    }

    pub fn add_from_string(&mut self, _path: &str, _content: &str) {
        todo!()
    }

    pub fn set_stub(&mut self, _stub: &str) {
        todo!()
    }
}

#[derive(Debug)]
pub struct PharException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for PharException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PharException {}

#[derive(Debug)]
pub struct PharFileInfo;

impl PharFileInfo {
    pub fn get_content(&self) -> String {
        todo!()
    }

    pub fn get_basename(&self) -> String {
        todo!()
    }

    pub fn is_dir(&self) -> bool {
        todo!()
    }
}

#[derive(Debug)]
pub struct PharData {
    path: String,
}

impl PharData {
    pub fn new(_a: String) -> Self {
        todo!()
    }

    pub fn new_with_format(_path: String, _flags: i64, _alias: &str, _format: i64) -> Self {
        todo!()
    }

    pub fn can_compress(_algo: i64) -> bool {
        todo!()
    }

    pub fn valid(&self) -> bool {
        todo!()
    }

    pub fn get(&self, _key: &str) -> Option<PharFileInfo> {
        todo!()
    }

    pub fn iter(&self) -> impl Iterator<Item = PharFileInfo> {
        todo!();
        std::iter::empty()
    }

    pub fn extract_to(&self, _a: &str, _b: Option<()>, _c: bool) {
        todo!()
    }

    pub fn add_empty_dir(&self, _a: &str) {
        todo!()
    }

    pub fn build_from_iterator(
        &self,
        _iter: &mut dyn Iterator<Item = std::path::PathBuf>,
        _base: &str,
    ) {
        todo!()
    }

    pub fn compress(&self, _algo: i64) {
        todo!()
    }
}
