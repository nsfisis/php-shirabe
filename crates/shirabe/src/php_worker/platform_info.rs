//! What `PlatformRepository` asks the PHP runtime, answered by the worker in one round trip.
//! `Composer\Platform\Runtime` itself has no Rust counterpart: its work belongs to the running
//! PHP interpreter, so it is ported as PHP into the worker (`ShirabePlatformRuntime`).

use bstr::{B, BStr, BString, ByteSlice as _};
use indexmap::IndexMap;
use shirabe_php_rpc::{cached, query};
use shirabe_php_shim::{PhpMixed, bformat, php_str};

/// The constants `PlatformInfo::has_constant` can be asked about.
const PLATFORM_CONSTANTS: &[&BStr] = &[
    php_str(b"PHP_VERSION"),
    php_str(b"PHP_DEBUG"),
    php_str(b"PHP_ZTS"),
    php_str(b"PHP_INT_SIZE"),
    php_str(b"AF_INET6"),
    php_str(b"GD_VERSION"),
    php_str(b"GMP_VERSION"),
    php_str(b"ICONV_VERSION"),
    php_str(b"INTL_ICU_VERSION"),
    php_str(b"LIBXML_DOTTED_VERSION"),
    php_str(b"MB_ONIGURUMA_VERSION"),
    php_str(b"OPENSSL_VERSION_TEXT"),
    php_str(b"PCRE_VERSION"),
    php_str(b"PGSQL_LIBPQ_VERSION"),
    php_str(b"RD_KAFKA_VERSION"),
    php_str(b"SODIUM_LIBRARY_VERSION"),
    php_str(b"LIBXSLT_DOTTED_VERSION"),
    php_str(b"ZipArchive::LIBZIP_VERSION"),
    php_str(b"ZLIB_VERSION"),
];

/// The classes `PlatformInfo::has_class` can be asked about.
const PLATFORM_CLASSES: &[&BStr] = &[php_str(b"ResourceBundle"), php_str(b"IntlChar")];

/// The extensions whose `ReflectionExtension::info()` output `PlatformInfo::get_extension_info`
/// can be asked about, when they are loaded.
const PLATFORM_EXTENSION_INFO: &[&BStr] = &[
    php_str(b"amqp"),
    php_str(b"bz2"),
    php_str(b"curl"),
    php_str(b"date"),
    php_str(b"fileinfo"),
    php_str(b"gd"),
    php_str(b"intl"),
    php_str(b"ldap"),
    php_str(b"mbstring"),
    php_str(b"memcached"),
    php_str(b"mongodb"),
    php_str(b"mysqlnd"),
    php_str(b"pcre"),
    php_str(b"pdo_mysql"),
    php_str(b"pdo_pgsql"),
    php_str(b"pdo_sqlite"),
    php_str(b"pgsql"),
    php_str(b"pq"),
    php_str(b"sqlite3"),
    php_str(b"ssh2"),
    php_str(b"xsl"),
    php_str(b"yaml"),
    php_str(b"zlib"),
];

shirabe_php_rpc::plugin_value_record! {
    /// The names the `__shirabe_platform` payload reports on.
    struct PlatformQuery {
        constants: Vec<BString>,
        classes: Vec<BString>,
        extension_info: Vec<BString>,
    }
}

shirabe_php_rpc::plugin_value_record! {
    /// The answer to the `__shirabe_platform` query, which [`PlatformInfo`] is built from.
    struct PlatformPayload {
        extensions: Vec<BString>,
        extension_versions: IndexMap<BString, BString>,
        extension_info: IndexMap<BString, BString>,
        /// Only the defined constants.
        constants: IndexMap<BString, PhpMixed>,
        classes: IndexMap<BString, bool>,
        curl_version: Option<BString>,
        inet_pton_ipv6: Option<BString>,
        resource_bundle_version: Option<BString>,
        intl_char_unicode_version: Option<Vec<i64>>,
        imagick_version_string: Option<BString>,
    }
}

