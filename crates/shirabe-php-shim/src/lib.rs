use indexmap::IndexMap;

#[derive(Debug, Clone)]
pub enum PhpMixed {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Box<PhpMixed>>),
    Array(IndexMap<String, Box<PhpMixed>>),
}

#[derive(Debug)]
pub struct Exception {
    pub message: String,
    pub code: i64,
}

#[derive(Debug)]
pub struct RuntimeException {
    pub message: String,
    pub code: i64,
}

#[derive(Debug)]
pub struct UnexpectedValueException {
    pub message: String,
    pub code: i64,
}

#[derive(Debug)]
pub struct InvalidArgumentException {
    pub message: String,
    pub code: i64,
}

pub fn get_debug_type(value: &PhpMixed) -> String {
    todo!()
}

pub fn defined(name: &str) -> bool {
    todo!()
}

pub fn hash(algo: &str, data: &str) -> String {
    todo!()
}

pub fn extension_loaded(name: &str) -> bool {
    todo!()
}

pub fn gzopen(file: &str, mode: &str) -> PhpMixed {
    todo!()
}

pub fn gzread(file: PhpMixed, length: i64) -> String {
    todo!()
}

pub fn gzclose(file: PhpMixed) {
    todo!()
}

pub fn fopen(file: &str, mode: &str) -> PhpMixed {
    todo!()
}

pub fn fwrite(file: PhpMixed, data: &str, length: i64) {
    todo!()
}

pub fn fclose(file: PhpMixed) {
    todo!()
}

pub fn parse_url(url: &str, component: i64) -> PhpMixed {
    todo!()
}

pub fn pathinfo(path: PhpMixed, option: i64) -> PhpMixed {
    todo!()
}

pub fn strtr(str: &str, from: &str, to: &str) -> String {
    todo!()
}

pub fn implode(glue: &str, pieces: &[String]) -> String {
    todo!()
}

pub const PHP_URL_PATH: i64 = 5;
pub const PATHINFO_FILENAME: i64 = 64;
pub const DIRECTORY_SEPARATOR: &str = "/";

pub const HHVM_VERSION: Option<&str> = None;

#[derive(Debug)]
pub struct Phar {
    path: String,
}

impl Phar {
    pub fn new(a: String) -> Self {
        todo!()
    }

    pub fn extract_to(&self, a: &str, b: Option<()>, c: bool) {
        todo!()
    }
}

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
    pub fn new(a: String) -> Self {
        todo!()
    }

    pub fn valid(&self) -> bool {
        todo!()
    }

    pub fn get(&self, key: &str) -> Option<PharFileInfo> {
        todo!()
    }

    pub fn iter(&self) -> impl Iterator<Item = PharFileInfo> {
        todo!();
        std::iter::empty()
    }

    pub fn extract_to(&self, a: &str, b: Option<()>, c: bool) {
        todo!()
    }

    pub fn add_empty_dir(&self, a: &str) {
        todo!()
    }
}
