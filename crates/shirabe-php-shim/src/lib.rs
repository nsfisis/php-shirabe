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

impl PhpMixed {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PhpMixed::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            PhpMixed::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            PhpMixed::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            PhpMixed::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<Box<PhpMixed>>> {
        match self {
            PhpMixed::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&IndexMap<String, Box<PhpMixed>>> {
        match self {
            PhpMixed::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, PhpMixed::Null)
    }
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

pub fn error_reporting(level: Option<i64>) -> i64 {
    todo!()
}

pub const E_WARNING: i64 = 2;
pub const E_NOTICE: i64 = 8;
pub const E_USER_WARNING: i64 = 512;
pub const E_USER_NOTICE: i64 = 1024;
pub const E_DEPRECATED: i64 = 8192;
pub const E_USER_DEPRECATED: i64 = 16384;

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
