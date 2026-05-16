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

#[derive(Debug)]
pub struct LogicException {
    pub message: String,
    pub code: i64,
}

#[derive(Debug)]
pub struct BadMethodCallException {
    pub message: String,
    pub code: i64,
}

#[derive(Debug)]
pub struct OutOfBoundsException {
    pub message: String,
    pub code: i64,
}

#[derive(Debug)]
pub struct ErrorException {
    pub message: String,
    pub code: i64,
    pub severity: i64,
    pub filename: String,
    pub lineno: i64,
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

pub fn pack(format: &str, values: &[PhpMixed]) -> Vec<u8> {
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

pub fn fseek(stream: PhpMixed, offset: i64) -> i64 {
    todo!()
}

pub fn rewind(stream: PhpMixed) -> bool {
    todo!()
}

pub fn strip_tags(str: &str) -> String {
    todo!()
}

pub const PHP_EOL: &str = "\n";

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

pub fn parse_url_all(url: &str) -> PhpMixed {
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

pub fn microtime(get_as_float: bool) -> f64 {
    todo!()
}

pub fn error_reporting(level: Option<i64>) -> i64 {
    todo!()
}

pub const E_ALL: i64 = 32767;
pub const E_WARNING: i64 = 2;
pub const E_NOTICE: i64 = 8;
pub const E_USER_WARNING: i64 = 512;
pub const E_USER_NOTICE: i64 = 1024;
pub const E_DEPRECATED: i64 = 8192;
pub const E_USER_DEPRECATED: i64 = 16384;

pub const PHP_URL_HOST: i64 = 1;
pub const PHP_URL_PORT: i64 = 2;
pub const PHP_URL_PATH: i64 = 5;
pub const PATHINFO_FILENAME: i64 = 64;
pub const PATHINFO_EXTENSION: i64 = 4;
pub const PATHINFO_DIRNAME: i64 = 1;
pub const PATHINFO_BASENAME: i64 = 2;
pub const DIRECTORY_SEPARATOR: &str = "/";

pub const HHVM_VERSION: Option<&str> = None;

#[derive(Debug)]
pub struct Phar {
    path: String,
}

impl Phar {
    pub const ZIP: i64 = 1;
    pub const TAR: i64 = 2;
    pub const GZ: i64 = 4096;
    pub const BZ2: i64 = 8192;

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

    pub fn new_with_format(path: String, flags: i64, alias: &str, format: i64) -> Self {
        todo!()
    }

    pub fn can_compress(algo: i64) -> bool {
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

    pub fn build_from_iterator(&self, iter: &mut dyn Iterator<Item = std::path::PathBuf>, base: &str) {
        todo!()
    }

    pub fn compress(&self, algo: i64) {
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

    pub fn open(&mut self, filename: &str, flags: i64) -> Result<(), i64> {
        todo!()
    }

    pub fn close(&self) -> bool {
        todo!()
    }

    pub fn count(&self) -> i64 {
        todo!()
    }

    pub fn stat_index(&self, index: i64) -> Option<IndexMap<String, Box<PhpMixed>>> {
        todo!()
    }

    pub fn extract_to(&self, path: &str) -> bool {
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

    pub fn add_empty_dir(&self, local_name: &str) -> bool {
        todo!()
    }

    pub fn add_file(&self, filepath: &str, local_name: &str) -> bool {
        todo!()
    }

    pub fn set_external_attributes_name(&self, name: &str, opsys: i64, attr: i64) -> bool {
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

pub trait JsonSerializable {
    fn json_serialize(&self) -> PhpMixed;
}

pub trait Countable {
    fn count(&self) -> i64;
}

pub fn in_array(needle: PhpMixed, haystack: &PhpMixed, strict: bool) -> bool {
    todo!()
}

pub fn realpath(path: &str) -> Option<String> {
    todo!()
}

pub const JSON_UNESCAPED_UNICODE: i64 = 256;
pub const JSON_UNESCAPED_SLASHES: i64 = 64;
pub const JSON_PRETTY_PRINT: i64 = 128;
pub const JSON_THROW_ON_ERROR: i64 = 4194304;

pub fn json_encode(value: &PhpMixed) -> Option<String> {
    todo!()
}

pub fn preg_quote(str: &str, delimiter: Option<char>) -> String {
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

pub fn function_exists(name: &str) -> bool {
    todo!()
}

pub fn mb_convert_encoding(string: Vec<u8>, to_encoding: &str, from_encoding: &str) -> String {
    todo!()
}

pub fn touch(path: &str) -> bool {
    todo!()
}

pub fn chmod(path: &str, mode: u32) -> bool {
    todo!()
}

pub fn strpbrk(haystack: &str, char_list: &str) -> Option<String> {
    todo!()
}

pub fn rawurldecode(s: &str) -> String {
    todo!()
}

pub fn rawurlencode(s: &str) -> String {
    todo!()
}

pub fn base64_encode(data: &str) -> String {
    todo!()
}

pub fn base64_decode(data: &str) -> Option<Vec<u8>> {
    todo!()
}

pub fn substr_count(haystack: &str, needle: &str) -> i64 {
    todo!()
}

pub fn openssl_x509_parse(certificate: &str, short_names: bool) -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub fn openssl_get_publickey(certificate: &str) -> Option<PhpMixed> {
    todo!()
}

pub fn openssl_pkey_get_details(key: PhpMixed) -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub fn fileperms(path: &str) -> i64 {
    todo!()
}

pub const FILTER_VALIDATE_BOOLEAN: i64 = 258;
pub const FILTER_VALIDATE_URL: i64 = 273;

pub fn filter_var(value: &str, filter: i64) -> bool {
    todo!()
}

pub fn ini_get(option: &str) -> Option<String> {
    todo!()
}

pub fn set_error_handler(callback: fn(i64, &str, &str, i64) -> bool) {
    todo!()
}

pub fn debug_backtrace() -> Vec<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub const PHP_VERSION: &str = "8.1.0";

pub const STDERR: i64 = 2;

pub fn is_resource(value: &PhpMixed) -> bool {
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

pub fn array_fill_keys(keys: PhpMixed, value: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn array_merge(array1: PhpMixed, array2: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn substr_replace(string: &str, replace: &str, start: usize, length: usize) -> String {
    todo!()
}

pub fn constant(name: &str) -> PhpMixed {
    todo!()
}

pub fn get_loaded_extensions() -> Vec<String> {
    todo!()
}

pub fn phpversion(extension: &str) -> Option<String> {
    todo!()
}

pub fn ob_start() -> bool {
    todo!()
}

pub fn ob_get_clean() -> Option<String> {
    todo!()
}

pub fn html_entity_decode(s: &str) -> String {
    todo!()
}

pub fn hash_file(algo: &str, filename: &str) -> Option<String> {
    todo!()
}

pub fn filesize(path: &str) -> Option<i64> {
    todo!()
}

pub fn random_int(min: i64, max: i64) -> i64 {
    todo!()
}

pub fn json_encode_ex(value: &PhpMixed, flags: i64) -> Option<String> {
    todo!()
}

pub const JSON_INVALID_UTF8_IGNORE: i64 = 1048576;

pub fn is_array(value: &PhpMixed) -> bool {
    todo!()
}

pub fn strnatcasecmp(s1: &str, s2: &str) -> i64 {
    todo!()
}

pub fn file_exists(path: &str) -> bool {
    todo!()
}

pub fn is_writable(path: &str) -> bool {
    todo!()
}

pub fn unlink(path: &str) -> bool {
    todo!()
}

pub fn file_put_contents(path: &str, data: &[u8]) -> Option<i64> {
    todo!()
}

pub fn str_repeat(s: &str, count: usize) -> String {
    todo!()
}

pub fn strrpos(haystack: &str, needle: &str) -> Option<usize> {
    todo!()
}

pub fn gzcompress(data: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

pub fn bzcompress(data: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

pub fn getcwd() -> Option<String> {
    todo!()
}

pub fn chdir(path: &str) -> Result<()> {
    todo!()
}

pub fn glob(pattern: &str) -> Vec<String> {
    todo!()
}

pub fn basename(path: &str) -> String {
    todo!()
}

pub fn explode(delimiter: &str, string: &str) -> Vec<String> {
    todo!()
}

pub struct FilesystemIterator;

impl FilesystemIterator {
    pub const KEY_AS_PATHNAME: i64 = 256;
    pub const CURRENT_AS_FILEINFO: i64 = 0;
}

pub const CURLOPT_PROXY: i64 = 10004;
pub const CURLOPT_NOPROXY: i64 = 10177;
pub const CURLOPT_PROXYAUTH: i64 = 111;
pub const CURLOPT_PROXYUSERPWD: i64 = 10006;
pub const CURLAUTH_BASIC: i64 = 1;
pub const CURLOPT_PROXY_CAINFO: i64 = 246;
pub const CURLOPT_PROXY_CAPATH: i64 = 247;
pub const CURL_VERSION_HTTPS_PROXY: i64 = 2097152;

pub fn curl_version() -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub fn bin2hex(data: &[u8]) -> String {
    todo!()
}

pub fn random_bytes(length: usize) -> Vec<u8> {
    todo!()
}

pub fn is_dir(path: &str) -> bool {
    todo!()
}

pub fn file_get_contents(path: &str) -> Option<String> {
    todo!()
}

pub fn strtolower(s: &str) -> String {
    todo!()
}

pub fn array_intersect_key(array1: &IndexMap<String, Box<PhpMixed>>, array2: &IndexMap<String, Box<PhpMixed>>) -> IndexMap<String, Box<PhpMixed>> {
    todo!()
}

pub fn is_file(path: &str) -> bool {
    todo!()
}

pub fn spl_object_hash<T: ?Sized>(object: &T) -> String {
    todo!()
}

pub fn serialize(value: &PhpMixed) -> String {
    todo!()
}

pub fn stream_context_create(options: &IndexMap<String, PhpMixed>, params: Option<&IndexMap<String, PhpMixed>>) -> PhpMixed {
    todo!()
}

pub fn stripos(haystack: &str, needle: &str) -> Option<usize> {
    todo!()
}

pub fn php_uname(mode: &str) -> String {
    todo!()
}

pub fn uasort<F>(array: &mut Vec<String>, compare: F)
where
    F: Fn(&str, &str) -> i64,
{
    todo!()
}

pub fn array_replace_recursive(base: IndexMap<String, PhpMixed>, replacement: IndexMap<String, PhpMixed>) -> IndexMap<String, PhpMixed> {
    todo!()
}

pub const PHP_MAJOR_VERSION: i64 = 8;
pub const PHP_MINOR_VERSION: i64 = 1;
pub const PHP_RELEASE_VERSION: i64 = 0;

pub const GLOB_MARK: i64 = 8;
pub const GLOB_ONLYDIR: i64 = 1024;
pub const GLOB_BRACE: i64 = 4096;

pub fn glob_with_flags(pattern: &str, flags: i64) -> Vec<String> {
    todo!()
}

pub fn time() -> i64 {
    todo!()
}

pub fn date(format: &str, timestamp: Option<i64>) -> String {
    todo!()
}

pub fn trigger_error(message: &str, error_level: i64) {
    todo!()
}

pub fn sys_get_temp_dir() -> String {
    todo!()
}