/// Everything `PlatformRepository` needs to know about the PHP runtime, fetched in a single round
/// trip because it would otherwise query the same runtime once per extension and constant.
///
/// The fields standing in for PHP calls and objects carry the entries the consumer reads off their
/// results rather than the results themselves.
#[derive(Debug, Clone)]
pub struct PlatformInfo {
    extensions: Vec<BString>,
    extension_versions: IndexMap<BString, BString>,
    extension_info: IndexMap<BString, BString>,
    /// Keyed as `ltrim($class.'::'.$constant, ':')`; `None` for a reported but undefined constant.
    constants: IndexMap<BString, Option<PhpMixed>>,
    classes: IndexMap<BString, bool>,
    /// `curl_version()['version']`, or `None` when the curl extension is not loaded.
    pub curl_version: Option<BString>,
    /// `@inet_pton('::')`, with PHP's `false` mapped to `None`.
    pub inet_pton_ipv6: Option<Vec<u8>>,
    /// `ResourceBundle::create('root', 'ICUDATA', false)->get('Version')`, or `None` when the class
    /// is absent or the bundle cannot be opened.
    pub resource_bundle_version: Option<BString>,
    /// `IntlChar::getUnicodeVersion()`, or `None` when the class is absent.
    pub intl_char_unicode_version: Option<Vec<i64>>,
    /// `(new Imagick())->getVersion()['versionString']`, or `None` when the extension is not
    /// loaded.
    pub imagick_version_string: Option<BString>,
}

impl Default for PlatformInfo {
    /// A runtime with no extensions loaded, no classes defined and every reported constant
    /// undefined.
    fn default() -> Self {
        PlatformInfo {
            extensions: Vec::new(),
            extension_versions: IndexMap::new(),
            extension_info: IndexMap::new(),
            constants: PLATFORM_CONSTANTS
                .iter()
                .map(|name| ((*name).to_owned(), None))
                .collect(),
            classes: PLATFORM_CLASSES
                .iter()
                .map(|name| ((*name).to_owned(), false))
                .collect(),
            curl_version: None,
            inet_pton_ipv6: None,
            resource_bundle_version: None,
            intl_char_unicode_version: None,
            imagick_version_string: None,
        }
    }
}

impl PlatformInfo {
    /// PHP `get_loaded_extensions()`.
    pub fn get_extensions(&self) -> &[BString] {
        &self.extensions
    }

    /// PHP `phpversion($extension)`, with PHP's `false` mapped to `'0'`.
    pub fn get_extension_version(&self, extension: impl AsRef<[u8]>) -> &BStr {
        let extension = extension.as_ref().as_bstr();
        self.extension_versions
            .get(extension)
            .unwrap_or_else(|| {
                panic!("PHP RPC: extension {extension:?} is not loaded in the platform payload")
            })
            .as_bstr()
    }

    /// PHP `(new \ReflectionExtension($extension))->info()` output. Only the extensions the worker
    /// reports can be asked about; any other name is a bug in the caller.
    pub fn get_extension_info(&self, extension: impl AsRef<[u8]>) -> &BStr {
        let extension = extension.as_ref().as_bstr();
        self.extension_info
            .get(extension)
            .unwrap_or_else(|| {
                panic!("PHP RPC: extension {extension:?} info is not in the platform payload")
            })
            .as_bstr()
    }

    /// PHP `defined(ltrim($class.'::'.$constant, ':'))`. Only the constants the worker reports can
    /// be asked about; any other name is a bug in the caller, not an undefined constant.
    pub fn has_constant(&self, constant_name: impl AsRef<[u8]>, class: Option<&BStr>) -> bool {
        self.constant(constant_name, class).is_some()
    }

    /// PHP `constant(ltrim($class.'::'.$constant, ':'))`, answering null for an undefined
    /// constant. See [`PlatformInfo::has_constant`] for the fixed set of names.
    pub fn get_constant(&self, constant_name: impl AsRef<[u8]>, class: Option<&BStr>) -> PhpMixed {
        self.constant(constant_name, class)
            .cloned()
            .unwrap_or(PhpMixed::Null)
    }

    /// PHP `class_exists($class, false)`. See [`PlatformInfo::has_constant`] for the fixed set of
    /// names.
    pub fn has_class(&self, class: impl AsRef<[u8]>) -> bool {
        let class = class.as_ref().as_bstr();
        *self.classes.get(class).unwrap_or_else(|| {
            panic!("PHP RPC: class {class:?} is not reported by the platform payload")
        })
    }

    /// For testing only: reports `extensions` as loaded, each at `version`.
    pub fn __set_extensions(
        &mut self,
        extensions: impl IntoIterator<Item = impl Into<BString>>,
        version: impl AsRef<[u8]>,
    ) {
        let extensions: Vec<BString> = extensions.into_iter().map(Into::into).collect();
        self.extension_versions = extensions
            .iter()
            .map(|name| (name.clone(), BString::from(version.as_ref())))
            .collect();
        self.extensions = extensions;
    }

