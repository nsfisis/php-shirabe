//! The options Composer passes to `HttpDownloader`, `RemoteFilesystem` and
//! `StreamContextFactory`: a stream context array (`http`, `ssl`, ...) together with Composer's
//! own top-level keys (`retry-auth-failure`, `max_file_size`, ...).
//!
//! PHP keeps them in a plain nested array, and so does this type: every group is an ordered map of
//! `PhpMixed`. The array is open in both directions. Its keys come from repository `options` in
//! `composer.json`, from plugins (`PreFileDownloadEvent::setTransportOptions()`,
//! `HttpDownloader::setOptions()`) and from Composer itself, so any key can be present and a known
//! key can hold a value of any type; PHP hands the values to `stream_context_create()` and
//! `curl_setopt()`, which convert them. The insertion order is observable too: it reaches plugins,
//! `json_encode()` and the `===` comparisons of the test doubles. The typed accessors below
//! therefore read and write single keys, apply the conversion of the PHP code that consumes the
//! key, and leave everything else as it is.

use bstr::{B, BString};
use indexmap::IndexMap;
use shirabe_php_rpc::{FromWire, PhpThrow, PluginValue, ToWire};
use shirabe_php_shim::{
    PhpMixed, TypeError, array_replace_recursive, bconcat, get_debug_type, implode_mixed,
    php_to_string,
};

#[derive(Debug, Clone, Default)]
pub struct StreamContextOptions {
    entries: IndexMap<BString, StreamContextOption>,
}

#[derive(Debug, Clone)]
enum StreamContextOption {
    Http(HttpOptions),
    Ssl(SslOptions),
    /// Any other key, or `http`/`ssl` holding something other than a keyed array.
    Other(PhpMixed),
}

/// The `http` group: the options of PHP's `http://` stream wrapper, which `CurlDownloader` maps to
/// curl options as well.
#[derive(Debug, Clone, Default)]
pub struct HttpOptions {
    entries: IndexMap<BString, PhpMixed>,
}

/// The `ssl` group: the options of PHP's SSL context.
#[derive(Debug, Clone, Default)]
pub struct SslOptions {
    entries: IndexMap<BString, PhpMixed>,
}

