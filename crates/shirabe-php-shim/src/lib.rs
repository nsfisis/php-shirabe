use indexmap::IndexMap;

#[derive(Debug, Clone, Default)]
pub enum PhpMixed {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Box<PhpMixed>>),
    Array(IndexMap<String, Box<PhpMixed>>),
    Object(ArrayObject),
}

impl serde::Serialize for PhpMixed {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            PhpMixed::Null => serializer.serialize_none(),
            PhpMixed::Bool(b) => serializer.serialize_bool(*b),
            PhpMixed::Int(i) => serializer.serialize_i64(*i),
            PhpMixed::Float(f) => serializer.serialize_f64(*f),
            PhpMixed::String(s) => serializer.serialize_str(s),
            PhpMixed::List(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            PhpMixed::Array(entries) => {
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (k, v) in entries {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
            PhpMixed::Object(object) => object.serialize(serializer),
        }
    }
}

/// PHP `===` semantics: type-strict and, for arrays, order-sensitive.
impl PartialEq for PhpMixed {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PhpMixed::Null, PhpMixed::Null) => true,
            (PhpMixed::Bool(a), PhpMixed::Bool(b)) => a == b,
            (PhpMixed::Int(a), PhpMixed::Int(b)) => a == b,
            (PhpMixed::Float(a), PhpMixed::Float(b)) => a == b,
            (PhpMixed::String(a), PhpMixed::String(b)) => a == b,
            (PhpMixed::List(a), PhpMixed::List(b)) => a == b,
            (PhpMixed::Array(a), PhpMixed::Array(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|((ka, va), (kb, vb))| ka == kb && va == vb)
            }
            (PhpMixed::Object(a), PhpMixed::Object(b)) => a == b,
            _ => false,
        }
    }
}

impl PartialEq for ArrayObject {
    fn eq(&self, other: &Self) -> bool {
        self.data.len() == other.data.len()
            && self
                .data
                .iter()
                .zip(other.data.iter())
                .all(|((ka, va), (kb, vb))| ka == kb && va == vb)
    }
}

