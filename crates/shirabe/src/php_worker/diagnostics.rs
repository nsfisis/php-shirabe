//! What the `diagnose` command asks the PHP runtime, answered by the worker in one round trip.

use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_rpc::{cached, query};
use shirabe_php_shim::php_str;
use std::sync::OnceLock;

shirabe_php_rpc::plugin_value_record! {
    /// `curl_version()`, together with the `CURL_*` constants the `diagnose` command consults.
    /// Every `Option` field is `None` when the corresponding array key or constant is absent.
    #[derive(Debug)]
    pub struct Curl {
        pub version: BString,
        pub libz_version: Option<BString>,
        pub brotli_version: Option<BString>,
        pub ssl_version: Option<BString>,
        pub features: Option<i64>,
        pub version_zstd: Option<i64>,
        pub version_http2: Option<i64>,
        pub has_http_version_2_0: bool,
        pub version_http3: Option<i64>,
    }
}

shirabe_php_rpc::plugin_value_record! {
    /// Everything the `diagnose` command needs to know about the PHP runtime, fetched in a single
    /// round trip because the command would otherwise query the same runtime dozens of times.
    #[derive(Debug)]
    pub struct Diagnostics {
        pub php_version: BString,
        pub php_version_id: i64,
        /// `None` when `PHP_BINARY` is undefined.
        pub php_binary: Option<std::path::PathBuf>,
        /// `None` when `OPENSSL_VERSION_TEXT` is undefined.
        pub openssl_version_text: Option<BString>,
        /// `0` when `OPENSSL_VERSION_NUMBER` is undefined.
        pub openssl_version_number: i64,
        pub has_hhvm_version: bool,
        pub has_php_windows_version_build: bool,
        /// `Composer\XdebugHandler\XdebugHandler::isXdebugActive()`.
        pub xdebug_active: bool,
        /// `0` when the ionCube loader is not loaded.
        pub ioncube_loader_iversion: i64,
        /// Empty when the ionCube loader is not loaded.
        pub ioncube_loader_version: BString,
        /// `phpinfo(INFO_GENERAL)` output.
        pub phpinfo_general: BString,
        /// `None` when the curl extension is not loaded.
        pub curl: Option<Curl>,
        extensions: IndexMap<BString, bool>,
        functions: IndexMap<BString, bool>,
        /// PHP's `false` (no such setting) is `None`.
        ini_settings: IndexMap<BString, Option<BString>>,
    }
}

shirabe_php_rpc::plugin_value_record! {
    /// The names the `__shirabe_diagnose` payload reports on.
    struct DiagnoseQuery {
        extensions: Vec<BString>,
        functions: Vec<BString>,
        ini_settings: Vec<BString>,
    }
}

/// The extensions `Diagnostics::extension_loaded` can be asked about.
const DIAGNOSE_EXTENSIONS: &[&BStr] = &[
    php_str(b"apcu"),
    php_str(b"curl"),
    php_str(b"filter"),
    php_str(b"hash"),
    php_str(b"iconv"),
    php_str(b"ionCube Loader"),
    php_str(b"mbstring"),
    php_str(b"openssl"),
    php_str(b"Phar"),
    php_str(b"uopz"),
    php_str(b"zip"),
    php_str(b"zlib"),
];

/// The functions `Diagnostics::function_exists` can be asked about.
const DIAGNOSE_FUNCTIONS: &[&BStr] = &[
    php_str(b"disk_free_space"),
    php_str(b"json_decode"),
    php_str(b"proc_open"),
];

/// The settings `Diagnostics::ini_get` can be asked about.
const DIAGNOSE_INI_SETTINGS: &[&BStr] = &[
    php_str(b"allow_url_fopen"),
    php_str(b"apc.enable_cli"),
    php_str(b"uopz.disable"),
    php_str(b"uopz.exit"),
    php_str(b"xdebug.profiler_enabled"),
];

impl Diagnostics {
    /// PHP `extension_loaded($name)`. Only the extensions the worker reports can be asked about;
    /// any other name is a bug in the caller, not a missing extension.
    pub fn extension_loaded(&self, name: impl AsRef<[u8]>) -> bool {
        let name = name.as_ref().as_bstr();
        *self.extensions.get(name).unwrap_or_else(|| {
            panic!("PHP RPC: extension {name:?} is not reported by the diagnose payload")
        })
    }

    /// PHP `function_exists($name)`. See [`Diagnostics::extension_loaded`] for the fixed set of names.
    pub fn function_exists(&self, name: impl AsRef<[u8]>) -> bool {
        let name = name.as_ref().as_bstr();
        *self.functions.get(name).unwrap_or_else(|| {
            panic!("PHP RPC: function {name:?} is not reported by the diagnose payload")
        })
    }

    /// PHP `ini_get($option)`, with PHP's `false` (no such setting) mapped to `None`. See
    /// [`Diagnostics::extension_loaded`] for the fixed set of names.
    pub fn ini_get(&self, option: impl AsRef<[u8]>) -> Option<&BStr> {
        let option = option.as_ref().as_bstr();
        self.ini_settings
            .get(option)
            .unwrap_or_else(|| {
                panic!("PHP RPC: ini setting {option:?} is not reported by the diagnose payload")
            })
            .as_ref()
            .map(|value| value.as_bstr())
    }
}

static DIAGNOSTICS: OnceLock<Diagnostics> = OnceLock::new();

/// PHP runtime information for the `diagnose` command. The worker is queried once per process;
/// subsequent calls reuse the cached payload.
pub fn get_diagnostics() -> anyhow::Result<&'static Diagnostics> {
    cached(&DIAGNOSTICS, || {
        query(
            "__shirabe_diagnose",
            DiagnoseQuery {
                extensions: DIAGNOSE_EXTENSIONS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                functions: DIAGNOSE_FUNCTIONS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                ini_settings: DIAGNOSE_INI_SETTINGS
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shirabe_php_rpc::{get_php_binary, get_php_version};

    #[test]
    fn queries_diagnostics() {
        let diagnostics = get_diagnostics().unwrap();
        assert_eq!(diagnostics.php_version, get_php_version().unwrap().version);
        assert!(diagnostics.php_version_id >= 70205);
        let php_binary = get_php_binary().unwrap();
        assert_eq!(
            diagnostics.php_binary.as_deref(),
            Some(php_binary.as_path())
        );
        assert!(diagnostics.function_exists("json_decode"));
        assert!(!diagnostics.extension_loaded("ionCube Loader"));
        assert!(
            diagnostics.phpinfo_general.contains_str("PHP Version"),
            "expected phpinfo(INFO_GENERAL) output, got: {}",
            diagnostics.phpinfo_general.display_lossy(),
        );
    }
}