impl StreamContextOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_array(array: IndexMap<BString, PhpMixed>) -> Self {
        let mut options = Self::new();
        for (key, value) in array {
            options.insert(key, value);
        }
        options
    }

    /// The array an `array $options` parameter receives. Anything else fails with the TypeError
    /// of PHP's type check, for which `parameter` names the parameter (e.g.
    /// `Composer\Util\HttpDownloader::get(): Argument #2 ($options)`).
    pub fn from_mixed(value: PhpMixed, parameter: impl AsRef<[u8]>) -> anyhow::Result<Self> {
        match value {
            PhpMixed::Array(array) => Ok(Self::from_array(array)),
            PhpMixed::List(list) => Ok(Self::from_array(
                list.into_iter()
                    .enumerate()
                    .map(|(i, value)| (bconcat!(i), value))
                    .collect(),
            )),
            other => Err(TypeError::new(bconcat!(
                parameter.as_ref(),
                " must be of type array, ",
                get_debug_type(&other),
                " given"
            ))
            .into()),
        }
    }

    pub fn to_array(&self) -> IndexMap<BString, PhpMixed> {
        self.entries
            .iter()
            .map(|(key, value)| (key.clone(), value.to_mixed()))
            .collect()
    }

    pub fn into_array(self) -> IndexMap<BString, PhpMixed> {
        self.entries
            .into_iter()
            .map(|(key, value)| (key, value.into_mixed()))
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `$options[$key]`, for a key no accessor below covers.
    pub fn get(&self, key: impl AsRef<[u8]>) -> Option<PhpMixed> {
        self.entries
            .get(key.as_ref())
            .map(StreamContextOption::to_mixed)
    }

    /// `isset($options[$key])`.
    pub fn isset(&self, key: impl AsRef<[u8]>) -> bool {
        self.entries
            .get(key.as_ref())
            .is_some_and(|value| !matches!(value, StreamContextOption::Other(PhpMixed::Null)))
    }

    /// `$options[$key] = $value`. An existing key keeps its position.
    pub fn insert(&mut self, key: impl Into<BString>, value: PhpMixed) {
        let key = key.into();
        let value = StreamContextOption::from_mixed(&key, value);
        self.entries.insert(key, value);
    }

    /// `unset($options[$key])`.
    pub fn remove(&mut self, key: impl AsRef<[u8]>) -> Option<PhpMixed> {
        self.entries
            .shift_remove(key.as_ref())
            .map(StreamContextOption::into_mixed)
    }

    /// `$options['http']`, when it is an array.
    pub fn http(&self) -> Option<&HttpOptions> {
        match self.entries.get(B("http")) {
            Some(StreamContextOption::Http(http)) => Some(http),
            _ => None,
        }
    }

    /// `$options['http']` as a write to one of its keys sees it: a missing, null or false group
    /// becomes an empty array, and a scalar fails as PHP does.
    pub fn http_mut(&mut self) -> anyhow::Result<&mut HttpOptions> {
        let entry = self.group_mut(b"http")?;
        if let StreamContextOption::Other(PhpMixed::Array(map)) = entry {
            *entry = StreamContextOption::Http(HttpOptions {
                entries: std::mem::take(map),
            });
        }
        match entry {
            StreamContextOption::Http(http) => Ok(http),
            _ => unreachable!("group_mut() leaves `http` an array"),
        }
    }

    /// `$options['http'] = $http`.
    pub fn set_http(&mut self, http: HttpOptions) {
        self.entries
            .insert("http".into(), StreamContextOption::Http(http));
    }

    /// `$options['ssl']`, when it is an array.
    pub fn ssl(&self) -> Option<&SslOptions> {
        match self.entries.get(B("ssl")) {
            Some(StreamContextOption::Ssl(ssl)) => Some(ssl),
            _ => None,
        }
    }

    /// `$options['ssl']` as a write to one of its keys sees it, like [`Self::http_mut`].
    pub fn ssl_mut(&mut self) -> anyhow::Result<&mut SslOptions> {
        let entry = self.group_mut(b"ssl")?;
        if let StreamContextOption::Other(PhpMixed::Array(map)) = entry {
            *entry = StreamContextOption::Ssl(SslOptions {
                entries: std::mem::take(map),
            });
        }
        match entry {
            StreamContextOption::Ssl(ssl) => Ok(ssl),
            _ => unreachable!("group_mut() leaves `ssl` an array"),
        }
    }

    /// `$options['ssl'] = $ssl`.
    pub fn set_ssl(&mut self, ssl: SslOptions) {
        self.entries
            .insert("ssl".into(), StreamContextOption::Ssl(ssl));
    }

    /// The group under `key`, auto-vivified for a write. A group that is not an array yet is left
    /// as `Other(PhpMixed::Array)` for the caller to wrap.
    fn group_mut(&mut self, key: &[u8]) -> anyhow::Result<&mut StreamContextOption> {
        let entry = self
            .entries
            .entry(key.into())
            .or_insert(StreamContextOption::Other(PhpMixed::Null));
        if let StreamContextOption::Other(value) = entry {
            value.offset_set_target()?;
        }
        Ok(entry)
    }

    pub fn set_retry_auth_failure(&mut self, retry_auth_failure: bool) {
        self.insert("retry-auth-failure", PhpMixed::Bool(retry_auth_failure));
    }

    /// `max_file_size`, when it is set. Its consumers declare it `?int` under strict_types, and
    /// `parameter` names the one that receives it (e.g.
    /// `stream_get_contents(): Argument #2 ($length)`) for the TypeError any other value raises.
    pub fn max_file_size(&self, parameter: impl AsRef<[u8]>) -> anyhow::Result<Option<i64>> {
        match self.get("max_file_size") {
            None | Some(PhpMixed::Null) => Ok(None),
            Some(PhpMixed::Int(max_file_size)) => Ok(Some(max_file_size)),
            Some(other) => Err(TypeError::new(bconcat!(
                parameter.as_ref(),
                " must be of type ?int, ",
                get_debug_type(&other),
                " given"
            ))
            .into()),
        }
    }

    /// PHP's `array_replace_recursive($this, $replacement)`.
    pub fn replace_recursive(self, replacement: Self) -> Self {
        Self::from_array(array_replace_recursive(
            self.into_array(),
            replacement.into_array(),
        ))
    }
}

impl From<IndexMap<BString, PhpMixed>> for StreamContextOptions {
    fn from(array: IndexMap<BString, PhpMixed>) -> Self {
        Self::from_array(array)
    }
}

impl ToWire for StreamContextOptions {
    fn to_wire(self) -> PluginValue {
        PhpMixed::Array(self.into_array()).to_wire()
    }
}

impl FromWire for StreamContextOptions {
    fn from_wire(value: &PluginValue) -> Result<Self, PhpThrow> {
        IndexMap::<BString, PhpMixed>::from_wire(value).map(Self::from_array)
    }
}

