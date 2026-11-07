//! ref: composer/src/Composer/Util/StreamContextFactory.php sha1:a9dd88a37c5f0e86a5f093e6a0e230ae27d69645

use crate::composer;
use crate::downloader::TransportException;
use crate::repository::PlatformRepository;
use crate::util::http::ProxyManager;
use crate::util::{Filesystem, HttpOptions, Platform, StreamContextOptions};
use bstr::BString;
use indexmap::IndexMap;
use shirabe_ca_bundle::CaBundle;
use shirabe_php_shim::{
    PhpMixed, TypeError, array_replace_recursive, bformat, explode, extension_loaded,
    get_debug_type, is_dir, path_from_bytes, path_to_bytes, php_uname, stream_context_create,
    stripos,
};

pub struct StreamContextFactory;

impl StreamContextFactory {
    /// Creates a context supporting HTTP proxies.
    pub fn get_context(
        url: impl AsRef<[u8]>,
        default_options: StreamContextOptions,
        default_params: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<Result<PhpMixed, Box<TransportException>>> {
        let mut options = StreamContextOptions::new();
        let mut http = HttpOptions::new();
        // specify defaults again to try and work better with curlwrappers enabled
        http.insert("follow_location", PhpMixed::Int(1));
        http.insert("max_redirects", PhpMixed::Int(20));
        options.set_http(http);

        options = options.replace_recursive(
            match Self::init_options(url, default_options.clone(), false)? {
                Ok(options) => options,
                Err(e) => return Ok(Err(e)),
            },
        );
        let mut default_options = default_options;
        if default_options.http().is_some() {
            default_options.http_mut().remove("header");
        }
        options = options.replace_recursive(default_options);

        if let Some(header) = options.http().and_then(HttpOptions::header).cloned() {
            let fixed = Self::fix_http_header_field(header)?;
            options.http_mut().set_header(fixed);
        }

        Ok(Ok(stream_context_create(
            &options.into_array(),
            Some(&default_params),
        )))
    }

    pub fn init_options(
        url: impl AsRef<[u8]>,
        mut options: StreamContextOptions,
        for_curl: bool,
    ) -> anyhow::Result<Result<StreamContextOptions, Box<TransportException>>> {
        let url = url.as_ref();
        // Make sure the headers are in an array form
        let http = options.http_mut();
        if http.header().is_none() {
            http.set_header(PhpMixed::List(vec![]));
        }
        if let Some(PhpMixed::String(header)) = http.header() {
            let lines = explode("\r\n", header);
            http.set_header_lines(lines);
        }

        // Add stream proxy options if there is a proxy
        if !for_curl {
            let proxy_manager = ProxyManager::get_instance();
            let proxy_manager = proxy_manager.as_ref().unwrap();
            let proxy = match proxy_manager.get_proxy_for_request(url) {
                Ok(proxy) => proxy,
                Err(e) => return Ok(Err(e)),
            };
            let proxy_options = proxy.get_context_options();
            if let Some(proxy_options) = proxy_options {
                let is_https_request = url.starts_with(b"https://");

                if proxy.is_secure() {
                    if !extension_loaded("openssl") {
                        return Ok(Err(Box::new(TransportException::new(
                            BString::from(
                                "You must enable the openssl extension to use a secure proxy.",
                            ),
                            0,
                        ))));
                    }
                    if is_https_request {
                        return Ok(Err(Box::new(TransportException::new(
                            BString::from(
                                "You must enable the curl extension to make https requests through a secure proxy.",
                            ),
                            0,
                        ))));
                    }
                } else if is_https_request && !extension_loaded("openssl") {
                    return Ok(Err(Box::new(TransportException::new(
                        BString::from(
                            "You must enable the openssl extension to make https requests through a proxy.",
                        ),
                        0,
                    ))));
                }

                let mut proxy_options = StreamContextOptions::from_array(
                    proxy_options
                        .iter()
                        .map(|(group, entries)| (group.clone(), PhpMixed::Array(entries.clone())))
                        .collect(),
                )
                .expect("every proxy context option group is an array");
                // Header will be a Proxy-Authorization string or not set
                if let Some(header) = proxy_options.http().and_then(HttpOptions::header).cloned() {
                    options.http_mut().push_header(header)?;
                    proxy_options.http_mut().remove("header");
                }
                options = options.replace_recursive(proxy_options);
            }
        }

        let php = shirabe_php_rpc::get_php_version()?;
        let php_version = bformat!("PHP {}.{}.{}", php.major, php.minor, php.release);

        let http_version = if for_curl {
            // PHP reports `cURL <version>` here. Shirabe's "curl" transport is backed by reqwest,
            // so name reqwest as the HTTP stack in the User-Agent.
            "reqwest"
        } else {
            "streams"
        };

        let has_user_agent = match options.http() {
            Some(http) if http.header().is_some() => {
                stripos(http.implode_header("")?, "user-agent").is_some()
            }
            _ => false,
        };

        if !has_user_agent {
            let platform_php_version = PlatformRepository::get_platform_php_version();
            let user_agent = bformat!(
                "User-Agent: Composer/{} ({}; {}; {php_version}; {http_version}{}{})",
                composer::get_version(),
                php_uname("s"),
                php_uname("r"),
                platform_php_version
                    .map(|v| bformat!("; Platform-PHP {v}"))
                    .unwrap_or_default(),
                if Platform::get_env("CI").is_some_and(|v| !v.is_empty() && v != "0") {
                    "; CI"
                } else {
                    ""
                }
            );
            options
                .http_mut()
                .push_header(PhpMixed::string(user_agent))?;
        }

        Ok(Ok(options))
    }

    pub fn get_tls_defaults(
        options: &StreamContextOptions,
        // `logger` was a PSR LoggerInterface; CaBundle is slated for removal so
        // it is now an unused `()` placeholder.
        logger: (),
    ) -> anyhow::Result<StreamContextOptions> {
        let ciphers = [
            "ECDHE-RSA-AES128-GCM-SHA256",
            "ECDHE-ECDSA-AES128-GCM-SHA256",
            "ECDHE-RSA-AES256-GCM-SHA384",
            "ECDHE-ECDSA-AES256-GCM-SHA384",
            "DHE-RSA-AES128-GCM-SHA256",
            "DHE-DSS-AES128-GCM-SHA256",
            "kEDH+AESGCM",
            "ECDHE-RSA-AES128-SHA256",
            "ECDHE-ECDSA-AES128-SHA256",
            "ECDHE-RSA-AES128-SHA",
            "ECDHE-ECDSA-AES128-SHA",
            "ECDHE-RSA-AES256-SHA384",
            "ECDHE-ECDSA-AES256-SHA384",
            "ECDHE-RSA-AES256-SHA",
            "ECDHE-ECDSA-AES256-SHA",
            "DHE-RSA-AES128-SHA256",
            "DHE-RSA-AES128-SHA",
            "DHE-DSS-AES128-SHA256",
            "DHE-RSA-AES256-SHA256",
            "DHE-DSS-AES256-SHA",
            "DHE-RSA-AES256-SHA",
            "AES128-GCM-SHA256",
            "AES256-GCM-SHA384",
            "AES128-SHA256",
            "AES256-SHA256",
            "AES128-SHA",
            "AES256-SHA",
            "AES",
            "CAMELLIA",
            "!aNULL",
            "!eNULL",
            "!EXPORT",
            "!DES",
            "!3DES",
            "!RC4",
            "!MD5",
            "!PSK",
            "!aECDH",
            "!EDH-DSS-DES-CBC3-SHA",
            "!EDH-RSA-DES-CBC3-SHA",
            "!KRB5-DES-CBC3-SHA",
        ]
        .join(":");

        // CN_match and SNI_server_name are only known once a URL is passed.
        // They will be set in the getOptionsForUrl() method which receives a URL.
        //
        // cafile or capath can be overridden by passing in those options to constructor.
        let mut ssl: IndexMap<BString, PhpMixed> = IndexMap::new();
        ssl.insert("ciphers".into(), PhpMixed::string(ciphers));
        ssl.insert("verify_peer".into(), PhpMixed::Bool(true));
        ssl.insert("verify_depth".into(), PhpMixed::Int(7));
        ssl.insert("SNI_enabled".into(), PhpMixed::Bool(true));
        ssl.insert("capture_peer_cert".into(), PhpMixed::Bool(true));

        if let Some(ssl_options) = options.ssl() {
            ssl = array_replace_recursive(ssl, ssl_options.to_array());
        }

        // Attempt to find a local cafile or throw an exception if none pre-set.
        // The user may go download one if this occurs.
        let isset = |ssl: &IndexMap<BString, PhpMixed>, key: &[u8]| {
            ssl.get(key).filter(|value| !value.is_null()).cloned()
        };
        if isset(&ssl, b"cafile").is_none() && isset(&ssl, b"capath").is_none() {
            let result = CaBundle::get_system_ca_root_bundle_path(logger);
            let key = if is_dir(&result) { "capath" } else { "cafile" };
            ssl.insert(key.into(), PhpMixed::string(path_to_bytes(&result)));
        }

        if let Some(cafile) = isset(&ssl, b"cafile") {
            let PhpMixed::String(cafile) = cafile else {
                return Err(TypeError::new(bformat!("Composer\\Util\\Filesystem::isReadable(): Argument #1 ($path) must be of type string, {} given", get_debug_type(&cafile)))
                .into());
            };
            let cafile = path_from_bytes(&cafile);
            if !Filesystem::is_readable(cafile) || !CaBundle::validate_ca_file(cafile, logger) {
                return Err(TransportException::new(
                    BString::from("The configured cafile was not valid or could not be read."),
                    0,
                )
                .into());
            }
        }

        if let Some(capath) = isset(&ssl, b"capath") {
            let PhpMixed::String(capath) = capath else {
                return Err(TypeError::new(bformat!(
                    "is_dir(): Argument #1 ($filename) must be of type string, {} given",
                    get_debug_type(&capath)
                ))
                .into());
            };
            let capath = path_from_bytes(&capath);
            if !is_dir(capath) || !Filesystem::is_readable(capath) {
                return Err(TransportException::new(
                    BString::from("The configured capath was not valid or could not be read."),
                    0,
                )
                .into());
            }
        }

        // Disable TLS compression to prevent CRIME attacks where supported.
        ssl.insert("disable_compression".into(), PhpMixed::Bool(true));

        let mut defaults = StreamContextOptions::new();
        defaults.insert("ssl", PhpMixed::Array(ssl))?;
        Ok(defaults)
    }

    /// PHP's fixHttpHeaderField() also moves the content-type header to the end of the array, to
    /// work around a bug of PHP's http stream wrapper (https://bugs.php.net/bug.php?id=61548).
    /// Requests are sent through reqwest, which does not have the bug, so the headers keep their
    /// order.
    fn fix_http_header_field(header: PhpMixed) -> anyhow::Result<PhpMixed> {
        let header = match header {
            PhpMixed::String(s) => PhpMixed::List(
                explode("\r\n", s)
                    .into_iter()
                    .map(PhpMixed::String)
                    .collect(),
            ),
            header @ (PhpMixed::List(_) | PhpMixed::Array(_)) => header,
            other => {
                return Err(TypeError::new(bformat!(
                    "explode(): Argument #2 ($string) must be of type string, {} given",
                    get_debug_type(&other)
                ))
                .into());
            }
        };
        // PHP's comparator runs only when there are two elements to compare, and the file declares
        // strict_types, so stripos() rejects any element that is not a string.
        if header.values().len() > 1
            && let Some(other) = header
                .values()
                .into_iter()
                .find(|value| !matches!(value, PhpMixed::String(_)))
        {
            return Err(TypeError::new(bformat!(
                "stripos(): Argument #1 ($haystack) must be of type string, {} given",
                get_debug_type(other)
            ))
            .into());
        }
        Ok(header)
    }
}