    /// For testing only: reports `info` as the `ReflectionExtension::info()` output of `extension`.
    pub fn __set_extension_info(&mut self, extension: impl AsRef<[u8]>, info: impl AsRef<[u8]>) {
        self.extension_info.insert(
            BString::from(extension.as_ref()),
            BString::from(info.as_ref()),
        );
    }

    /// For testing only: reports the constant as defined with `value`. Panics on a constant the
    /// worker does not report, so a test cannot describe a runtime the worker cannot report.
    pub fn __set_constant(
        &mut self,
        constant_name: impl AsRef<[u8]>,
        class: Option<&BStr>,
        value: PhpMixed,
    ) {
        let key = Self::constant_key(constant_name, class);
        let entry = self.constants.get_mut(&key).unwrap_or_else(|| {
            panic!("PHP RPC: constant {key:?} is not reported by the platform payload")
        });
        *entry = Some(value);
    }

    /// For testing only: reports the class as defined. See [`PlatformInfo::__set_constant`].
    pub fn __set_class(&mut self, class: impl AsRef<[u8]>) {
        let class = class.as_ref().as_bstr();
        let entry = self.classes.get_mut(class).unwrap_or_else(|| {
            panic!("PHP RPC: class {class:?} is not reported by the platform payload")
        });
        *entry = true;
    }

    fn constant(&self, constant_name: impl AsRef<[u8]>, class: Option<&BStr>) -> Option<&PhpMixed> {
        let key = Self::constant_key(constant_name, class);
        self.constants
            .get(&key)
            .unwrap_or_else(|| {
                panic!("PHP RPC: constant {key:?} is not reported by the platform payload")
            })
            .as_ref()
    }

    fn constant_key(constant_name: impl AsRef<[u8]>, class: Option<&BStr>) -> BString {
        let constant_name = constant_name.as_ref();
        match class {
            Some(class) => bformat!("{class}::{constant_name}"),
            None => BString::from(constant_name),
        }
    }
}

static PLATFORM_INFO: std::sync::OnceLock<PlatformInfo> = std::sync::OnceLock::new();

/// PHP runtime information for `PlatformRepository`. The worker is queried once per process;
/// subsequent calls reuse the cached payload.
pub fn get_platform_info() -> anyhow::Result<&'static PlatformInfo> {
    cached(&PLATFORM_INFO, || {
        let payload: PlatformPayload = query(
            "__shirabe_platform",
            PlatformQuery {
                constants: PLATFORM_CONSTANTS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                classes: PLATFORM_CLASSES
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                extension_info: PLATFORM_EXTENSION_INFO
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            },
        )?;
        Ok(PlatformInfo {
            extensions: payload.extensions,
            extension_versions: payload.extension_versions,
            extension_info: payload.extension_info,
            constants: PLATFORM_CONSTANTS
                .iter()
                .map(|name| ((*name).to_owned(), payload.constants.get(B(name)).cloned()))
                .collect(),
            classes: PLATFORM_CLASSES
                .iter()
                .map(|name| {
                    let exists = *payload.classes.get(B(name)).unwrap_or_else(|| {
                        panic!("PHP RPC: the platform payload does not report the class {name:?}")
                    });
                    ((*name).to_owned(), exists)
                })
                .collect(),
            curl_version: payload.curl_version,
            inet_pton_ipv6: payload.inet_pton_ipv6.map(Vec::from),
            resource_bundle_version: payload.resource_bundle_version,
            intl_char_unicode_version: payload.intl_char_unicode_version,
            imagick_version_string: payload.imagick_version_string,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_php_rpc::get_php_version;

    #[test]
    fn queries_platform_info() {
        let platform_info = get_platform_info().unwrap();

        let extensions = platform_info.get_extensions();
        assert!(
            extensions.iter().any(|extension| extension == "Core"),
            "expected the Core extension among {extensions:?}",
        );
        assert_eq!(
            platform_info.get_extension_version("Core"),
            get_php_version().unwrap().version
        );

        assert!(platform_info.has_constant("PHP_VERSION", None));
        assert_eq!(
            platform_info.get_constant("PHP_INT_SIZE", None),
            PhpMixed::Int(8)
        );
        match platform_info.get_constant("PHP_VERSION", None) {
            PhpMixed::String(s) => assert!(!s.is_empty(), "expected a non-empty PHP_VERSION"),
            other => panic!("expected a string, got {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "is not reported by the platform payload")]
    fn platform_info_rejects_an_unreported_constant() {
        PlatformInfo::default().has_constant("SHIRABE_DOES_NOT_EXIST_XYZ", None);
    }
}