impl StreamContextOption {
    fn from_mixed(key: &[u8], value: PhpMixed) -> Self {
        match (key, value) {
            (b"http", PhpMixed::Array(entries)) => Self::Http(HttpOptions { entries }),
            (b"ssl", PhpMixed::Array(entries)) => Self::Ssl(SslOptions { entries }),
            (_, value) => Self::Other(value),
        }
    }

    fn to_mixed(&self) -> PhpMixed {
        match self {
            Self::Http(http) => PhpMixed::Array(http.entries.clone()),
            Self::Ssl(ssl) => PhpMixed::Array(ssl.entries.clone()),
            Self::Other(value) => value.clone(),
        }
    }

    fn into_mixed(self) -> PhpMixed {
        match self {
            Self::Http(http) => PhpMixed::Array(http.entries),
            Self::Ssl(ssl) => PhpMixed::Array(ssl.entries),
            Self::Other(value) => value,
        }
    }
}

impl HttpOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: impl AsRef<[u8]>) -> Option<&PhpMixed> {
        self.entries.get(key.as_ref())
    }

    /// `$options['http'][$key]`, when it is set.
    fn isset(&self, key: &[u8]) -> Option<&PhpMixed> {
        self.entries.get(key).filter(|value| !value.is_null())
    }

    pub fn insert(&mut self, key: impl Into<BString>, value: PhpMixed) {
        self.entries.insert(key.into(), value);
    }

    pub fn remove(&mut self, key: impl AsRef<[u8]>) -> Option<PhpMixed> {
        self.entries.shift_remove(key.as_ref())
    }

    /// `header`, when it is set: a string of `\r\n`-separated lines or an array of lines, as the
    /// caller gave it.
    pub fn header(&self) -> Option<&PhpMixed> {
        self.isset(b"header")
    }

    pub fn set_header(&mut self, header: PhpMixed) {
        self.insert("header", header);
    }

    pub fn set_header_lines(&mut self, lines: Vec<BString>) {
        self.set_header(PhpMixed::List(
            lines.into_iter().map(PhpMixed::String).collect(),
        ));
    }

    /// `$options['http']['header'][] = $line`.
    pub fn push_header(&mut self, line: PhpMixed) -> anyhow::Result<()> {
        self.entries
            .entry("header".into())
            .or_insert(PhpMixed::Null)
            .push(line)
    }

    /// `$options['http']['header'] = (array) $options['http']['header']`, for a set header.
    pub fn cast_header_to_array(&mut self) {
        if let Some(header) = self.entries.get_mut(B("header"))
            && !header.is_null()
            && !matches!(header, PhpMixed::List(_) | PhpMixed::Array(_))
        {
            *header = PhpMixed::List(vec![std::mem::take(header)]);
        }
    }

    /// `implode($glue, $options['http']['header'])`, which fails unless the header is an array.
    pub fn implode_header(&self, glue: impl AsRef<[u8]>) -> anyhow::Result<BString> {
        implode_mixed(glue, self.get("header").unwrap_or(&PhpMixed::Null))
    }

    /// The header lines as `CURLOPT_HTTPHEADER` takes them: an array, each value converted to a
    /// string.
    pub fn header_lines(&self) -> anyhow::Result<Vec<BString>> {
        match self.get("header") {
            Some(PhpMixed::List(list)) => Ok(list.iter().map(php_to_string).collect()),
            Some(PhpMixed::Array(map)) => Ok(map.values().map(php_to_string).collect()),
            other => Err(TypeError::new(bconcat!(
                "curl_setopt(): The CURLOPT_HTTPHEADER option must have an array value, ",
                get_debug_type(other.unwrap_or(&PhpMixed::Null)),
                " given"
            ))
            .into()),
        }
    }

    pub fn method(&self) -> Option<&PhpMixed> {
        self.isset(b"method")
    }

    pub fn set_method(&mut self, method: impl Into<BString>) {
        self.insert("method", PhpMixed::String(method.into()));
    }

    pub fn content(&self) -> Option<&PhpMixed> {
        self.isset(b"content")
    }

    pub fn set_content(&mut self, content: impl Into<BString>) {
        self.insert("content", PhpMixed::String(content.into()));
    }

    pub fn timeout(&self) -> Option<&PhpMixed> {
        self.isset(b"timeout")
    }

    pub fn set_timeout(&mut self, timeout: i64) {
        self.insert("timeout", PhpMixed::Int(timeout));
    }
}

