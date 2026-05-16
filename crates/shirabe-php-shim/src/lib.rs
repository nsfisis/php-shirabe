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

pub fn is_int(value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_scalar(value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_numeric(value: &PhpMixed) -> bool {
    todo!()
}

pub fn trim(s: &str, chars: &str) -> String {
    todo!()
}

pub fn strtotime(time: &str) -> Option<i64> {
    todo!()
}

pub fn strcasecmp(s1: &str, s2: &str) -> i64 {
    todo!()
}

pub fn sprintf(format: &str, args: &[PhpMixed]) -> String {
    todo!()
}

pub fn array_values<V: Clone>(array: &IndexMap<String, V>) -> Vec<V> {
    todo!()
}

pub fn array_keys<V>(array: &IndexMap<String, V>) -> Vec<String> {
    todo!()
}

pub fn str_replace(search: &str, replace: &str, subject: &str) -> String {
    todo!()
}

pub fn php_to_string(value: &PhpMixed) -> String {
    todo!()
}

pub fn substr(s: &str, start: i64, length: Option<i64>) -> String {
    todo!()
}

pub const FILTER_VALIDATE_EMAIL: i64 = 274;

pub const PATH_SEPARATOR: &str = ":";

pub fn spl_autoload_functions() -> Vec<PhpMixed> {
    todo!()
}

pub fn spl_autoload_register(callback: PhpMixed) {
    todo!()
}

pub fn spl_autoload_unregister(callback: PhpMixed) -> bool {
    todo!()
}

pub fn array_pop(array: &mut Vec<String>) -> Option<String> {
    todo!()
}

pub fn array_push(array: &mut Vec<String>, value: String) -> i64 {
    todo!()
}

pub fn array_search_in_vec(needle: &str, haystack: &[String]) -> Option<usize> {
    todo!()
}

pub fn array_splice<T: Clone>(array: &mut Vec<T>, offset: usize, length: usize, replacement: &[T]) -> Vec<T> {
    todo!()
}

pub fn array_map_str_fn<F: Fn(&str) -> String>(callback: F, array: &[String]) -> Vec<String> {
    todo!()
}

pub fn is_callable(value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_object(value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_a(object_or_class: &PhpMixed, class: &str, allow_string: bool) -> bool {
    todo!()
}

pub fn str_contains(haystack: &str, needle: &str) -> bool {
    todo!()
}

pub fn str_starts_with(haystack: &str, needle: &str) -> bool {
    todo!()
}

pub fn str_ends_with(haystack: &str, needle: &str) -> bool {
    todo!()
}

pub fn strpos(haystack: &str, needle: &str) -> Option<usize> {
    todo!()
}

pub fn strtoupper(s: &str) -> String {
    todo!()
}

pub fn strlen(s: &str) -> i64 {
    todo!()
}

pub fn krsort<V>(array: &mut IndexMap<i64, V>) {
    todo!()
}

pub fn max_i64(a: i64, b: i64) -> i64 {
    todo!()
}

pub fn count_mixed(value: &PhpMixed) -> i64 {
    todo!()
}

pub fn array_slice(value: &PhpMixed, offset: i64, length: Option<i64>) -> PhpMixed {
    todo!()
}

pub fn array_slice_strs(value: &[String], offset: i64, length: Option<i64>) -> Vec<String> {
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

    pub fn new(a: String) -> Self {
        todo!()
    }

    pub fn extract_to(&self, a: &str, b: Option<()>, c: bool) {
        todo!()
    }

    pub fn running(return_full: bool) -> String {
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

pub fn urlencode(s: &str) -> String {
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
pub const FILTER_VALIDATE_IP: i64 = 275;
pub const FILTER_VALIDATE_INT: i64 = 257;

pub fn filter_var(value: &str, filter: i64) -> bool {
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

pub fn spl_autoload_unregister(
    callback: Box<dyn Fn(&str) -> PhpMixed + Send + Sync>,
) -> bool {
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

pub fn curl_close(handle: CurlHandle) {
    todo!()
}

pub fn curl_setopt(handle: &CurlHandle, option: i64, value: PhpMixed) -> bool {
    todo!()
}

pub fn curl_setopt_array(handle: &CurlHandle, options: &IndexMap<i64, PhpMixed>) -> bool {
    todo!()
}

pub fn curl_getinfo(handle: &CurlHandle) -> PhpMixed {
    todo!()
}

pub fn curl_error(handle: &CurlHandle) -> String {
    todo!()
}

pub fn curl_errno(handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn curl_strerror(errornum: i64) -> Option<String> {
    todo!()
}

pub fn curl_multi_init() -> CurlMultiHandle {
    todo!()
}

pub fn curl_multi_setopt(mh: &CurlMultiHandle, option: i64, value: PhpMixed) -> bool {
    todo!()
}

pub fn curl_multi_add_handle(mh: &CurlMultiHandle, handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn curl_multi_remove_handle(mh: &CurlMultiHandle, handle: &CurlHandle) -> i64 {
    todo!()
}

pub fn curl_multi_exec(mh: &CurlMultiHandle, still_running: &mut bool) -> i64 {
    todo!()
}

pub fn curl_multi_select(mh: &CurlMultiHandle, timeout: f64) -> i64 {
    todo!()
}

pub fn curl_multi_info_read(mh: &CurlMultiHandle) -> PhpMixed {
    todo!()
}

pub fn curl_share_init() -> CurlShareHandle {
    todo!()
}

pub fn curl_share_setopt(sh: &CurlShareHandle, option: i64, value: PhpMixed) -> bool {
    todo!()
}

/// Cast a `\CurlHandle` to int (its spl_object_id) as `(int) $curlHandle` in PHP.
pub fn curl_handle_id(handle: &CurlHandle) -> i64 {
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

pub fn ctype_alnum(s: &str) -> bool {
    todo!()
}

pub fn ord(c: &str) -> i64 {
    todo!()
}

pub fn gethostname() -> String {
    todo!()
}

pub fn feof(stream: PhpMixed) -> bool {
    todo!()
}

pub fn str_replace_array(
    search: &[String],
    replace: &[String],
    subject: &str,
) -> String {
    todo!()
}

pub fn file(filename: &str, flags: i64) -> Option<Vec<String>> {
    todo!()
}

pub fn ucwords(s: &str) -> String {
    todo!()
}

pub fn get_current_user() -> String {
    todo!()
}

pub const FILE_IGNORE_NEW_LINES: i64 = 2;
pub const FILTER_VALIDATE_EMAIL: i64 = 274;

pub fn array_diff(array1: &[String], array2: &[String]) -> Vec<String> {
    todo!()
}

pub fn copy(source: &str, dest: &str) -> bool {
    todo!()
}

pub fn exec(command: &str, output: Option<&mut Vec<String>>, exit_code: Option<&mut i64>) -> Option<String> {
    todo!()
}

pub fn tempnam(dir: &str, prefix: &str) -> Option<String> {
    todo!()
}

pub fn openssl_verify(data: &str, signature: &[u8], pub_key_id: PhpMixed, algorithm: PhpMixed) -> i64 {
    todo!()
}

pub fn openssl_pkey_get_public(public_key: &str) -> PhpMixed {
    todo!()
}

pub fn openssl_get_md_methods() -> Vec<String> {
    todo!()
}

pub fn openssl_free_key(key: PhpMixed) {
    todo!()
}

pub fn iterator_to_array<T>(iter: T) -> Vec<PhpMixed>
where
    T: IntoIterator<Item = PhpMixed>,
{
    todo!()
}

pub fn end_arr<V: Clone>(array: &IndexMap<String, V>) -> Option<V> {
    todo!()
}

pub fn fileowner(filename: &str) -> Option<i64> {
    todo!()
}

pub fn unlink_silent(path: &str) -> bool {
    todo!()
}

pub fn array_unique<T: Clone>(array: &[T]) -> Vec<T> {
    todo!()
}

pub fn current(value: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn key(value: PhpMixed) -> Option<String> {
    todo!()
}

pub fn reset<T: Clone>(array: &[T]) -> Option<T> {
    todo!()
}


pub const OPENSSL_ALGO_SHA384: i64 = 9;
pub const PHP_VERSION_ID: i64 = 80100;

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

pub const PHP_WINDOWS_VERSION_MAJOR: i64 = 0;
pub const PHP_WINDOWS_VERSION_MINOR: i64 = 0;

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

pub fn json_decode(s: &str, assoc: bool) -> anyhow::Result<PhpMixed> {
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

pub fn http_build_query(data: &[(&str, &str)], sep_str: &str, sep: &str) -> String {
    todo!()
}

pub fn token_get_all(source: &str) -> Vec<PhpMixed> {
    todo!()
}

pub const T_COMMENT: i64 = 315;
pub const T_DOC_COMMENT: i64 = 316;
pub const T_WHITESPACE: i64 = 317;

pub fn dirname_levels(path: &str, levels: i64) -> String {
    todo!()
}

pub fn strtr_array(s: &str, pairs: &IndexMap<String, String>) -> String {
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

pub fn array_search(needle: &str, haystack: &IndexMap<String, String>) -> Option<String> {
    todo!()
}

pub fn strcmp(s1: &str, s2: &str) -> i64 {
    todo!()
}

pub fn rtrim(s: &str, chars: Option<&str>) -> String {
    todo!()
}

pub fn rmdir(dir: &str) -> bool {
    todo!()
}

pub fn is_link(path: &str) -> bool {
    todo!()
}

pub fn strpos(haystack: &str, needle: &str) -> Option<usize> {
    todo!()
}

pub fn str_pad(input: &str, length: usize, pad_string: &str, pad_type: i64) -> String {
    todo!()
}

pub const STR_PAD_LEFT: i64 = 0;
pub const STR_PAD_RIGHT: i64 = 1;
pub const STR_PAD_BOTH: i64 = 2;

pub fn abs(value: i64) -> i64 {
    todo!()
}

pub fn str_contains(haystack: &str, needle: &str) -> bool {
    todo!()
}

pub fn str_starts_with(haystack: &str, needle: &str) -> bool {
    todo!()
}

pub const DATE_ATOM: &str = "Y-m-d\\TH:i:sP";

pub fn ucfirst(s: &str) -> String {
    todo!()
}

pub fn is_scalar(value: &PhpMixed) -> bool {
    todo!()
}

pub fn strval(value: &PhpMixed) -> String {
    todo!()
}

pub fn usleep(microseconds: u64) {
    todo!()
}

pub fn mb_strlen(s: &str, encoding: &str) -> i64 {
    todo!()
}

pub fn strlen(s: &str) -> i64 {
    todo!()
}

pub fn substr(s: &str, offset: i64, length: Option<i64>) -> String {
    todo!()
}

pub fn strtoupper(s: &str) -> String {
    todo!()
}

pub fn stream_isatty(stream: PhpMixed) -> bool {
    todo!()
}

pub fn posix_getuid() -> i64 {
    todo!()
}

pub fn posix_geteuid() -> i64 {
    todo!()
}

pub fn posix_getpwuid(uid: i64) -> PhpMixed {
    todo!()
}

pub fn posix_isatty(stream: PhpMixed) -> bool {
    todo!()
}

pub fn fstat(stream: PhpMixed) -> PhpMixed {
    todo!()
}

pub fn getenv(name: &str) -> Option<String> {
    todo!()
}

pub fn putenv(setting: &str) -> bool {
    todo!()
}

/// PHP superglobal $_SERVER access
pub fn server_get(name: &str) -> Option<String> {
    todo!()
}

pub fn server_set(name: &str, value: String) {
    todo!()
}

pub fn server_unset(name: &str) {
    todo!()
}

pub fn server_contains_key(name: &str) -> bool {
    todo!()
}

pub fn server_argv() -> Vec<String> {
    todo!()
}

/// PHP superglobal $_ENV access
pub fn env_get(name: &str) -> Option<String> {
    todo!()
}

pub fn env_set(name: &str, value: String) {
    todo!()
}

pub fn env_unset(name: &str) {
    todo!()
}

pub fn env_contains_key(name: &str) -> bool {
    todo!()
}

pub fn str_replace(search: &str, replace: &str, subject: &str) -> String {
    todo!()
}

pub fn trim(s: &str, chars: Option<&str>) -> String {
    todo!()
}

pub fn count(value: &PhpMixed) -> i64 {
    todo!()
}

pub fn sprintf(format: &str, args: &[PhpMixed]) -> String {
    todo!()
}

pub fn array_shift<T>(array: &mut Vec<T>) -> Option<T> {
    todo!()
}

pub fn array_pop<T>(array: &mut Vec<T>) -> Option<T> {
    todo!()
}

pub fn array_unshift<T>(array: &mut Vec<T>, value: T) {
    todo!()
}

pub fn array_reverse<T: Clone>(array: &[T], preserve_keys: bool) -> Vec<T> {
    todo!()
}

pub fn array_filter<T: Clone, F>(array: &[T], callback: F) -> Vec<T>
where
    F: Fn(&T) -> bool,
{
    todo!()
}

pub fn array_all<T, F>(array: &[T], callback: F) -> bool
where
    F: Fn(&T) -> bool,
{
    todo!()
}

pub fn array_any<T, F>(array: &[T], callback: F) -> bool
where
    F: Fn(&T) -> bool,
{
    todo!()
}

pub fn array_reduce<T, U, F>(array: &[T], callback: F, initial: U) -> U
where
    F: Fn(U, &T) -> U,
{
    todo!()
}

pub fn array_intersect<T: Clone + PartialEq>(array1: &[T], array2: &[T]) -> Vec<T> {
    todo!()
}

pub fn array_keys<V>(array: &IndexMap<String, V>) -> Vec<String> {
    todo!()
}

pub fn mkdir(pathname: &str, mode: u32, recursive: bool) -> bool {
    todo!()
}

pub fn rename(old_name: &str, new_name: &str) -> bool {
    todo!()
}

pub fn clearstatcache() {
    todo!()
}

pub fn disk_free_space(directory: &str) -> Option<f64> {
    todo!()
}

pub fn filemtime(filename: &str) -> Option<i64> {
    todo!()
}

/// Equivalent to PHP's __DIR__ magic constant
pub fn php_dir() -> String {
    todo!()
}

/// Equivalent to PHP's `require <file>` returning the file's return value
pub fn require_php_file(filename: &str) -> PhpMixed {
    todo!()
}

pub fn array_flip(array: &PhpMixed) -> PhpMixed {
    todo!()
}

pub fn max(a: i64, b: i64) -> i64 {
    todo!()
}

pub fn array_key_exists<V>(key: &str, array: &IndexMap<String, V>) -> bool {
    todo!()
}

pub fn fgets(handle: PhpMixed) -> Option<String> {
    todo!()
}

pub fn umask() -> u32 {
    todo!()
}

pub fn basename_with_suffix(path: &str, suffix: &str) -> String {
    todo!()
}

pub fn inet_pton(host: &str) -> Option<Vec<u8>> {
    todo!()
}

pub fn ltrim(s: &str, chars: Option<&str>) -> String {
    todo!()
}

pub fn floor(value: f64) -> f64 {
    todo!()
}

pub fn chr(value: u8) -> String {
    todo!()
}

pub fn filter_var_with_options(
    value: &str,
    filter: i64,
    options: &IndexMap<String, PhpMixed>,
) -> PhpMixed {
    todo!()
}

pub fn memory_get_usage() -> i64 {
    todo!()
}

pub fn mb_check_encoding(value: &str, encoding: &str) -> bool {
    todo!()
}

pub fn iconv(in_charset: &str, out_charset: &str, string: &str) -> Option<String> {
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

pub fn str_ends_with(haystack: &str, needle: &str) -> bool {
    todo!()
}

pub fn sort<T: Ord>(array: &mut Vec<T>) {
    todo!()
}

pub fn sort_with_flags<T: Ord>(array: &mut Vec<T>, flags: i64) {
    todo!()
}

pub const SORT_REGULAR: i64 = 0;
pub const SORT_NUMERIC: i64 = 1;
pub const SORT_STRING: i64 = 2;
pub const SORT_NATURAL: i64 = 6;
pub const SORT_FLAG_CASE: i64 = 8;

pub fn usort<T, F>(array: &mut Vec<T>, compare: F)
where
    F: FnMut(&T, &T) -> i64,
{
    todo!()
}

pub fn ksort<V>(array: &mut IndexMap<String, V>) {
    todo!()
}

pub fn is_int(value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_null(value: &PhpMixed) -> bool {
    todo!()
}

pub fn r#eval(code: &str) -> PhpMixed {
    todo!()
}

pub fn array_is_list(array: &PhpMixed) -> bool {
    todo!()
}

pub fn array_values<V: Clone>(array: &IndexMap<String, V>) -> Vec<V> {
    todo!()
}

pub fn array_splice<T>(
    array: &mut Vec<T>,
    offset: i64,
    length: Option<i64>,
    replacement: Vec<T>,
) -> Vec<T> {
    todo!()
}

pub fn array_pop_first<T>(array: &mut Vec<T>) -> Option<T> {
    todo!()
}

pub fn reset_first<T: Clone>(array: &[T]) -> Option<T> {
    todo!()
}

pub fn call_user_func<T>(callback: &str, args: &[PhpMixed]) -> T
where
    T: From<PhpMixed>,
{
    todo!()
}

pub fn array_merge_recursive(arrays: Vec<PhpMixed>) -> PhpMixed {
    todo!()
}

pub fn is_object(value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_numeric(value: &PhpMixed) -> bool {
    todo!()
}

pub fn levenshtein(string1: &str, string2: &str) -> i64 {
    todo!()
}

pub fn array_slice<V: Clone>(
    array: &IndexMap<String, V>,
    offset: i64,
    length: Option<i64>,
) -> IndexMap<String, V> {
    todo!()
}

pub fn asort<V: Ord>(array: &mut IndexMap<String, V>) {
    todo!()
}

pub const PHP_EOL: &str = "\n";

pub const PHP_INT_MAX: i64 = i64::MAX;
pub const PHP_INT_MIN: i64 = i64::MIN;
pub const PHP_INT_SIZE: i64 = 8;

pub fn call_user_func_array(callback: &str, args: &PhpMixed) -> PhpMixed {
    todo!()
}

pub fn array_map<T, U, F>(callback: F, array: &[T]) -> Vec<U>
where
    F: Fn(&T) -> U,
{
    todo!()
}

impl Phar {
    pub const SHA512: i64 = 16;

    pub fn new_phar(filename: String, flags: i64, alias: &str) -> Self {
        todo!()
    }

    pub fn set_signature_algorithm(&mut self, algo: i64) {
        todo!()
    }

    pub fn start_buffering(&mut self) {
        todo!()
    }

    pub fn stop_buffering(&mut self) {
        todo!()
    }

    pub fn add_from_string(&mut self, path: &str, content: &str) {
        todo!()
    }

    pub fn set_stub(&mut self, stub: &str) {
        todo!()
    }
}