impl serde::Serialize for ArrayObject {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.data.len()))?;
        for (k, v) in &self.data {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
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

    pub fn as_array_mut(&mut self) -> Option<&mut IndexMap<String, Box<PhpMixed>>> {
        match self {
            PhpMixed::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_list_mut(&mut self) -> Option<&mut Vec<Box<PhpMixed>>> {
        match self {
            PhpMixed::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&ArrayObject> {
        match self {
            PhpMixed::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, PhpMixed::Null)
    }

    /// PHP loose boolean cast `(bool) $value`.
    pub fn to_bool(&self) -> bool {
        todo!()
    }

    pub fn get(&self, key: &str) -> Option<&PhpMixed> {
        self.as_array().and_then(|m| m.get(key).map(|v| v.as_ref()))
    }

    /// Treats PhpMixed::Null as None, everything else as Some.
    pub fn as_opt(&self) -> Option<&PhpMixed> {
        if self.is_null() { None } else { Some(self) }
    }

    pub fn unwrap_or(self, default: PhpMixed) -> PhpMixed {
        if self.is_null() { default } else { self }
    }

    pub fn unwrap_or_default(self) -> PhpMixed {
        if self.is_null() { PhpMixed::Null } else { self }
    }

    pub fn unwrap(self) -> PhpMixed {
        if self.is_null() {
            panic!("called `PhpMixed::unwrap()` on a `Null` value");
        }
        self
    }

    /// Treats PhpMixed::Null as None and applies the function for chaining.
    pub fn and_then<U, F: FnOnce(&PhpMixed) -> Option<U>>(&self, f: F) -> Option<U> {
        self.as_opt().and_then(f)
    }

    /// Treats `Null` and `Bool(false)` as the falsy case, anything else as Some.
    pub fn ok_or_else<E, F: FnOnce() -> E>(self, err: F) -> Result<PhpMixed, E> {
        match self {
            PhpMixed::Null | PhpMixed::Bool(false) => Err(err()),
            v => Ok(v),
        }
    }

    /// PHP duck-typed helper-set entry. Real implementation lives in QuestionHelper.
    pub fn ask(
        &self,
        _input: &dyn std::any::Any,
        _output: &mut dyn std::any::Any,
        _question: &dyn std::any::Any,
    ) -> PhpMixed {
        todo!()
    }
}

impl From<()> for PhpMixed {
    fn from(_value: ()) -> Self {
        PhpMixed::Null
    }
}

impl From<bool> for PhpMixed {
    fn from(value: bool) -> Self {
        PhpMixed::Bool(value)
    }
}

/// Blanket downcast helper so trait objects (`dyn Command`, `dyn OutputInterface`,
/// etc.) can be downcast to their concrete type, mirroring PHP `instanceof`.
pub trait AsAny {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

impl<T: std::any::Any> AsAny for T {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl From<i64> for PhpMixed {
    fn from(value: i64) -> Self {
        PhpMixed::Int(value)
    }
}

impl From<f64> for PhpMixed {
    fn from(value: f64) -> Self {
        PhpMixed::Float(value)
    }
}

impl From<String> for PhpMixed {
    fn from(value: String) -> Self {
        PhpMixed::String(value)
    }
}

impl From<&str> for PhpMixed {
    fn from(value: &str) -> Self {
        PhpMixed::String(value.to_string())
    }
}

impl<T> From<IndexMap<String, T>> for PhpMixed
where
    T: Into<PhpMixed>,
{
    fn from(value: IndexMap<String, T>) -> Self {
        PhpMixed::Array(
            value
                .into_iter()
                .map(|(k, v)| (k, Box::new(v.into())))
                .collect(),
        )
    }
}

impl<T> From<Vec<T>> for PhpMixed
where
    T: Into<PhpMixed>,
{
    fn from(value: Vec<T>) -> Self {
        PhpMixed::List(value.into_iter().map(|v| Box::new(v.into())).collect())
    }
}

impl<T> From<Option<T>> for PhpMixed
where
    T: Into<PhpMixed>,
{
    fn from(value: Option<T>) -> Self {
        match value {
            Some(v) => v.into(),
            None => PhpMixed::Null,
        }
    }
}

impl<T> From<Box<T>> for PhpMixed
where
    T: Into<PhpMixed>,
{
    fn from(value: Box<T>) -> Self {
        (*value).into()
    }
}

impl std::fmt::Display for PhpMixed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(&php_to_string(self))
    }
}

#[derive(Debug)]
pub struct Exception {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for Exception {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl std::error::Error for Exception {}

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

impl std::error::Error for RuntimeException {}

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

impl std::error::Error for UnexpectedValueException {}

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

impl std::error::Error for InvalidArgumentException {}

#[derive(Debug)]
pub struct TypeError {
    pub message: String,
    pub code: i64,
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl std::error::Error for TypeError {}

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

impl std::error::Error for LogicException {}

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

impl std::error::Error for BadMethodCallException {}

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

impl std::error::Error for OutOfBoundsException {}

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

impl std::error::Error for ErrorException {}

pub fn is_bool(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Bool(_))
}

pub fn is_string(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::String(_))
}

pub fn is_int(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Int(_))
}

pub fn is_scalar(_value: &PhpMixed) -> bool {
    matches!(
        _value,
        PhpMixed::Bool(_) | PhpMixed::Int(_) | PhpMixed::Float(_) | PhpMixed::String(_)
    )
}

pub fn is_numeric(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn strtotime(_time: &str) -> Option<i64> {
    todo!()
}

pub fn strcasecmp(_s1: &str, _s2: &str) -> i64 {
    _s1.to_ascii_lowercase().cmp(&_s2.to_ascii_lowercase()) as i64
}

pub fn sprintf(_format: &str, _args: &[PhpMixed]) -> String {
    todo!()
}

pub fn array_values<V: Clone>(_array: &IndexMap<String, V>) -> Vec<V> {
    _array.values().cloned().collect()
}

pub fn array_keys<V>(_array: &IndexMap<String, V>) -> Vec<String> {
    _array.keys().cloned().collect()
}

pub fn str_replace(_search: &str, _replace: &str, _subject: &str) -> String {
    todo!()
}

pub fn php_to_string(value: &PhpMixed) -> String {
    match value {
        PhpMixed::Null => String::new(),
        PhpMixed::Bool(true) => "1".to_string(),
        PhpMixed::Bool(false) => String::new(),
        PhpMixed::Int(i) => i.to_string(),
        PhpMixed::Float(f) => f.to_string(),
        PhpMixed::String(s) => s.clone(),
        // PHP renders any array as the literal string "Array".
        PhpMixed::List(_) | PhpMixed::Array(_) => "Array".to_string(),
        PhpMixed::Object(_) => todo!(),
    }
}

// Byte-based, matching PHP's substr. A negative start/length counts from the end.
// The result is reinterpreted as UTF-8 (lossily), which only matters when a slice
// boundary falls inside a multibyte sequence.
pub fn substr(s: &str, start: i64, length: Option<i64>) -> String {
    let bytes = s.as_bytes();
    let len = bytes.len() as i64;
    let start = if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    };
    let end = match length {
        None => len,
        Some(l) if l < 0 => (len + l).max(start),
        Some(l) => (start + l).min(len),
    };
    String::from_utf8_lossy(&bytes[start as usize..end as usize]).into_owned()
}

pub const FILTER_VALIDATE_EMAIL: i64 = 274;

pub const PATH_SEPARATOR: &str = ":";

pub fn spl_autoload_functions() -> Vec<PhpMixed> {
    todo!()
}

pub fn array_push(_array: &mut Vec<String>, _value: String) -> i64 {
    _array.push(_value);
    _array.len() as i64
}

pub fn array_search_in_vec(_needle: &str, _haystack: &[String]) -> Option<usize> {
    _haystack.iter().position(|s| s.as_str() == _needle)
}

pub fn array_map_str_fn<F: Fn(&str) -> String>(_callback: F, _array: &[String]) -> Vec<String> {
    _array.iter().map(|s| _callback(s)).collect()
}

pub fn is_callable(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn is_object(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Object(_))
}

pub fn is_a(_object_or_class: &PhpMixed, _class: &str, _allow_string: bool) -> bool {
    todo!()
}

pub fn str_contains(_haystack: &str, _needle: &str) -> bool {
    _haystack.contains(_needle)
}

pub fn str_starts_with(_haystack: &str, _needle: &str) -> bool {
    _haystack.starts_with(_needle)
}

pub fn str_ends_with(_haystack: &str, _needle: &str) -> bool {
    _haystack.ends_with(_needle)
}

pub fn strpos(_haystack: &str, _needle: &str) -> Option<usize> {
    _haystack.find(_needle)
}

pub fn strtoupper(_s: &str) -> String {
    _s.to_ascii_uppercase()
}

pub fn strlen(_s: &str) -> i64 {
    _s.len() as i64
}

pub fn krsort<V>(_array: &mut IndexMap<i64, V>) {
    todo!()
}

pub fn max_i64(_a: i64, _b: i64) -> i64 {
    _a.max(_b)
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

// Overload accepting an `anyhow::Error` (PHP's `get_class($e)` is commonly used on exceptions).
pub fn get_class_err(_e: &anyhow::Error) -> String {
    todo!()
}

/// Overload accepting any object reference. PHP's `get_class($obj)` returns the
/// class name; in Rust we don't have a runtime class name, so this stub is left
/// as `todo!()`.
pub fn get_class_obj<T: ?Sized>(_object: &T) -> String {
    todo!()
}

pub fn get_debug_type(_value: &PhpMixed) -> String {
    todo!()
}

// Models the constants defined in a standard modern PHP CLI environment on a
// non-Windows platform with the common extensions loaded (curl, openssl, json).
// Windows-only, HHVM and Composer-bootstrap constants are reported undefined.
pub fn defined(name: &str) -> bool {
    matches!(
        name,
        "CURLMOPT_MAX_HOST_CONNECTIONS"
            | "CURL_HTTP_VERSION_2_0"
            | "CURL_HTTP_VERSION_3"
            | "CURL_VERSION_HTTP2"
            | "CURL_VERSION_HTTP3"
            | "CURL_VERSION_HTTPS_PROXY"
            | "CURL_VERSION_LIBZ"
            | "CURL_VERSION_ZSTD"
            | "GLOB_BRACE"
            | "JSON_ERROR_UTF8"
            | "OPENSSL_VERSION_TEXT"
            | "PHP_BINARY"
            | "SIGINT"
            | "STDIN"
            | "STDOUT"
    )
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

// Models the extensions loaded in a standard PHP CLI environment running Composer.
// Opt-in extensions (apcu, xdebug, ionCube, uopz) are reported absent.
pub fn extension_loaded(name: &str) -> bool {
    matches!(
        name,
        "Phar"
            | "curl"
            | "filter"
            | "hash"
            | "iconv"
            | "intl"
            | "mbstring"
            | "openssl"
            | "zip"
            | "zlib"
    )
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

pub const FILE_APPEND: i64 = 8;

pub const STDIN: PhpResource = PhpResource::Stdin;

pub fn fopen(_file: &str, _mode: &str) -> PhpMixed {
    todo!()
}

pub fn fwrite(_file: PhpMixed, _data: &str, _length: i64) -> Option<i64> {
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
    _pieces.join(_glue)
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

static ERROR_REPORTING_LEVEL: std::sync::atomic::AtomicI64 =
    std::sync::atomic::AtomicI64::new(E_ALL);

pub fn error_reporting(level: Option<i64>) -> i64 {
    let old = ERROR_REPORTING_LEVEL.load(std::sync::atomic::Ordering::Relaxed);
    if let Some(level) = level {
        ERROR_REPORTING_LEVEL.store(level, std::sync::atomic::Ordering::Relaxed);
    }
    old
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

pub fn in_array(needle: PhpMixed, haystack: &PhpMixed, strict: bool) -> bool {
    let values: Vec<&PhpMixed> = match haystack {
        PhpMixed::List(items) => items.iter().map(|item| item.as_ref()).collect(),
        PhpMixed::Array(map) => map.values().map(|item| item.as_ref()).collect(),
        _ => return false,
    };

    if !strict {
        // TODO(phase-c): non-strict in_array needs PHP's loose `==` comparison semantics. Only the
        // strict path is implemented; loose comparison is deferred rather than approximated.
        todo!("non-strict in_array (PHP loose comparison)");
    }

    values.iter().any(|value| **value == needle)
}

// TODO(phase-c): takes &Path and returns Option<PathBuf>
pub fn realpath(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .canonicalize()
        .ok()
        .and_then(|p| p.to_str().map(ToOwned::to_owned))
}

pub const JSON_UNESCAPED_UNICODE: i64 = 256;
pub const JSON_UNESCAPED_SLASHES: i64 = 64;
pub const JSON_PRETTY_PRINT: i64 = 128;
pub const JSON_THROW_ON_ERROR: i64 = 4194304;

pub fn json_encode<T: serde::Serialize + ?Sized>(_value: &T) -> Option<String> {
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

// Models the classes available in a standard PHP CLI environment running Composer:
// the common bundled extensions (zip, Phar) plus Composer's own runtime classes.
pub fn class_exists(name: &str) -> bool {
    matches!(name, "Composer\\InstalledVersions" | "Phar" | "ZipArchive")
}

// Models the functions available in a standard modern PHP CLI environment on a
// non-Windows platform with the common extensions loaded (curl, mbstring, iconv,
// zlib, posix, pcntl). Opt-in or Windows-only functions are reported absent.
pub fn function_exists(name: &str) -> bool {
    matches!(
        name,
        "bzcompress"
            | "cli_set_process_title"
            | "curl_multi_exec"
            | "curl_multi_init"
            | "curl_multi_setopt"
            | "curl_share_init"
            | "curl_strerror"
            | "date_default_timezone_get"
            | "date_default_timezone_set"
            | "disk_free_space"
            | "exec"
            | "filter_var"
            | "getmypid"
            | "gzcompress"
            | "iconv"
            | "ini_set"
            | "json_decode"
            | "mb_check_encoding"
            | "mb_convert_encoding"
            | "mb_strlen"
            | "pcntl_async_signals"
            | "pcntl_signal"
            | "php_strip_whitespace"
            | "php_uname"
            | "posix_geteuid"
            | "posix_getpwuid"
            | "posix_getuid"
            | "posix_isatty"
            | "proc_open"
            | "putenv"
            | "shell_exec"
            | "stream_isatty"
            | "symlink"
    )
}

pub fn mb_convert_encoding(_string: Vec<u8>, _to_encoding: &str, _from_encoding: &str) -> String {
    todo!()
}

pub fn touch(_path: &str) -> bool {
    todo!()
}

pub fn touch2(_path: &str, _mtime: i64) -> bool {
    todo!()
}

pub fn touch3(_path: &str, _mtime: i64, _atime: i64) -> bool {
    todo!()
}

/// PHP `PHP_OS_FAMILY` constant: the family of the host OS.
/// One of "Windows", "BSD", "Darwin", "Solaris", "Linux", "Unknown".
pub fn php_os_family() -> &'static str {
    match std::env::consts::OS {
        "linux" | "android" => "Linux",
        "macos" | "ios" => "Darwin",
        "windows" => "Windows",
        "freebsd" | "dragonfly" | "netbsd" | "openbsd" => "BSD",
        "solaris" | "illumos" => "Solaris",
        _ => "Unknown",
    }
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

// Models the configuration of a standard PHP CLI environment. Settings belonging
// to extensions that are not loaded (apcu, uopz, xdebug) are not registered, so
// PHP's ini_get returns false (None) for them.
pub fn ini_get(option: &str) -> Option<String> {
    match option {
        "allow_url_fopen" => Some("1".to_string()),
        "default_socket_timeout" => Some("60".to_string()),
        "disable_functions" => Some(String::new()),
        "mbstring.func_overload" => Some("0".to_string()),
        "memory_limit" => Some("-1".to_string()),
        "open_basedir" => Some(String::new()),
        _ => None,
    }
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

// TODO(php-runtime): the callback should be registered in PHP runtime.
pub fn set_error_handler(_callback: fn(i64, &str, &str, i64) -> bool) {}

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

/// PHP `array_merge`.
///
/// Must reproduce PHP's mixed integer/string key semantics:
/// - string keys: a later array's value overwrites an earlier one, keeping the
///   earlier key's position;
/// - integer-like keys ("0","1",...): values are appended and renumbered
///   sequentially across all inputs (they are NOT overwritten by key).
///
/// A naive per-entry `IndexMap::insert` is INCORRECT for inputs that mix string
/// and integer keys (e.g. an AliasPackage's provides/replaces, where
/// self.version expansion appends links under "0","1",... keys). See the typed
/// [`array_merge_map`] variant used by such call sites.
pub fn array_merge(_array1: PhpMixed, _array2: PhpMixed) -> PhpMixed {
    todo!()
}

/// PHP `array_merge` for a string-keyed map that MAY also contain integer-like
/// keys. Typed counterpart of [`array_merge`] for `IndexMap<String, V>` values
/// (e.g. `Link` maps from `getProvides`/`getReplaces`).
///
/// Must reproduce the same mixed-key semantics as [`array_merge`]: string keys
/// overwrite in place (later wins), integer-like keys ("0","1",...) are appended
/// and renumbered sequentially across both inputs. A naive `IndexMap::insert`
/// per entry is INCORRECT because it would collide on shared integer keys.
pub fn array_merge_map<V>(
    _array1: IndexMap<String, V>,
    _array2: IndexMap<String, V>,
) -> IndexMap<String, V> {
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
    std::fs::metadata(_path).ok().map(|m| m.len() as i64)
}

pub fn random_int(_min: i64, _max: i64) -> i64 {
    todo!()
}

pub fn json_encode_ex<T: serde::Serialize + ?Sized>(_value: &T, _flags: i64) -> Option<String> {
    todo!()
}

pub const JSON_INVALID_UTF8_IGNORE: i64 = 1048576;

pub fn is_array(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::List(_) | PhpMixed::Array(_))
}

pub fn strnatcasecmp(_s1: &str, _s2: &str) -> i64 {
    todo!()
}

pub fn file_exists(_path: &str) -> bool {
    std::path::Path::new(_path).exists()
}

pub fn is_writable(_path: &str) -> bool {
    todo!()
}

pub fn unlink(_path: &str) -> bool {
    std::fs::remove_file(_path).is_ok()
}

pub fn file_put_contents(_path: &str, _data: &[u8]) -> Option<i64> {
    std::fs::write(_path, _data)
        .ok()
        .map(|_| _data.len() as i64)
}

pub fn str_repeat(_s: &str, _count: usize) -> String {
    _s.repeat(_count)
}

pub fn strrpos(_haystack: &str, _needle: &str) -> Option<usize> {
    _haystack.rfind(_needle)
}

pub fn gzcompress(_data: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

pub fn bzcompress(_data: &[u8]) -> Option<Vec<u8>> {
    todo!()
}

pub fn getcwd() -> Option<String> {
    std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

pub fn chdir(_path: &str) -> anyhow::Result<()> {
    Ok(std::env::set_current_dir(_path)?)
}

pub fn glob(_pattern: &str) -> Vec<String> {
    todo!()
}

pub fn basename(_path: &str) -> String {
    todo!()
}

pub fn explode(delimiter: &str, string: &str) -> Vec<String> {
    string.split(delimiter).map(|s| s.to_string()).collect()
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

pub fn stream_get_contents_with_max(stream: PhpMixed, max_length: Option<i64>) -> Option<String> {
    let _ = (stream, max_length);
    todo!()
}

pub fn bin2hex(_data: &[u8]) -> String {
    _data.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn random_bytes(_length: usize) -> Vec<u8> {
    todo!()
}

pub fn is_dir(_path: &str) -> bool {
    std::path::Path::new(_path).is_dir()
}

pub fn file_get_contents(_path: &str) -> Option<String> {
    todo!()
}

pub fn file_get_contents5(
    _path: &str,
    _use_include_path: bool,
    _context: PhpMixed,
    _offset: i64,
    _length: Option<i64>,
) -> Option<String> {
    todo!()
}

pub fn strtolower(_s: &str) -> String {
    _s.to_ascii_lowercase()
}

pub fn ctype_alnum(_s: &str) -> bool {
    !_s.is_empty() && _s.bytes().all(|b| b.is_ascii_alphanumeric())
}

pub fn ord(_c: &str) -> i64 {
    _c.as_bytes().first().copied().unwrap_or(0) as i64
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
    _array1
        .iter()
        .filter(|&x| !_array2.contains(x))
        .cloned()
        .collect()
}

pub fn copy(_source: &str, _dest: &str) -> bool {
    std::fs::copy(_source, _dest).is_ok()
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

pub fn iterator_to_array<I>(iter: I) -> Vec<I::Item>
where
    I: IntoIterator,
{
    iter.into_iter().collect()
}

pub fn end_arr<V: Clone>(_array: &IndexMap<String, V>) -> Option<V> {
    _array.values().last().cloned()
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
    _array.first().cloned()
}

pub const OPENSSL_ALGO_SHA384: i64 = 9;

pub fn array_intersect_key(
    _array1: &IndexMap<String, PhpMixed>,
    _array2: &IndexMap<String, PhpMixed>,
) -> IndexMap<String, PhpMixed> {
    _array1
        .iter()
        .filter(|(k, _)| _array2.contains_key(k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn is_file(_path: &str) -> bool {
    std::path::Path::new(_path).is_file()
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
    _haystack
        .to_ascii_lowercase()
        .find(_needle.to_ascii_lowercase().as_str())
}

pub fn php_uname(mode: &str) -> String {
    match mode {
        // sysname, as reported by uname(2). On Windows PHP returns "Windows NT",
        // which differs from PHP_OS.
        "s" => match std::env::consts::OS {
            "linux" => "Linux",
            "macos" => "Darwin",
            "windows" => "Windows NT",
            "freebsd" => "FreeBSD",
            "netbsd" => "NetBSD",
            "openbsd" => "OpenBSD",
            "dragonfly" => "DragonFly",
            "solaris" => "SunOS",
            other => other,
        }
        .to_string(),
        _ => todo!(),
    }
}

pub fn uasort<T, F>(_array: &mut Vec<T>, _compare: F)
where
    F: FnMut(&T, &T) -> i64,
{
    todo!()
}

pub fn uasort_map<K, V, F>(_array: &mut IndexMap<K, V>, _compare: F)
where
    F: FnMut(&V, &V) -> i64,
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
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub fn date(_format: &str, _timestamp: Option<i64>) -> String {
    todo!()
}

pub fn trigger_error(_message: &str, _error_level: i64) {
    todo!()
}

pub fn sys_get_temp_dir() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
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

pub fn dirname_levels(_path: &str, _levels: i64) -> String {
    todo!()
}

// Byte-based, matching PHP's array form of strtr: at each position the longest
// matching key wins (insertion order breaks ties), and replacements are not
// re-scanned. Empty keys are ignored.
pub fn strtr_array(s: &str, pairs: &IndexMap<String, String>) -> String {
    let mut keys: Vec<&String> = pairs.keys().filter(|k| !k.is_empty()).collect();
    keys.sort_by(|a, b| b.len().cmp(&a.len()));

    let bytes = s.as_bytes();
    let mut result: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let mut matched = false;
        for key in &keys {
            let kb = key.as_bytes();
            if bytes[i..].starts_with(kb) {
                result.extend_from_slice(pairs[*key].as_bytes());
                i += kb.len();
                matched = true;
                break;
            }
        }
        if !matched {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&result).into_owned()
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
    _s1.cmp(_s2) as i64
}

pub fn rtrim(_s: &str, _chars: Option<&str>) -> String {
    todo!()
}

pub fn rmdir(_dir: &str) -> bool {
    std::fs::remove_dir(_dir).is_ok()
}

pub fn is_link(_path: &str) -> bool {
    std::fs::symlink_metadata(_path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

pub fn str_pad(_input: &str, _length: usize, _pad_string: &str, _pad_type: i64) -> String {
    todo!()
}

pub const STR_PAD_LEFT: i64 = 0;
pub const STR_PAD_RIGHT: i64 = 1;
pub const STR_PAD_BOTH: i64 = 2;

pub fn abs(_value: i64) -> i64 {
    _value.abs()
}

pub fn ucfirst(_s: &str) -> String {
    todo!()
}

pub fn strval(value: &PhpMixed) -> String {
    php_to_string(value)
}

pub fn usleep(_microseconds: u64) {
    std::thread::sleep(std::time::Duration::from_micros(_microseconds));
}

pub fn mb_strlen(_s: &str, _encoding: &str) -> i64 {
    todo!()
}

pub fn stream_isatty(stream: PhpResource) -> bool {
    stream_isatty_resource(&stream)
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

pub fn posix_isatty(_stream: PhpResource) -> bool {
    todo!()
}

pub fn fstat(_stream: PhpResource) -> PhpMixed {
    todo!()
}

pub fn getenv(_name: &str) -> Option<String> {
    std::env::var(_name).ok()
}

// TODO(phase-c): only the simple `^(\w+)=(.+)$` form is supported.
pub fn putenv(setting: &str) -> bool {
    let is_word =
        |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    let Some((name, value)) = setting.split_once('=') else {
        panic!("putenv: unsupported setting format: {setting:?}");
    };
    if !is_word(name) {
        panic!("putenv: unsupported setting format: {setting:?}");
    }
    unsafe {
        std::env::set_var(name, value);
    }
    true
}

/// PHP superglobal $_SERVER access. In the CLI SAPI $_SERVER is populated from
/// the environment, which is the only source modeled here.
pub fn server_get(name: &str) -> Option<String> {
    // TODO: is var_os() better?
    std::env::var(name).ok()
}

// TODO(php-runtime): modify the real PHP's $_SERVER.
pub fn server_set(_name: &str, _value: String) {}

// TODO(php-runtime): modify the real PHP's $_SERVER.
pub fn server_unset(_name: &str) {}

pub fn server_contains_key(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

/// PHP superglobal $_ENV access.
pub fn env_get(name: &str) -> Option<String> {
    // TODO: is var_os() better?
    std::env::var(name).ok()
}

// TODO(php-runtime): modify the real PHP's $_ENV.
pub fn env_set(_name: &str, _value: String) {}

// TODO(php-runtime): modify the real PHP's $_ENV.
pub fn env_unset(_name: &str) {}

pub fn env_contains_key(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

pub fn trim(_s: &str, _chars: Option<&str>) -> String {
    todo!()
}

pub fn count(_value: &PhpMixed) -> i64 {
    todo!()
}

pub fn array_shift<T>(_array: &mut Vec<T>) -> Option<T> {
    if _array.is_empty() {
        None
    } else {
        Some(_array.remove(0))
    }
}

pub fn array_pop<T>(_array: &mut Vec<T>) -> Option<T> {
    _array.pop()
}

pub fn array_unshift<T>(_array: &mut Vec<T>, _value: T) {
    _array.insert(0, _value);
}

pub fn array_reverse<T: Clone>(_array: &[T], _preserve_keys: bool) -> Vec<T> {
    _array.iter().rev().cloned().collect()
}

pub fn array_filter<T: Clone, F>(_array: &[T], _callback: F) -> Vec<T>
where
    F: Fn(&T) -> bool,
{
    _array.iter().filter(|&x| _callback(x)).cloned().collect()
}

pub fn array_filter_map<F>(
    _array: &IndexMap<String, Box<PhpMixed>>,
    _callback: F,
) -> IndexMap<String, PhpMixed>
where
    F: Fn(&PhpMixed) -> bool,
{
    _array
        .iter()
        .filter(|(_, v)| _callback(v.as_ref()))
        .map(|(k, v)| (k.clone(), v.as_ref().clone()))
        .collect()
}

pub fn array_all<T, F>(_array: &[T], _callback: F) -> bool
where
    F: Fn(&T) -> bool,
{
    _array.iter().all(|x| _callback(x))
}

pub fn array_any<T, F>(_array: &[T], _callback: F) -> bool
where
    F: Fn(&T) -> bool,
{
    _array.iter().any(|x| _callback(x))
}

pub fn array_reduce<T, U, F>(_array: &[T], _callback: F, _initial: U) -> U
where
    F: Fn(U, &T) -> U,
{
    _array.iter().fold(_initial, |acc, x| _callback(acc, x))
}

pub fn array_intersect<T: Clone + PartialEq>(_array1: &[T], _array2: &[T]) -> Vec<T> {
    _array1
        .iter()
        .filter(|&x| _array2.contains(x))
        .cloned()
        .collect()
}

pub fn mkdir(_pathname: &str, _mode: u32, _recursive: bool) -> bool {
    todo!()
}

pub fn rename(_old_name: &str, _new_name: &str) -> bool {
    std::fs::rename(_old_name, _new_name).is_ok()
}

pub fn clearstatcache() {
    // Rust performs a fresh syscall for every metadata query; there is no stat
    // cache to invalidate.
}

pub fn clearstatcache2(_clear_realpath_cache: bool, _filename: &str) {
    // Rust performs a fresh syscall for every metadata query; there is no stat
    // cache to invalidate.
}

pub fn disk_free_space(_directory: &str) -> Option<f64> {
    todo!()
}

pub fn filemtime(_filename: &str) -> Option<i64> {
    std::fs::metadata(_filename)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
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

pub fn array_flip_strings(_array: &[String]) -> IndexMap<String, PhpMixed> {
    _array
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), PhpMixed::Int(i as i64)))
        .collect()
}

pub fn max(_a: i64, _b: i64) -> i64 {
    _a.max(_b)
}

pub fn array_key_exists<V>(_key: &str, _array: &IndexMap<String, V>) -> bool {
    _array.contains_key(_key)
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
    _value.floor()
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
    _array.sort();
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
    let mut compare = _compare;
    _array.sort_by(|a, b| compare(a, b).cmp(&0));
}

pub fn ksort<V>(_array: &mut IndexMap<String, V>) {
    todo!()
}

pub fn is_null(_value: &PhpMixed) -> bool {
    matches!(_value, PhpMixed::Null)
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
    _array.first().cloned()
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
    _array.iter().map(|x| _callback(x)).collect()
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

// The shim does not raise PHP-level errors, so there is never a last error.
pub fn error_get_last() -> Option<IndexMap<String, Box<PhpMixed>>> {
    None
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

    pub fn get_sub_pathname(&self) -> String {
        todo!()
    }
}

impl IntoIterator for &RecursiveIteratorIterator {
    type Item = RecursiveIteratorFileInfo;
    type IntoIter = std::vec::IntoIter<RecursiveIteratorFileInfo>;

    fn into_iter(self) -> Self::IntoIter {
        todo!()
    }
}

#[derive(Debug)]
pub struct RecursiveIteratorFileInfo;

impl RecursiveIteratorFileInfo {
    pub fn is_dir(&self) -> bool {
        todo!()
    }

    pub fn is_file(&self) -> bool {
        todo!()
    }

    pub fn is_link(&self) -> bool {
        todo!()
    }

    pub fn get_pathname(&self) -> String {
        todo!()
    }

    pub fn get_size(&self) -> i64 {
        todo!()
    }
}

pub fn recursive_directory_iterator(
    _path: &str,
    _flags: i64,
) -> Result<RecursiveDirectoryIterator, UnexpectedValueException> {
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

static DEFAULT_TIMEZONE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

// PHP defaults to "UTC" when no default timezone has been configured.
pub fn date_default_timezone_get() -> String {
    DEFAULT_TIMEZONE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| "UTC".to_string())
}

pub fn date_default_timezone_set(tz: &str) -> bool {
    *DEFAULT_TIMEZONE.lock().unwrap() = Some(tz.to_string());
    true
}

pub fn getmypid() -> i64 {
    std::process::id() as i64
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

thread_local! {
    static SHUTDOWN_FUNCTIONS: std::cell::RefCell<Vec<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

pub fn register_shutdown_function(callback: Box<dyn Fn()>) {
    SHUTDOWN_FUNCTIONS.with(|f| f.borrow_mut().push(callback));
}

// Runs the registered shutdown functions in registration order, mirroring PHP
// executing them at the end of the request. Must be invoked at every process exit.
pub fn run_shutdown_functions() {
    let functions = SHUTDOWN_FUNCTIONS.with(|f| std::mem::take(&mut *f.borrow_mut()));
    for callback in &functions {
        callback();
    }
}

pub fn round(_value: f64, _precision: i64) -> f64 {
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
    _array
        .iter()
        .filter(|(k, _)| _callback(k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn escapeshellcmd(_command: &str) -> String {
    todo!()
}

pub fn system(_command: &str, _result_code: Option<&mut i64>) -> Option<String> {
    todo!()
}

pub fn array_chunk<T: Clone>(_array: &[T], _size: i64, _preserve_keys: bool) -> Vec<Vec<T>> {
    _array.chunks(_size as usize).map(|c| c.to_vec()).collect()
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
    // Rust has no cycle collector; nothing is collected.
    0
}

pub fn gc_disable() {
    // Rust has no cycle collector to disable.
}

pub fn gc_enable() {
    // Rust has no cycle collector to enable.
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
    _array.last().cloned()
}

pub fn fileatime(_filename: &str) -> Option<i64> {
    std::fs::metadata(_filename)
        .ok()
        .and_then(|m| m.accessed().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
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
    _array1
        .into_iter()
        .filter(|(k, _)| !_array2.contains_key(k.as_str()))
        .collect()
}

pub fn min(_a: i64, _b: i64) -> i64 {
    _a.min(_b)
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
pub const PREG_BACKTRACK_LIMIT_ERROR: i64 = 2;

#[derive(Debug, Clone)]
pub struct ArrayObject {
    data: IndexMap<String, Box<PhpMixed>>,
}

impl ArrayObject {
    pub fn new(_array: Option<PhpMixed>) -> Self {
        todo!()
    }

    pub fn to_array(&self) -> IndexMap<String, Box<PhpMixed>> {
        self.data.clone()
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

#[derive(Debug, Clone)]
pub enum PhpResource {
    Stdin,
    Stdout,
    Stderr,
    File(std::rc::Rc<std::cell::RefCell<std::fs::File>>),
}

pub fn gethostbyname(_hostname: &str) -> String {
    todo!()
}

pub fn http_get_last_response_headers() -> Option<Vec<String>> {
    todo!()
}

pub fn http_clear_last_response_headers() {
    todo!()
}

pub fn zlib_decode(_data: &str) -> Option<String> {
    todo!()
}

pub const STREAM_NOTIFY_FAILURE: i64 = 9;
pub const STREAM_NOTIFY_FILE_SIZE_IS: i64 = 5;
pub const STREAM_NOTIFY_PROGRESS: i64 = 7;

pub fn date_create<Tz: chrono::TimeZone>(s: &str) -> chrono::ParseResult<chrono::DateTime<Tz>> {
    todo!()
}

/// PHP: \DATE_RFC3339 ("Y-m-d\TH:i:sP").
pub const DATE_RFC3339: &str = "%Y-%m-%dT%H:%M:%S%:z";

/// PHP: \DATE_ATOM (equivalent to \DATE_RFC3339).
pub const DATE_ATOM: &str = DATE_RFC3339;

/// Convert PHP-compatible date time format to strftime-compatible format.
/// Only the patterns Composer actually passes are supported; anything else panics.
pub fn date_format_to_strftime(format: &str) -> &'static str {
    match format {
        "Y-m-d H:i:s" => "%Y-%m-%d %H:%M:%S",
        "Y-m-d" => "%Y-%m-%d",
        "Ymd" => "%Y%m%d",
        other => panic!("Unsupported PHP date format: {other:?}"),
    }
}

// NOTE: &str matching in const expression does not compile for now.
pub const PHP_OS: &str = match std::env::consts::OS.as_bytes() {
    b"linux" => "Linux",
    b"macos" => "Darwin",
    b"windows" => "WINNT",
    b"freebsd" => "FreeBSD",
    b"openbsd" => "OpenBSD",
    b"netbsd" => "NetBSD",
    b"dragonfly" => "DragonFly",
    b"solaris" | b"illumos" => "SunOS",
    _ => std::env::consts::OS,
};

// ===== Symfony Console Phase B shim additions =====

pub fn instance_of<T>(_value: &PhpMixed) -> bool {
    todo!()
}
pub fn to_array(_value: PhpMixed) -> IndexMap<String, Box<PhpMixed>> {
    todo!()
}
pub fn to_string(_value: &PhpMixed) -> String {
    todo!()
}
pub fn to_bool(_value: &PhpMixed) -> bool {
    todo!()
}
pub fn php_truthy(value: &PhpMixed) -> bool {
    match value {
        PhpMixed::Null => false,
        PhpMixed::Bool(b) => *b,
        PhpMixed::Int(i) => *i != 0,
        PhpMixed::Float(f) => *f != 0.0,
        // PHP treats only "" and "0" as falsy strings.
        PhpMixed::String(s) => !s.is_empty() && s != "0",
        PhpMixed::List(items) => !items.is_empty(),
        PhpMixed::Array(entries) => !entries.is_empty(),
        // Objects are always truthy.
        PhpMixed::Object(_) => true,
    }
}
pub fn boolval(value: &PhpMixed) -> bool {
    php_truthy(value)
}
pub fn is_iterable(_value: &PhpMixed) -> bool {
    todo!()
}

pub fn shell_exec(_command: &str) -> Option<String> {
    todo!()
}

pub fn mb_detect_encoding(
    _s: &str,
    _encodings: Option<Vec<String>>,
    _strict: bool,
) -> Option<String> {
    todo!()
}
pub fn mb_strwidth(_s: &str, _encoding: Option<&str>) -> i64 {
    todo!()
}
pub fn mb_substr(_s: &str, _start: i64, _length: Option<i64>, _encoding: Option<&str>) -> String {
    todo!()
}
pub fn mb_str_split(_s: &str, _length: i64) -> Vec<String> {
    todo!()
}
pub fn mb_convert_variables(_to: &str, _from: &str, _vars: &mut Vec<String>) -> Option<String> {
    todo!()
}

pub fn ceil(_v: f64) -> f64 {
    todo!()
}
pub fn intdiv(_a: i64, _b: i64) -> i64 {
    todo!()
}
pub fn hexdec(_s: &str) -> i64 {
    todo!()
}
pub fn byte_at(s: &str, i: usize) -> u8 {
    s.as_bytes().get(i).copied().unwrap_or(0)
}
pub fn str_split(_s: &str, _length: i64) -> Vec<String> {
    todo!()
}
pub fn stripcslashes(_s: &str) -> String {
    todo!()
}
pub fn str_bitand(_a: &str, _b: &str) -> String {
    todo!()
}
pub fn wordwrap(_s: &str, _width: i64, _break_str: &str, _cut: bool) -> String {
    todo!()
}
pub fn ctype_digit(_s: &str) -> bool {
    todo!()
}
pub fn is_numeric_string(_s: &str) -> bool {
    todo!()
}
pub fn is_numeric_to_int(_value: &PhpMixed) -> i64 {
    todo!()
}
pub fn explode_limit(_delimiter: &str, _string: &str, _limit: i64) -> Vec<String> {
    todo!()
}
pub fn sort_natural_flag_case(_values: &mut Vec<String>) {
    todo!()
}
pub fn get_debug_type_obj<T>(_value: &T) -> String {
    todo!()
}
pub fn dir() -> String {
    todo!()
}
pub fn exit(status: i64) -> ! {
    // PHP runs registered shutdown functions before terminating.
    run_shutdown_functions();
    std::process::exit(status as i32);
}

pub fn preg_match_all(_pattern: &str, _subject: &str) -> Vec<Vec<String>> {
    todo!()
}
pub fn preg_match_all_simple(
    _pattern: &str,
    _subject: &str,
    _matches: &mut Vec<Vec<String>>,
) -> anyhow::Result<i64> {
    todo!()
}
pub fn preg_match_all_set_order(
    _pattern: &str,
    _subject: &str,
    _matches: &mut Vec<Vec<String>>,
) -> anyhow::Result<i64> {
    todo!()
}
pub fn preg_match_offset(
    _pattern: &str,
    _subject: &str,
    _matches: &mut Vec<String>,
    _flags: i64,
    _offset: i64,
) -> bool {
    todo!()
}
pub fn preg_match_groups(_pattern: &str, _subject: &str) -> Option<Vec<String>> {
    todo!()
}
pub fn preg_grep(_pattern: &str, _input: &Vec<String>) -> Vec<String> {
    todo!()
}
pub fn preg_split_chars(_pattern: &str, _subject: &str) -> Vec<String> {
    todo!()
}

#[derive(Debug, Default)]
pub struct PregOffsetCaptureMatches {
    groups: Vec<Vec<(String, usize)>>,
}
impl PregOffsetCaptureMatches {
    pub fn group(&self, i: usize) -> &[(String, usize)] {
        &self.groups[i]
    }
}
// Translates a PHP PCRE pattern (delimiters + trailing modifiers) into a regex
// the `regex` crate can compile. Only delimiter stripping and the i/x/s/m
// modifiers are handled; PCRE-only constructs (possessive quantifiers,
// lookaround, backreferences) are not supported by `regex` and must be avoided
// in the caller's pattern.
// TODO(phase-c): replace with a faithful PCRE engine to restore full semantics.
fn compile_php_pattern(pattern: &str) -> anyhow::Result<regex::Regex> {
    let delimiter = pattern
        .chars()
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty regex pattern"))?;
    let end = pattern
        .rfind(delimiter)
        .filter(|&i| i > 0)
        .ok_or_else(|| anyhow::anyhow!("unterminated regex pattern: {pattern}"))?;
    let inner = &pattern[delimiter.len_utf8()..end];
    let modifiers = &pattern[end + delimiter.len_utf8()..];

    let flags: String = modifiers
        .chars()
        .filter(|c| matches!(c, 'i' | 'x' | 's' | 'm'))
        .collect();

    let translated = if flags.is_empty() {
        inner.to_string()
    } else {
        format!("(?{flags}){inner}")
    };

    Ok(regex::Regex::new(&translated)?)
}

pub fn preg_match_all_offset_capture(
    pattern: &str,
    subject: &str,
    matches: &mut PregOffsetCaptureMatches,
) -> anyhow::Result<i64> {
    let re = compile_php_pattern(pattern)?;
    let group_count = re.captures_len();
    matches.groups = vec![Vec::new(); group_count];

    let mut count = 0;
    for caps in re.captures_iter(subject) {
        count += 1;
        for g in 0..group_count {
            // PHP stores ["", -1] for non-participating groups under
            // PREG_OFFSET_CAPTURE; the unsigned offset here approximates -1 as 0,
            // which callers must not rely on for absent groups.
            let entry = caps
                .get(g)
                .map(|m| (m.as_str().to_string(), m.start()))
                .unwrap_or_else(|| (String::new(), 0));
            matches.groups[g].push(entry);
        }
    }

    Ok(count)
}
pub fn preg_replace_callback<F>(
    _pattern: &str,
    _callback: F,
    _subject: &str,
) -> anyhow::Result<String>
where
    F: FnMut(&[Option<String>]) -> anyhow::Result<String>,
{
    todo!()
}

pub fn is_resource_value(_resource: &PhpResource) -> bool {
    true
}
pub fn get_resource_type(_resource: &PhpResource) -> String {
    "stream".to_string()
}
pub fn stream_isatty_resource(resource: &PhpResource) -> bool {
    use std::io::IsTerminal;
    match resource {
        PhpResource::Stdin => std::io::stdin().is_terminal(),
        PhpResource::Stdout => std::io::stdout().is_terminal(),
        PhpResource::Stderr => std::io::stderr().is_terminal(),
        PhpResource::File(_) => false,
    }
}
pub fn fwrite_resource(resource: &PhpResource, data: &str) {
    use std::io::Write;
    let bytes = data.as_bytes();
    match resource {
        PhpResource::Stdin => {}
        PhpResource::Stdout => {
            let _ = std::io::stdout().write_all(bytes);
        }
        PhpResource::Stderr => {
            let _ = std::io::stderr().write_all(bytes);
        }
        PhpResource::File(file) => {
            let _ = file.borrow_mut().write_all(bytes);
        }
    }
}
pub fn fflush_resource(resource: &PhpResource) {
    use std::io::Write;
    match resource {
        PhpResource::Stdin => {}
        PhpResource::Stdout => {
            let _ = std::io::stdout().flush();
        }
        PhpResource::Stderr => {
            let _ = std::io::stderr().flush();
        }
        PhpResource::File(file) => {
            let _ = file.borrow_mut().flush();
        }
    }
}
pub fn fgetc(_resource: &PhpResource) -> Option<String> {
    todo!()
}
pub fn ftell(_resource: &PhpResource) -> i64 {
    todo!()
}
pub fn stream_get_meta_data(_resource: &PhpResource) -> IndexMap<String, Box<PhpMixed>> {
    todo!()
}
pub fn stream_set_blocking(_resource: &PhpResource, _enable: bool) -> bool {
    todo!()
}
pub fn stream_select(
    _read: &mut Vec<PhpResource>,
    _write: &mut Vec<PhpResource>,
    _except: &mut Vec<PhpResource>,
    _seconds: i64,
    _microseconds: Option<i64>,
) -> i64 {
    todo!()
}
pub fn php_fopen_resource(path: &str, mode: &str) -> PhpResource {
    match path {
        "php://output" | "php://stdout" => return PhpResource::Stdout,
        "php://stderr" => return PhpResource::Stderr,
        "php://stdin" | "php://input" => return PhpResource::Stdin,
        _ => {}
    }
    // Strip the binary/text flags PHP accepts as part of the mode.
    let base_mode: String = mode.chars().filter(|c| *c != 'b' && *c != 't').collect();
    let mut options = std::fs::OpenOptions::new();
    match base_mode.as_str() {
        "r" => options.read(true),
        "r+" => options.read(true).write(true),
        "w" => options.write(true).create(true).truncate(true),
        "w+" => options.read(true).write(true).create(true).truncate(true),
        "a" => options.append(true).create(true),
        "a+" => options.read(true).append(true).create(true),
        "x" => options.write(true).create_new(true),
        "x+" => options.read(true).write(true).create_new(true),
        _ => options.read(true),
    };
    let file = options
        .open(path)
        .unwrap_or_else(|e| panic!("php_fopen_resource failed to open {path:?}: {e}"));
    PhpResource::File(std::rc::Rc::new(std::cell::RefCell::new(file)))
}
pub fn php_stdout_resource() -> PhpResource {
    PhpResource::Stdout
}
pub fn php_stderr_resource() -> PhpResource {
    PhpResource::Stderr
}
pub fn stdin() -> PhpResource {
    PhpResource::Stdin
}

// TODO(phase-c): reports proc_open as unavailable, so callers (terminal size and
// tty detection) fall back to their defaults. A real implementation requires a
// non-lossy signature able to hold the child process and its pipes; defer it to
// the broader process-subsystem work (ProcessExecutor).
pub fn proc_open(_command: &str, _descriptorspec: &Vec<PhpMixed>, _pipes: &mut PhpMixed) -> bool {
    false
}
pub fn proc_close(_process: bool) -> i64 {
    -1
}

pub fn sapi_windows_vt100_support(_resource: &PhpResource) -> bool {
    todo!()
}
pub fn sapi_windows_cp_get(_kind: Option<&str>) -> i64 {
    todo!()
}
pub fn sapi_windows_cp_set(_codepage: i64) -> bool {
    todo!()
}
pub fn sapi_windows_cp_conv(_in_codepage: i64, _out_codepage: i64, _subject: &str) -> String {
    todo!()
}

pub const SIGINT: i64 = 2;
pub const SIGTERM: i64 = 15;
pub const SIGUSR1: i64 = 10;
pub const SIGUSR2: i64 = 12;
// No-op until real signal handling is wired up; signal registration itself is
// deferred (see the TODO(plugin) notes in SignalRegistry::register).
pub fn pcntl_async_signals(_enable: bool) {}
pub fn pcntl_signal(_signal: i64, _handler: PhpMixed) -> bool {
    todo!()
}
pub fn pcntl_signal_get_handler(_signal: i64) -> PhpMixed {
    todo!()
}
pub fn call_php_callable(_callback: &PhpMixed, _args: &[PhpMixed]) -> PhpMixed {
    todo!()
}

pub fn cli_set_process_title(_title: &str) -> bool {
    todo!()
}
pub fn setproctitle(_title: &str) {
    todo!()
}
pub fn spl_object_hash_process<T>(_object: &T) -> String {
    todo!()
}

pub fn server(_key: &str) -> String {
    todo!()
}
pub fn server_argv() -> Vec<String> {
    todo!()
}
pub fn server_php_self() -> String {
    todo!()
}
pub fn server_shell() -> Option<String> {
    todo!()
}

pub fn file_put_contents3(_filename: &str, _data: &str, _flags: i64) -> Option<i64> {
    todo!()
}

#[derive(Debug)]
pub struct DirectoryIteratorEntry;
impl DirectoryIteratorEntry {
    pub fn get_basename(&self) -> String {
        todo!()
    }
    pub fn is_file(&self) -> bool {
        todo!()
    }
    pub fn get_extension(&self) -> String {
        todo!()
    }
}
pub fn directory_iterator(_path: &str) -> Vec<DirectoryIteratorEntry> {
    todo!()
}

pub fn array_key_last(_array: &IndexMap<String, Box<PhpMixed>>) -> usize {
    todo!()
}
pub fn array_splice_mixed(
    _array: &mut Vec<PhpMixed>,
    _offset: i64,
    _length: i64,
    _replacement: Vec<PhpMixed>,
) {
    todo!()
}

pub fn str_replace_arrays(_search: &[String], _replace: &[String], _subject: &str) -> String {
    todo!()
}
pub fn str_replace_arr(_search: &[&str], _replace: &str, _subject: &str) -> String {
    todo!()
}

pub fn php_exception_get_code(_error: &anyhow::Error) -> i64 {
    todo!()
}
pub fn sscanf(_subject: &str, _format: &str, _a: &mut i64, _b: &mut i64) -> i64 {
    todo!()
}
pub fn trigger_deprecation(_package: &str, _version: &str, _message: &str, _arg: &str) {
    todo!()
}
