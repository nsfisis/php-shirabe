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

impl std::fmt::Display for RuntimeException {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[derive(Debug)]
pub struct UnexpectedValueException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for UnexpectedValueException {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[derive(Debug)]
pub struct InvalidArgumentException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for InvalidArgumentException {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[derive(Debug)]
pub struct LogicException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for LogicException {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[derive(Debug)]
pub struct BadMethodCallException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for BadMethodCallException {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[derive(Debug)]
pub struct OutOfBoundsException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for OutOfBoundsException {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

#[derive(Debug)]
pub struct ErrorException {
    pub message: String,
    pub code: i64,
    pub severity: i64,
    pub filename: String,
    pub lineno: i64,
}

impl std::fmt::Display for ErrorException {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

pub fn is_bool(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_string(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_int(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_scalar(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_numeric(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn strtotime(_time: &str) -> Option<i64> {
    todo!()
}

pub fn strcasecmp(_s1: &str, _s2: &str) -> i64 {
    todo!()
}

pub fn sprintf(_format: &str, _args: &[PhpMixed]) -> String {
    todo!()
}

pub fn array_values<V: Clone>(_array: &IndexMap<String, V>) -> Vec<V> {
    todo!()
}

pub fn array_keys<V>(_array: &IndexMap<String, V>) -> Vec<String> {
    todo!()
}

pub fn str_replace(_search: &str, _replace: &str, _subject: &str) -> String {
    todo!()
}

pub fn php_to_string(_value: &PhpMixed) -> String {
    todo!()
}

pub fn substr(_s: &str, _start: i64, _length: Option<i64>) -> String {
    todo!()
}

pub const FILTER_VALIDATE_EMAIL: i64 = 274;

pub const PATH_SEPARATOR: &str = ":";

pub fn spl_autoload_functions() -> Vec<PhpMixed> {
    todo!()
}

pub fn array_push(_array: &mut Vec<String>, _value: String) -> i64 {
    todo!()
}

pub fn array_search_in_vec(_needle: &str, _haystack: &[String]) -> Option<usize> {
    todo!()
}

pub fn array_map_str_fn<F: Fn(&str) -> String>(_callback: F, _array: &[String]) -> Vec<String> {
    todo!()
}

pub fn is_callable(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_object(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_a(_object_or_class: &PhpMixed, _class: &str, _allow_string: bool) -> bool {
    todo!()
}

pub fn str_contains(_haystack: &str, _needle: &str) -> bool {
    todo!()
}

pub fn str_starts_with(_haystack: &str, _needle: &str) -> bool {
    todo!()
}

pub fn str_ends_with(_haystack: &str, _needle: &str) -> bool {
    todo!()
}

pub fn strpos(_haystack: &str, _needle: &str) -> Option<usize> {
    todo!()
}

pub fn strtoupper(_s: &str) -> String {
    todo!()
}

pub fn strlen(_s: &str) -> i64 {
    todo!()
}

pub fn krsort<V>(_array: &mut IndexMap<i64, V>) {
    todo!()
}

pub fn max_i64(_a: i64, _b: i64) -> i64 {
    todo!()
}

pub fn count_mixed(_value: &PhpMixed) -> i64 {
    todo!()
}

pub fn array_slice_mixed(_value: &PhpMixed, _offset: i64, _length: Option<i64>) -> PhpMixed {
    todo!()
}

pub fn array_slice_strs(_value: &[String], _offset: i64, _length: Option<i64>) -> Vec<String> {
    todo!()
}

pub fn empty(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn method_exists(_object: &PhpMixed, _method_name: &str) -> bool {
    todo!()
}

pub fn get_class(_object: &PhpMixed) -> String {
    todo!()
}

pub fn get_debug_type(_value: &PhpMixed) -> String {
    todo!()
}

pub fn defined(_name: &str) -> bool {
    todo!()
}

pub fn hash(_algo: &str, _data: &str) -> String {
    todo!()
}

pub fn hash_raw(_algo: &str, _data: &str) -> Vec<u8> {
    todo!()
}

pub fn pack(_format: &str, _values: &[PhpMixed]) -> Vec<u8> {
    todo!()
}

pub fn unpack(_format: &str, _data: &[u8]) -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub const PHP_VERSION_ID: i64 = 80100;

pub fn extension_loaded(_name: &str) -> bool {
    todo!()
}

pub fn gzopen(_file: &str, _mode: &str) -> PhpMixed {
    todo!()
}

pub fn gzread(_file: PhpMixed, _length: i64) -> String {
    todo!()
}

pub fn gzclose(_file: PhpMixed) {
    todo!()
}

pub fn fseek(_stream: PhpMixed, _offset: i64) -> i64 {
    todo!()
}

pub fn rewind(_stream: PhpMixed) -> bool {
    todo!()
}

pub fn strip_tags(_str: &str) -> String {
    todo!()
}

pub const PHP_EOL: &str = "\n";

pub fn fopen(_file: &str, _mode: &str) -> PhpMixed {
    todo!()
}

pub fn fwrite(_file: PhpMixed, _data: &str, _length: i64) {
    todo!()
}

pub fn fclose(_file: PhpMixed) {
    todo!()
}

pub fn parse_url(_url: &str, _component: i64) -> PhpMixed {
    todo!()
}

pub fn parse_url_all(_url: &str) -> PhpMixed {
    todo!()
}

pub fn pathinfo(_path: PhpMixed, _option: i64) -> PhpMixed {
    todo!()
}

pub fn strtr(_str: &str, _from: &str, _to: &str) -> String {
    todo!()
}

pub fn implode(_glue: &str, _pieces: &[String]) -> String {
    todo!()
}

pub fn version_compare(_v1: &str, _v2: &str, _op: &str) -> bool {
    todo!()
}

pub fn version_compare_2(_v1: &str, _v2: &str) -> i64 {
    todo!()
}

pub fn microtime(_get_as_float: bool) -> f64 {
    todo!()
}

pub fn error_reporting(_level: Option<i64>) -> i64 {
    todo!()
}

pub const E_ALL: i64 = 32767;
pub const E_WARNING: i64 = 2;
pub const E_NOTICE: i64 = 8;
pub const E_USER_WARNING: i64 = 512;
pub const E_USER_NOTICE: i64 = 1024;
pub const E_DEPRECATED: i64 = 8192;
pub const E_USER_DEPRECATED: i64 = 16384;

pub const PHP_URL_SCHEME: i64 = 0;
pub const PHP_URL_HOST: i64 = 1;
pub const PHP_URL_PORT: i64 = 2;
pub const PHP_URL_USER: i64 = 3;
pub const PHP_URL_PASS: i64 = 4;
pub const PHP_URL_PATH: i64 = 5;
pub const PHP_URL_QUERY: i64 = 6;
pub const PHP_URL_FRAGMENT: i64 = 7;
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

#[derive(Debug)]
pub struct PharException {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for PharException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
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

    pub fn stat_index(&self, _index: i64) -> Option<IndexMap<String, Box<PhpMixed>>> {
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

pub trait JsonSerializable {
    fn json_serialize(&self) -> PhpMixed;
}

pub trait Countable {
    fn count(&self) -> i64;
}

pub fn in_array(_needle: PhpMixed, _haystack: &PhpMixed, _strict: bool) -> bool {
    todo!()
}

pub fn realpath(_path: &str) -> Option<String> {
    todo!()
}

pub const JSON_UNESCAPED_UNICODE: i64 = 256;
pub const JSON_UNESCAPED_SLASHES: i64 = 64;
pub const JSON_PRETTY_PRINT: i64 = 128;
pub const JSON_THROW_ON_ERROR: i64 = 4194304;

pub fn json_encode(_value: &PhpMixed) -> Option<String> {
    todo!()
}

pub fn preg_quote(_str: &str, _delimiter: Option<char>) -> String {
    todo!()
}

// Returns 1 on match, 0 on no match; populates matches[0]=full match, matches[1..]=captures.
// Optional groups that did not participate in the match are stored as None.
pub fn preg_match(_pattern: &str, _subject: &str, _matches: &mut Vec<Option<String>>) -> i64 {
    todo!()
}

// Returns Some(result) on success, None on error.
pub fn preg_replace(_pattern: &str, _replacement: &str, _subject: &str) -> Option<String> {
    todo!()
}

// Returns Some(parts) on success, None on error.
pub fn preg_split(_pattern: &str, _subject: &str) -> Option<Vec<String>> {
    todo!()
}

pub fn dirname(_path: &str) -> String {
    todo!()
}

pub fn stream_get_contents(_stream: PhpMixed) -> Option<String> {
    todo!()
}

pub fn class_exists(_name: &str) -> bool {
    todo!()
}

pub fn function_exists(_name: &str) -> bool {
    todo!()
}

pub fn mb_convert_encoding(_string: Vec<u8>, _to_encoding: &str, _from_encoding: &str) -> String {
    todo!()
}

pub fn touch(_path: &str) -> bool {
    todo!()
}

pub fn chmod(_path: &str, _mode: u32) -> bool {
    todo!()
}

pub fn strpbrk(_haystack: &str, _char_list: &str) -> Option<String> {
    todo!()
}

pub fn rawurldecode(_s: &str) -> String {
    todo!()
}

pub fn rawurlencode(_s: &str) -> String {
    todo!()
}

pub fn urlencode(_s: &str) -> String {
    todo!()
}

pub fn base64_encode(_data: &str) -> String {
    todo!()
}

pub fn base64_decode(_data: &str) -> Option<Vec<u8>> {
    todo!()
}

pub fn substr_count(_haystack: &str, _needle: &str) -> i64 {
    todo!()
}

pub fn openssl_x509_parse(
    _certificate: &str,
    _short_names: bool,
) -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub fn openssl_get_publickey(_certificate: &str) -> Option<PhpMixed> {
    todo!()
}

pub fn openssl_pkey_get_details(_key: PhpMixed) -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub fn fileperms(_path: &str) -> i64 {
    todo!()
}

pub const FILTER_VALIDATE_BOOLEAN: i64 = 258;
pub const FILTER_VALIDATE_URL: i64 = 273;
pub const FILTER_VALIDATE_IP: i64 = 275;
pub const FILTER_VALIDATE_INT: i64 = 257;

pub fn filter_var(_value: &str, _filter: i64) -> bool {
    todo!()
}

pub fn ini_get(option: &str) -> Option<String> {
    let _ = option;
    todo!()
}

pub fn apcu_add(key: &str, var: PhpMixed) -> bool {
    let _ = (key, var);
    todo!()
}

pub fn apcu_fetch(key: &str, success: &mut bool) -> PhpMixed {
    let _ = (key, success);
    todo!()
}

pub fn spl_autoload_register(
    callback: Box<dyn Fn(&str) -> PhpMixed + Send + Sync>,
    throw: bool,
    prepend: bool,
) -> bool {
    let _ = (callback, throw, prepend);
    todo!()
}

pub fn spl_autoload_unregister(callback: Box<dyn Fn(&str) -> PhpMixed + Send + Sync>) -> bool {
    let _ = callback;
    todo!()
}

pub fn stream_resolve_include_path(filename: &str) -> Option<String> {
    let _ = filename;
    todo!()
}

/// Equivalent to PHP `include $file;`
pub fn include_file(file: &str) -> PhpMixed {
    let _ = file;
    todo!()
}

pub fn set_error_handler(_callback: fn(i64, &str, &str, i64) -> bool) {
    todo!()
}

pub fn debug_backtrace() -> Vec<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub const PHP_VERSION: &str = "8.1.0";

pub const STDERR: i64 = 2;

pub fn is_resource(_value: &PhpMixed) -> bool {
    todo!()
}

#[derive(Debug)]
pub struct RarEntry;

impl RarEntry {
    pub fn extract(&self, _path: &str) -> bool {
        todo!()
    }
}

pub fn var_export(_value: &PhpMixed, _return: bool) -> String {
    todo!()
}

pub fn var_export_str(_value: &str, _return: bool) -> String {
    todo!()
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

pub fn array_fill_keys(_keys: PhpMixed, _value: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn array_merge(_array1: PhpMixed, _array2: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn substr_replace(_string: &str, _replace: &str, _start: usize, _length: usize) -> String {
    todo!()
}

pub fn constant(_name: &str) -> PhpMixed {
    todo!()
}

pub fn get_loaded_extensions() -> Vec<String> {
    todo!()
}

pub fn phpversion(_extension: &str) -> Option<String> {
    todo!()
}

pub fn ob_start() -> bool {
    todo!()
}

pub fn ob_get_clean() -> Option<String> {
    todo!()
}

pub fn html_entity_decode(_s: &str) -> String {
    todo!()
}

pub fn hash_file(_algo: &str, _filename: &str) -> Option<String> {
    todo!()
}

pub fn filesize(_path: &str) -> Option<i64> {
    todo!()
}

pub fn random_int(_min: i64, _max: i64) -> i64 {
    todo!()
}

pub fn json_encode_ex(_value: &PhpMixed, _flags: i64) -> Option<String> {
    todo!()
}

pub const JSON_INVALID_UTF8_IGNORE: i64 = 1048576;

pub fn is_array(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn strnatcasecmp(_s1: &str, _s2: &str) -> i64 {
    todo!()
}

pub fn file_exists(_path: &str) -> bool {
    todo!()
}

pub fn is_writable(_path: &str) -> bool {
    todo!()
}

pub fn unlink(_path: &str) -> bool {
    todo!()
}

pub fn file_put_contents(_path: &str, _data: &[u8]) -> Option<i64> {
    todo!()
}

pub fn str_repeat(_s: &str, _count: usize) -> String {
    todo!()
}

pub fn strrpos(_haystack: &str, _needle: &str) -> Option<usize> {
    todo!()
}

pub fn gzcompress(_data: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

pub fn bzcompress(_data: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

pub fn getcwd() -> Option<String> {
    todo!()
}

pub fn chdir(_path: &str) -> anyhow::Result<()> {
    todo!()
}

pub fn glob(_pattern: &str) -> Vec<String> {
    todo!()
}

pub fn basename(_path: &str) -> String {
    todo!()
}

pub fn explode(_delimiter: &str, _string: &str) -> Vec<String> {
    todo!()
}

pub fn explode_with_limit(delimiter: &str, string: &str, limit: i64) -> Vec<String> {
    let _ = (delimiter, string, limit);
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

pub const CURLM_OK: i64 = 0;
pub const CURLM_BAD_HANDLE: i64 = 1;
pub const CURLM_BAD_EASY_HANDLE: i64 = 2;
pub const CURLM_OUT_OF_MEMORY: i64 = 3;
pub const CURLM_INTERNAL_ERROR: i64 = 4;
pub const CURLM_CALL_MULTI_PERFORM: i64 = -1;

pub const CURLMOPT_PIPELINING: i64 = 3;
pub const CURLMOPT_MAX_HOST_CONNECTIONS: i64 = 7;

pub const CURLSHOPT_SHARE: i64 = 1;
pub const CURL_LOCK_DATA_COOKIE: i64 = 2;
pub const CURL_LOCK_DATA_DNS: i64 = 3;
pub const CURL_LOCK_DATA_SSL_SESSION: i64 = 4;

pub const CURLOPT_URL: i64 = 10002;
pub const CURLOPT_FOLLOWLOCATION: i64 = 52;
pub const CURLOPT_CONNECTTIMEOUT: i64 = 78;
pub const CURLOPT_TIMEOUT: i64 = 13;
pub const CURLOPT_WRITEHEADER: i64 = 10029;
pub const CURLOPT_FILE: i64 = 10001;
pub const CURLOPT_ENCODING: i64 = 10102;
pub const CURLOPT_PROTOCOLS: i64 = 181;
pub const CURLOPT_CUSTOMREQUEST: i64 = 10036;
pub const CURLOPT_POSTFIELDS: i64 = 10015;
pub const CURLOPT_HTTPHEADER: i64 = 10023;
pub const CURLOPT_CAINFO: i64 = 10065;
pub const CURLOPT_CAPATH: i64 = 10097;
pub const CURLOPT_SSL_VERIFYPEER: i64 = 64;
pub const CURLOPT_SSL_VERIFYHOST: i64 = 81;
pub const CURLOPT_SSLCERT: i64 = 10025;
pub const CURLOPT_SSLKEY: i64 = 10087;
pub const CURLOPT_SSLKEYPASSWD: i64 = 10026;
pub const CURLOPT_IPRESOLVE: i64 = 113;
pub const CURLOPT_SHARE: i64 = 10100;
pub const CURLOPT_HTTP_VERSION: i64 = 84;

pub const CURLPROTO_HTTP: i64 = 1;
pub const CURLPROTO_HTTPS: i64 = 2;

pub const CURL_IPRESOLVE_V4: i64 = 1;
pub const CURL_IPRESOLVE_V6: i64 = 2;

pub const CURL_HTTP_VERSION_2_0: i64 = 3;
pub const CURL_HTTP_VERSION_3: i64 = 30;

pub const CURL_VERSION_HTTP2: i64 = 65536;
pub const CURL_VERSION_HTTP3: i64 = 33554432;
pub const CURL_VERSION_LIBZ: i64 = 8;

pub const CURLE_OK: i64 = 0;
pub const CURLE_OPERATION_TIMEDOUT: i64 = 28;

#[derive(Debug)]
pub struct CurlHandle;

#[derive(Debug)]
pub struct CurlMultiHandle;

#[derive(Debug)]
pub struct CurlShareHandle;

pub fn curl_version() -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub fn curl_init() -> CurlHandle {
    todo!()
}

pub fn curl_close(_handle: CurlHandle) {
    todo!()
}

pub fn curl_setopt(_handle: &CurlHandle, _option: i64, _value: PhpMixed) -> bool {
    todo!()
}

pub fn curl_setopt_array(_handle: &CurlHandle, _options: &IndexMap<i64, PhpMixed>) -> bool {
    todo!()
}

pub fn curl_getinfo(_handle: &CurlHandle) -> PhpMixed {
    todo!()
}

pub fn curl_error(_handle: &CurlHandle) -> String {
    todo!()
}

pub fn curl_errno(_handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn curl_strerror(_errornum: i64) -> Option<String> {
    todo!()
}

pub fn curl_multi_init() -> CurlMultiHandle {
    todo!()
}

pub fn curl_multi_setopt(_mh: &CurlMultiHandle, _option: i64, _value: PhpMixed) -> bool {
    todo!()
}

pub fn curl_multi_add_handle(_mh: &CurlMultiHandle, _handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn curl_multi_remove_handle(_mh: &CurlMultiHandle, _handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn curl_multi_exec(_mh: &CurlMultiHandle, _still_running: &mut bool) -> i64 {
    todo!()
}

pub fn curl_multi_select(_mh: &CurlMultiHandle, _timeout: f64) -> i64 {
    todo!()
}

pub fn curl_multi_info_read(_mh: &CurlMultiHandle) -> PhpMixed {
    todo!()
}

pub fn curl_share_init() -> CurlShareHandle {
    todo!()
}

pub fn curl_share_setopt(_sh: &CurlShareHandle, _option: i64, _value: PhpMixed) -> bool {
    todo!()
}

/// Cast a `\CurlHandle` to int (its spl_object_id) as `(int) $curlHandle` in PHP.
pub fn curl_handle_id(_handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn restore_error_handler() {
    todo!()
}

/// Closure-capturing variant of PHP `set_error_handler()`.
pub fn set_error_handler_closure(callback: Box<dyn FnMut(i64, &str) -> bool>) {
    let _ = callback;
    todo!()
}

pub fn stream_get_contents_with_max(stream: PhpMixed, max_length: Option<i64>) -> Option<String> {
    let _ = (stream, max_length);
    todo!()
}

pub fn bin2hex(_data: &[u8]) -> String {
    todo!()
}

pub fn random_bytes(_length: usize) -> Vec<u8> {
    todo!()
}

pub fn is_dir(_path: &str) -> bool {
    todo!()
}

pub fn file_get_contents(_path: &str) -> Option<String> {
    todo!()
}

pub fn strtolower(_s: &str) -> String {
    todo!()
}

pub fn ctype_alnum(_s: &str) -> bool {
    todo!()
}

pub fn ord(_c: &str) -> i64 {
    todo!()
}

pub fn gethostname() -> String {
    todo!()
}

pub fn feof(_stream: PhpMixed) -> bool {
    todo!()
}

pub fn str_replace_array(_search: &[String], _replace: &[String], _subject: &str) -> String {
    todo!()
}

pub fn file(_filename: &str, _flags: i64) -> Option<Vec<String>> {
    todo!()
}

pub fn ucwords(_s: &str) -> String {
    todo!()
}

pub fn get_current_user() -> String {
    todo!()
}

pub const FILE_IGNORE_NEW_LINES: i64 = 2;

pub fn array_diff(_array1: &[String], _array2: &[String]) -> Vec<String> {
    todo!()
}

pub fn copy(_source: &str, _dest: &str) -> bool {
    todo!()
}

pub fn exec(
    _command: &str,
    _output: Option<&mut Vec<String>>,
    _exit_code: Option<&mut i64>,
) -> Option<String> {
    todo!()
}

pub fn tempnam(_dir: &str, _prefix: &str) -> Option<String> {
    todo!()
}

pub fn openssl_verify(
    _data: &str,
    _signature: &[u8],
    _pub_key_id: PhpMixed,
    _algorithm: PhpMixed,
) -> i64 {
    todo!()
}

pub fn openssl_pkey_get_public(_public_key: &str) -> PhpMixed {
    todo!()
}

pub fn openssl_get_md_methods() -> Vec<String> {
    todo!()
}

pub fn openssl_free_key(_key: PhpMixed) {
    todo!()
}

pub fn iterator_to_array<T>(_iter: T) -> Vec<PhpMixed>
where
    T: IntoIterator<Item = PhpMixed>,
{
    todo!()
}

pub fn end_arr<V: Clone>(_array: &IndexMap<String, V>) -> Option<V> {
    todo!()
}

pub fn fileowner(_filename: &str) -> Option<i64> {
    todo!()
}

pub fn unlink_silent(_path: &str) -> bool {
    todo!()
}

pub fn array_unique<T: Clone>(_array: &[T]) -> Vec<T> {
    todo!()
}

pub fn current(_value: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn key(_value: PhpMixed) -> Option<String> {
    todo!()
}

pub fn reset<T: Clone>(_array: &[T]) -> Option<T> {
    todo!()
}

pub const OPENSSL_ALGO_SHA384: i64 = 9;

pub fn array_intersect_key(
    _array1: &IndexMap<String, Box<PhpMixed>>,
    _array2: &IndexMap<String, Box<PhpMixed>>,
) -> IndexMap<String, Box<PhpMixed>> {
    todo!()
}

pub fn is_file(_path: &str) -> bool {
    todo!()
}

pub fn spl_object_hash<T: ?Sized>(_object: &T) -> String {
    todo!()
}

pub fn serialize(_value: &PhpMixed) -> String {
    todo!()
}

pub fn stream_context_create(
    _options: &IndexMap<String, PhpMixed>,
    _params: Option<&IndexMap<String, PhpMixed>>,
) -> PhpMixed {
    todo!()
}

pub fn stripos(_haystack: &str, _needle: &str) -> Option<usize> {
    todo!()
}

pub fn php_uname(_mode: &str) -> String {
    todo!()
}

pub fn uasort<F>(_array: &mut Vec<String>, _compare: F)
where
    F: Fn(&str, &str) -> i64,
{
    todo!()
}

pub fn array_replace_recursive(
    _base: IndexMap<String, PhpMixed>,
    _replacement: IndexMap<String, PhpMixed>,
) -> IndexMap<String, PhpMixed> {
    todo!()
}

pub const PHP_MAJOR_VERSION: i64 = 8;
pub const PHP_MINOR_VERSION: i64 = 1;
pub const PHP_RELEASE_VERSION: i64 = 0;

pub const PHP_WINDOWS_VERSION_MAJOR: i64 = 0;
pub const PHP_WINDOWS_VERSION_MINOR: i64 = 0;

pub const GLOB_MARK: i64 = 8;
pub const GLOB_ONLYDIR: i64 = 1024;
pub const GLOB_BRACE: i64 = 4096;

pub fn glob_with_flags(_pattern: &str, _flags: i64) -> Vec<String> {
    todo!()
}

pub fn time() -> i64 {
    todo!()
}

pub fn date(_format: &str, _timestamp: Option<i64>) -> String {
    todo!()
}

pub fn trigger_error(_message: &str, _error_level: i64) {
    todo!()
}

pub fn sys_get_temp_dir() -> String {
    todo!()
}

pub fn json_decode(_s: &str, _assoc: bool) -> anyhow::Result<PhpMixed> {
    todo!()
}

pub fn http_build_query_mixed(
    data: &IndexMap<String, PhpMixed>,
    numeric_prefix: &str,
    arg_separator: &str,
) -> String {
    let _ = (data, numeric_prefix, arg_separator);
    todo!()
}

pub fn http_build_query(_data: &[(&str, &str)], _sep_str: &str, _sep: &str) -> String {
    todo!()
}

pub fn token_get_all(_source: &str) -> Vec<PhpMixed> {
    todo!()
}

pub const T_COMMENT: i64 = 315;
pub const T_DOC_COMMENT: i64 = 316;
pub const T_WHITESPACE: i64 = 317;

pub fn dirname_levels(_path: &str, _levels: i64) -> String {
    todo!()
}

pub fn strtr_array(_s: &str, _pairs: &IndexMap<String, String>) -> String {
    todo!()
}

pub fn array_search_mixed(
    needle: &PhpMixed,
    haystack: &PhpMixed,
    strict: bool,
) -> Option<PhpMixed> {
    let _ = (needle, haystack, strict);
    todo!()
}

pub fn array_search(_needle: &str, _haystack: &IndexMap<String, String>) -> Option<String> {
    todo!()
}

pub fn strcmp(_s1: &str, _s2: &str) -> i64 {
    todo!()
}

pub fn rtrim(_s: &str, _chars: Option<&str>) -> String {
    todo!()
}

pub fn rmdir(_dir: &str) -> bool {
    todo!()
}

pub fn is_link(_path: &str) -> bool {
    todo!()
}

pub fn str_pad(_input: &str, _length: usize, _pad_string: &str, _pad_type: i64) -> String {
    todo!()
}

pub const STR_PAD_LEFT: i64 = 0;
pub const STR_PAD_RIGHT: i64 = 1;
pub const STR_PAD_BOTH: i64 = 2;

pub fn abs(_value: i64) -> i64 {
    todo!()
}

pub const DATE_ATOM: &str = "Y-m-d\\TH:i:sP";

pub fn ucfirst(_s: &str) -> String {
    todo!()
}

pub fn strval(_value: &PhpMixed) -> String {
    todo!()
}

pub fn usleep(_microseconds: u64) {
    todo!()
}

pub fn mb_strlen(_s: &str, _encoding: &str) -> i64 {
    todo!()
}

pub fn stream_isatty(_stream: PhpMixed) -> bool {
    todo!()
}

pub fn posix_getuid() -> i64 {
    todo!()
}

pub fn posix_geteuid() -> i64 {
    todo!()
}

pub fn posix_getpwuid(_uid: i64) -> PhpMixed {
    todo!()
}

pub fn posix_isatty(_stream: PhpMixed) -> bool {
    todo!()
}

pub fn fstat(_stream: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn getenv(_name: &str) -> Option<String> {
    todo!()
}

pub fn putenv(_setting: &str) -> bool {
    todo!()
}

/// PHP superglobal $_SERVER access
pub fn server_get(_name: &str) -> Option<String> {
    todo!()
}

pub fn server_set(_name: &str, _value: String) {
    todo!()
}

pub fn server_unset(_name: &str) {
    todo!()
}

pub fn server_contains_key(_name: &str) -> bool {
    todo!()
}

pub fn server_argv() -> Vec<String> {
    todo!()
}

/// PHP superglobal $_ENV access
pub fn env_get(_name: &str) -> Option<String> {
    todo!()
}

pub fn env_set(_name: &str, _value: String) {
    todo!()
}

pub fn env_unset(_name: &str) {
    todo!()
}

pub fn env_contains_key(_name: &str) -> bool {
    todo!()
}

pub fn trim(_s: &str, _chars: Option<&str>) -> String {
    todo!()
}

pub fn count(_value: &PhpMixed) -> i64 {
    todo!()
}

pub fn array_shift<T>(_array: &mut Vec<T>) -> Option<T> {
    todo!()
}

pub fn array_pop<T>(_array: &mut Vec<T>) -> Option<T> {
    todo!()
}

pub fn array_unshift<T>(_array: &mut Vec<T>, _value: T) {
    todo!()
}

pub fn array_reverse<T: Clone>(_array: &[T], _preserve_keys: bool) -> Vec<T> {
    todo!()
}

pub fn array_filter<T: Clone, F>(_array: &[T], _callback: F) -> Vec<T>
where
    F: Fn(&T) -> bool,
{
    todo!()
}

pub fn array_all<T, F>(_array: &[T], _callback: F) -> bool
where
    F: Fn(&T) -> bool,
{
    todo!()
}

pub fn array_any<T, F>(_array: &[T], _callback: F) -> bool
where
    F: Fn(&T) -> bool,
{
    todo!()
}

pub fn array_reduce<T, U, F>(_array: &[T], _callback: F, _initial: U) -> U
where
    F: Fn(U, &T) -> U,
{
    todo!()
}

pub fn array_intersect<T: Clone + PartialEq>(_array1: &[T], _array2: &[T]) -> Vec<T> {
    todo!()
}

pub fn mkdir(_pathname: &str, _mode: u32, _recursive: bool) -> bool {
    todo!()
}

pub fn rename(_old_name: &str, _new_name: &str) -> bool {
    todo!()
}

pub fn clearstatcache() {
    todo!()
}

pub fn disk_free_space(_directory: &str) -> Option<f64> {
    todo!()
}

pub fn filemtime(_filename: &str) -> Option<i64> {
    todo!()
}

/// Equivalent to PHP's __DIR__ magic constant
pub fn php_dir() -> String {
    todo!()
}

/// Equivalent to PHP's `require <file>` returning the file's return value
pub fn require_php_file(_filename: &str) -> PhpMixed {
    todo!()
}

pub fn array_flip(_array: &PhpMixed) -> PhpMixed {
    todo!()
}

pub fn max(_a: i64, _b: i64) -> i64 {
    todo!()
}

pub fn array_key_exists<V>(_key: &str, _array: &IndexMap<String, V>) -> bool {
    todo!()
}

pub fn fgets(_handle: PhpMixed) -> Option<String> {
    todo!()
}

pub fn umask() -> u32 {
    todo!()
}

pub fn basename_with_suffix(_path: &str, _suffix: &str) -> String {
    todo!()
}

pub fn inet_pton(_host: &str) -> Option<Vec<u8>> {
    todo!()
}

pub fn ltrim(_s: &str, _chars: Option<&str>) -> String {
    todo!()
}

pub fn floor(_value: f64) -> f64 {
    todo!()
}

pub fn chr(_value: u8) -> String {
    todo!()
}

pub fn filter_var_with_options(
    _value: &str,
    _filter: i64,
    _options: &IndexMap<String, PhpMixed>,
) -> PhpMixed {
    todo!()
}

pub fn memory_get_usage() -> i64 {
    todo!()
}

pub fn mb_check_encoding(_value: &str, _encoding: &str) -> bool {
    todo!()
}

pub fn iconv(_in_charset: &str, _out_charset: &str, _string: &str) -> Option<String> {
    todo!()
}

pub const JSON_ERROR_NONE: i64 = 0;
pub const JSON_ERROR_DEPTH: i64 = 1;
pub const JSON_ERROR_STATE_MISMATCH: i64 = 2;
pub const JSON_ERROR_CTRL_CHAR: i64 = 3;
pub const JSON_ERROR_UTF8: i64 = 5;

pub fn json_last_error() -> i64 {
    todo!()
}

pub fn sort<T: Ord>(_array: &mut Vec<T>) {
    todo!()
}

pub fn sort_with_flags<T: Ord>(_array: &mut Vec<T>, _flags: i64) {
    todo!()
}

pub const SORT_REGULAR: i64 = 0;
pub const SORT_NUMERIC: i64 = 1;
pub const SORT_STRING: i64 = 2;
pub const SORT_NATURAL: i64 = 6;
pub const SORT_FLAG_CASE: i64 = 8;

pub fn usort<T, F>(_array: &mut Vec<T>, _compare: F)
where
    F: FnMut(&T, &T) -> i64,
{
    todo!()
}

pub fn ksort<V>(_array: &mut IndexMap<String, V>) {
    todo!()
}

pub fn is_null(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn r#eval(_code: &str) -> PhpMixed {
    todo!()
}

pub fn array_is_list(_array: &PhpMixed) -> bool {
    todo!()
}

pub fn array_splice<T>(
    _array: &mut Vec<T>,
    _offset: i64,
    _length: Option<i64>,
    _replacement: Vec<T>,
) -> Vec<T> {
    todo!()
}

pub fn array_pop_first<T>(_array: &mut Vec<T>) -> Option<T> {
    todo!()
}

pub fn reset_first<T: Clone>(_array: &[T]) -> Option<T> {
    todo!()
}

pub fn call_user_func<T>(_callback: &str, _args: &[PhpMixed]) -> T
where
    T: From<PhpMixed>,
{
    todo!()
}

pub fn array_merge_recursive(_arrays: Vec<PhpMixed>) -> PhpMixed {
    todo!()
}

pub fn levenshtein(_string1: &str, _string2: &str) -> i64 {
    todo!()
}

pub fn array_slice<V: Clone>(
    _array: &IndexMap<String, V>,
    _offset: i64,
    _length: Option<i64>,
) -> IndexMap<String, V> {
    todo!()
}

pub fn asort<V: Ord>(_array: &mut IndexMap<String, V>) {
    todo!()
}

pub const PHP_INT_MAX: i64 = i64::MAX;
pub const PHP_INT_MIN: i64 = i64::MIN;
pub const PHP_INT_SIZE: i64 = 8;

pub fn call_user_func_array(_callback: &str, _args: &PhpMixed) -> PhpMixed {
    todo!()
}

pub fn array_map<T, U, F>(_callback: F, _array: &[T]) -> Vec<U>
where
    F: Fn(&T) -> U,
{
    todo!()
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

pub fn php_strip_whitespace(_path: &str) -> String {
    todo!()
}

pub fn error_get_last() -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub fn is_readable(_path: &str) -> bool {
    todo!()
}

pub fn stream_get_wrappers() -> Vec<String> {
    todo!()
}

pub fn php_require(_file: &str) -> PhpMixed {
    todo!()
}

pub fn intval(_value: &PhpMixed) -> i64 {
    todo!()
}

#[derive(Debug)]
pub struct RecursiveDirectoryIterator;

impl RecursiveDirectoryIterator {
    pub const SKIP_DOTS: i64 = 4096;
    pub const FOLLOW_SYMLINKS: i64 = 512;
}

#[derive(Debug)]
pub struct RecursiveIteratorIterator;

impl RecursiveIteratorIterator {
    pub const SELF_FIRST: i64 = 0;
    pub const CHILD_FIRST: i64 = 16;
}

pub fn recursive_directory_iterator(_path: &str, _flags: i64) -> RecursiveDirectoryIterator {
    todo!()
}

pub fn recursive_iterator_iterator(
    _iter: RecursiveDirectoryIterator,
    _mode: i64,
) -> RecursiveIteratorIterator {
    todo!()
}

pub fn globals_get(_name: &str) -> PhpMixed {
    todo!()
}

pub fn globals_set(_name: &str, _value: PhpMixed) {
    todo!()
}

pub fn clone<T: Clone>(_value: T) -> T {
    todo!()
}

pub fn date_default_timezone_get() -> String {
    todo!()
}

pub fn date_default_timezone_set(_tz: &str) -> bool {
    todo!()
}

pub fn getmypid() -> i64 {
    todo!()
}

pub fn ini_set(_varname: &str, _value: &str) -> Option<String> {
    todo!()
}

pub fn is_subclass_of(_object_or_class: &PhpMixed, _class_name: &str, _allow_string: bool) -> bool {
    todo!()
}

pub fn memory_get_peak_usage(_real_usage: bool) -> i64 {
    todo!()
}

pub fn register_shutdown_function(_callback: Box<dyn Fn()>) {
    todo!()
}

pub fn round(_value: f64, _precision: i64) -> f64 {
    todo!()
}

pub fn stdin_handle() -> PhpMixed {
    todo!()
}

pub fn composer_dev_warning_time() -> i64 {
    todo!()
}

pub fn instantiate_class(_class: &str, _args: Vec<PhpMixed>) -> PhpMixed {
    todo!()
}

pub fn array_filter_use_key(
    _array: &IndexMap<String, PhpMixed>,
    _callback: Box<dyn Fn(&str) -> bool>,
) -> IndexMap<String, PhpMixed> {
    todo!()
}

pub fn escapeshellcmd(_command: &str) -> String {
    todo!()
}

pub fn system(_command: &str, _result_code: Option<&mut i64>) -> Option<String> {
    todo!()
}

pub fn array_chunk<T: Clone>(_array: &[T], _size: i64, _preserve_keys: bool) -> Vec<Vec<T>> {
    todo!()
}

pub fn number_format(
    _number: f64,
    _decimals: i64,
    _decimal_separator: &str,
    _thousands_separator: &str,
) -> String {
    todo!()
}

pub fn is_executable(_path: &str) -> bool {
    todo!()
}

pub fn gc_collect_cycles() -> i64 {
    todo!()
}

pub fn gc_disable() {
    todo!()
}

pub fn gc_enable() {
    todo!()
}

pub fn addcslashes(_string: &str, _charlist: &str) -> String {
    todo!()
}

pub fn strnatcmp(_s1: &str, _s2: &str) -> i64 {
    todo!()
}

pub fn uksort<V, F>(_array: &mut IndexMap<String, V>, _callback: F)
where
    F: FnMut(&str, &str) -> i64,
{
    todo!()
}

pub fn end<V: Clone>(_array: &[V]) -> Option<V> {
    todo!()
}

pub fn fileatime(_filename: &str) -> Option<i64> {
    todo!()
}

pub fn fread(_handle: PhpMixed, _length: i64) -> Option<String> {
    todo!()
}

pub fn lstat(_filename: &str) -> Option<IndexMap<String, Box<PhpMixed>>> {
    todo!()
}

pub fn react_promise_resolve(_value: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn symlink(_target: &str, _link: &str) -> bool {
    todo!()
}

pub fn array_diff_key(
    _array1: IndexMap<String, PhpMixed>,
    _array2: &IndexMap<String, PhpMixed>,
) -> IndexMap<String, PhpMixed> {
    todo!()
}

pub fn min(_a: i64, _b: i64) -> i64 {
    todo!()
}

pub fn escapeshellarg(_arg: &str) -> String {
    todo!()
}

pub fn strcspn(_string: &str, _characters: &str) -> usize {
    todo!()
}

pub fn strstr(_haystack: &str, _needle: &str) -> Option<String> {
    todo!()
}

pub fn ioncube_loader_iversion() -> i64 {
    todo!()
}

pub fn ioncube_loader_version() -> String {
    todo!()
}

pub fn phpinfo(_what: i64) {
    todo!()
}

pub fn opendir(_path: &str) -> Option<PhpMixed> {
    todo!()
}

pub fn stream_copy_to_stream(_source: PhpMixed, _dest: PhpMixed) -> Option<i64> {
    todo!()
}

pub const SKIP_DOTS: i64 = 4096;
pub const CHILD_FIRST: i64 = 16;
pub const SELF_FIRST: i64 = 0;
pub const CURL_VERSION_ZSTD: i64 = 8388608;
pub const INFO_GENERAL: i64 = 1;
pub const OPENSSL_VERSION_NUMBER: i64 = 0;
pub const OPENSSL_VERSION_TEXT: &str = "";
pub const PHP_BINARY: &str = "";
pub const PHP_WINDOWS_VERSION_BUILD: i64 = 0;
pub const DATE_RFC3339: &str = "Y-m-d\\TH:i:sP";
pub const PREG_BACKTRACK_LIMIT_ERROR: i64 = 2;

#[derive(Debug)]
pub struct ArrayObject {
    data: IndexMap<String, Box<PhpMixed>>,
}

impl ArrayObject {
    pub fn new(_array: Option<PhpMixed>) -> Self {
        todo!()
    }
}

#[derive(Debug)]
pub struct JsonObject {
    data: IndexMap<String, Box<PhpMixed>>,
}

#[derive(Debug)]
pub struct StdClass {
    pub data: IndexMap<String, Box<PhpMixed>>,
}

#[derive(Debug)]
pub struct PhpResource;