impl SslOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: impl AsRef<[u8]>) -> Option<&PhpMixed> {
        self.entries.get(key.as_ref())
    }

    /// `$options['ssl'][$key]`, when it is set.
    fn isset(&self, key: &[u8]) -> Option<&PhpMixed> {
        self.entries.get(key).filter(|value| !value.is_null())
    }

    pub fn insert(&mut self, key: impl Into<BString>, value: PhpMixed) {
        self.entries.insert(key.into(), value);
    }

    pub fn set_cafile(&mut self, cafile: impl Into<BString>) {
        self.insert("cafile", PhpMixed::String(cafile.into()));
    }

    pub fn set_capath(&mut self, capath: impl Into<BString>) {
        self.insert("capath", PhpMixed::String(capath.into()));
    }

    pub fn allow_self_signed(&self) -> Option<&PhpMixed> {
        self.isset(b"allow_self_signed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_php_shim::error_message;

    fn array<K: AsRef<[u8]>>(entries: Vec<(K, PhpMixed)>) -> PhpMixed {
        PhpMixed::Array(
            entries
                .into_iter()
                .map(|(key, value)| (BString::from(key.as_ref()), value))
                .collect(),
        )
    }

    fn options<K: AsRef<[u8]>>(entries: Vec<(K, PhpMixed)>) -> StreamContextOptions {
        match array(entries) {
            PhpMixed::Array(array) => StreamContextOptions::from_array(array),
            _ => unreachable!(),
        }
    }

    #[test]
    fn keeps_the_order_and_the_values_of_every_key() {
        let original = array(vec![
            ("ssl", array(vec![("verify_peer", PhpMixed::Int(0))])),
            ("custom", PhpMixed::List(vec![PhpMixed::Float(1.5)])),
            (
                "http",
                array(vec![
                    ("verify_peer", PhpMixed::Bool(false)),
                    ("header", PhpMixed::string("X-Foo: bar")),
                ]),
            ),
            ("max_file_size", PhpMixed::string("10")),
        ]);
        let PhpMixed::Array(entries) = original.clone() else {
            unreachable!()
        };
        let mut options = StreamContextOptions::from_array(entries);
        options.http_mut().unwrap().set_method("POST");

        let PhpMixed::Array(mut expected) = original else {
            unreachable!()
        };
        if let PhpMixed::Array(http) = &mut expected[2] {
            http.insert("method".into(), PhpMixed::string("POST"));
        }
        assert_eq!(
            PhpMixed::Array(expected),
            PhpMixed::Array(options.into_array())
        );
    }

    #[test]
    fn writes_to_a_missing_or_null_group_create_it() {
        let mut missing = StreamContextOptions::new();
        missing.http_mut().unwrap().set_timeout(10);
        let mut null = options(vec![("http", PhpMixed::Null)]);
        null.http_mut().unwrap().set_timeout(10);

        let expected = array(vec![("http", array(vec![("timeout", PhpMixed::Int(10))]))]);
        assert_eq!(expected, PhpMixed::Array(missing.into_array()));
        assert_eq!(expected, PhpMixed::Array(null.into_array()));
    }

    #[test]
    fn writes_to_a_scalar_group_fail() {
        let mut options = options(vec![("ssl", PhpMixed::Int(1))]);

        let error = options.ssl_mut().unwrap_err();

        assert_eq!(
            "Cannot use a scalar value as an array",
            error_message(&error)
        );
        assert!(options.ssl().is_none());
    }

    #[test]
    fn pushes_a_header_line_after_the_highest_integer_key() {
        let mut options = options(vec![(
            "http",
            array(vec![(
                "header",
                array(vec![
                    ("5", PhpMixed::string("A: a")),
                    ("ua", PhpMixed::string("B: b")),
                ]),
            )]),
        )]);

        options
            .http_mut()
            .unwrap()
            .push_header(PhpMixed::string("C: c"))
            .unwrap();

        assert_eq!(
            Some(&array(vec![
                ("5", PhpMixed::string("A: a")),
                ("ua", PhpMixed::string("B: b")),
                ("6", PhpMixed::string("C: c")),
            ])),
            options.http().unwrap().header()
        );
    }

    #[test]
    fn rejects_a_max_file_size_that_is_not_an_integer() {
        let options = options(vec![("max_file_size", PhpMixed::string("10"))]);

        let error = options
            .max_file_size("f(): Argument #1 ($size)")
            .unwrap_err();

        assert_eq!(
            "f(): Argument #1 ($size) must be of type ?int, string given",
            error_message(&error)
        );
    }
}
