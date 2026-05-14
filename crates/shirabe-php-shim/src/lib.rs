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

pub fn is_bool(value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_string(value: &PhpMixed) -> bool {
    todo!()
}

pub fn empty(value: &PhpMixed) -> bool {
    todo!()
}

pub fn method_exists(object: &PhpMixed, method_name: &str) -> bool {
    todo!()
}

pub fn get_class(object: &PhpMixed) -> String {
    todo!()
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

pub fn hash_raw(algo: &str, data: &str) -> Vec<u8> {
    todo!()
}

pub fn unpack(format: &str, data: &[u8]) -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub const PHP_VERSION_ID: i64 = 80100;

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

pub fn version_compare(v1: &str, v2: &str, op: &str) -> bool {
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

#[derive(Debug)]
pub struct ZipArchive {
    pub num_files: i64,
}

impl ZipArchive {
    pub fn new() -> Self {
        todo!()
    }

    pub fn open(&mut self, filename: &str) -> bool {
        todo!()
    }

    pub fn close(&self) {
        todo!()
    }

    pub fn locate_name(&self, name: &str) -> Option<i64> {
        todo!()
    }

    pub fn get_from_index(&self, index: i64) -> Option<String> {
        todo!()
    }

    pub fn get_name_index(&self, index: i64) -> String {
        todo!()
    }

    pub fn get_stream(&self, name: &str) -> Option<PhpMixed> {
        todo!()
    }
}

pub trait JsonSerializable {
    fn json_serialize(&self) -> PhpMixed;
}

pub fn in_array(needle: PhpMixed, haystack: &PhpMixed, strict: bool) -> bool {
    todo!()
}

pub fn dirname(path: &str) -> String {
    todo!()
}

pub fn stream_get_contents(stream: PhpMixed) -> Option<String> {
    todo!()
}

pub fn class_exists(name: &str) -> bool {
    todo!()
}

#[derive(Debug)]
pub struct RarEntry;

impl RarEntry {
    pub fn extract(&self, path: &str) -> bool {
        todo!()
    }
}

pub fn var_export(_value: &PhpMixed, _return: bool) -> String {
    todo!()
}

#[derive(Debug)]
pub struct RarArchive;

impl RarArchive {
    pub fn open(file: &str) -> Option<Self> {
        todo!()
    }

    pub fn get_entries(&self) -> Option<Vec<RarEntry>> {
        todo!()
    }

    pub fn close(&self) {
        todo!()
    }
}
