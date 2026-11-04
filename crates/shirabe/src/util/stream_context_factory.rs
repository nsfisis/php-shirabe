//! ref: composer/src/Composer/Util/StreamContextFactory.php sha1:a9dd88a37c5f0e86a5f093e6a0e230ae27d69645

use crate::composer;
use crate::downloader::TransportException;
use crate::repository::PlatformRepository;
use crate::util::http::ProxyManager;
use crate::util::{Filesystem, Platform};
use bstr::{B, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_ca_bundle::CaBundle;
use shirabe_php_shim::{
    PhpMixed, TypeError, array_replace_recursive, bconcat, explode, extension_loaded,
    get_debug_type, implode_mixed, is_dir, path_from_bytes, path_to_bytes, php_uname,
    stream_context_create, stripos, uasort, uasort_map,
};

pub struct StreamContextFactory;

impl StreamContextFactory {
    /// Creates a context supporting HTTP proxies.
    pub fn get_context(
        url: impl AsRef<[u8]>,
        default_options: IndexMap<BString, PhpMixed>,
        default_params: IndexMap<BString, PhpMixed>,
    ) -> anyhow::Result<Result<PhpMixed, Box<TransportException>>> {
        let mut options: IndexMap<BString, PhpMixed> = {
            let mut http = IndexMap::new();
            // specify defaults again to try and work better with curlwrappers enabled
            http.insert("follow_location".into(), PhpMixed::Int(1));
            http.insert("max_redirects".into(), PhpMixed::Int(20));
            let mut o = IndexMap::new();
            o.insert("http".into(), PhpMixed::Array(http.into_iter().collect()));
            o
        };

        options = array_replace_recursive(
            options,
            match Self::init_options(url, default_options.clone(), false)? {
                Ok(options) => options,
                Err(e) => return Ok(Err(e)),
            },
        );
        let default_options = {
            let mut o = default_options;
            if let Some(PhpMixed::Array(http)) = o.get_mut(B("http")) {
                http.shift_remove(B("header"));
            }
            o
        };
        options = array_replace_recursive(options, default_options);

        if let Some(PhpMixed::Array(http)) = options.get_mut(B("http"))
            && let Some(header) = http.get(B("header")).filter(|header| !header.is_null())
        {
            let fixed = Self::fix_http_header_field(header.clone())?;
            http.insert("header".into(), fixed);
        }

        Ok(Ok(stream_context_create(&options, Some(&default_params))))
    }

    pub fn init_options(
        url: impl AsRef<[u8]>,
        mut options: IndexMap<BString, PhpMixed>,
        for_curl: bool,
    ) -> anyhow::Result<Result<IndexMap<BString, PhpMixed>, Box<TransportException>>> {
        let url = url.as_ref();
        // Make sure the headers are in an array form
        let http = options
            .entry(BString::from("http"))
            .or_insert(PhpMixed::Null);
        if http.get(B("header")).is_none_or(PhpMixed::is_null) {
            http.offset_set_target()?
                .insert("header".into(), PhpMixed::List(vec![]));
        }
        let http = http
            .as_array_mut()
            .expect("`http` holds a header, so it is an array");
        if let Some(PhpMixed::String(header_str)) = http.get(B("header")).cloned() {
            let parts: Vec<PhpMixed> = explode("\r\n", &header_str)
                .into_iter()
                .map(PhpMixed::String)
                .collect();
            http.insert("header".into(), PhpMixed::List(parts));
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

                // Header will be a Proxy-Authorization string or not set
                let proxy_http = proxy_options.get(bstr::B("http"));
                if let Some(proxy_header) = proxy_http.and_then(|h| h.get(B("header"))) {
                    Self::http_header_mut(&mut options).push(proxy_header.clone())?;
                }

                let proxy_options_flat: IndexMap<BString, PhpMixed> = proxy_options
                    .iter()
                    .map(|(k, v)| {
                        let inner: IndexMap<BString, PhpMixed> = v
                            .iter()
                            .filter(|(ik, _)| *ik != "header")
                            .map(|(ik, iv)| (ik.clone(), iv.clone()))
                            .collect();
                        (k.as_bstr().into(), PhpMixed::Array(inner))
                    })
                    .collect();
                options = array_replace_recursive(options, proxy_options_flat);
            }
        }

        let php = shirabe_php_rpc::get_php_version()?;
        let php_version = bconcat!("PHP ", php.major, ".", php.minor, ".", php.release);

        let http_version = if for_curl {
            // PHP reports `cURL <version>` here. Shirabe's "curl" transport is backed by reqwest,
            // so name reqwest as the HTTP stack in the User-Agent.
            "reqwest"
        } else {
            "streams"
        };

        let has_user_agent = stripos(
            implode_mixed("", Self::http_header_mut(&mut options))?,
            "user-agent",
        )
        .is_some();

        if !has_user_agent {
            let platform_php_version = PlatformRepository::get_platform_php_version();
            let user_agent = bconcat!(
                "User-Agent: Composer/",
                composer::get_version(),
                " (",
                php_uname("s"),
                "; ",
                php_uname("r"),
                "; ",
                php_version,
                "; ",
                http_version,
                platform_php_version
                    .map(|v| bconcat!("; Platform-PHP ", v))
                    .unwrap_or_default(),
                if Platform::get_env("CI").is_some_and(|v| !v.is_empty() && v != "0") {
                    "; CI"
                } else {
                    ""
                },
                ")"
            );
            Self::http_header_mut(&mut options).push(PhpMixed::string(user_agent))?;
        }

        Ok(Ok(options))
    }

    pub fn get_tls_defaults(
        options: &IndexMap<BString, PhpMixed>,
        // `logger` was a PSR LoggerInterface; CaBundle is slated for removal so
        // it is now an unused `()` placeholder.
        logger: (),
    ) -> anyhow::Result<IndexMap<BString, PhpMixed>, Box<TransportException>> {
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
        let ssl_defaults: IndexMap<BString, PhpMixed> = {
            let mut ssl = IndexMap::new();
            ssl.insert("ciphers".into(), PhpMixed::string(ciphers));
            ssl.insert("verify_peer".into(), PhpMixed::Bool(true));
            ssl.insert("verify_depth".into(), PhpMixed::Int(7));
            ssl.insert("SNI_enabled".into(), PhpMixed::Bool(true));
            ssl.insert("capture_peer_cert".into(), PhpMixed::Bool(true));
            ssl
        };

        let mut defaults: IndexMap<BString, PhpMixed> = {
            let mut d = IndexMap::new();
            d.insert(
                "ssl".into(),
                PhpMixed::Array(ssl_defaults.into_iter().collect()),
            );
            d
        };

        if let Some(ssl_options) = options.get(B("ssl"))
            && let Some(ssl_defaults_mixed) = defaults.get(B("ssl")).cloned()
        {
            let merged = array_replace_recursive(
                match ssl_defaults_mixed {
                    PhpMixed::Array(a) => a,
                    _ => IndexMap::new(),
                },
                match ssl_options.clone() {
                    PhpMixed::Array(a) => a,
                    _ => IndexMap::new(),
                },
            );
            defaults.insert("ssl".into(), PhpMixed::Array(merged));
        }

        // Attempt to find a local cafile or throw an exception if none pre-set.
        // The user may go download one if this occurs.
        let ssl = defaults.get(B("ssl")).and_then(|v| v.as_array());
        let has_cafile = ssl.as_ref().and_then(|a| a.get(B("cafile"))).is_some();
        let has_capath = ssl.as_ref().and_then(|a| a.get(B("capath"))).is_some();
        if !has_cafile && !has_capath {
            let result = CaBundle::get_system_ca_root_bundle_path(logger);
            let is_dir = is_dir(&result);
            let result = BString::from(path_to_bytes(&result));
            if is_dir {
                if let Some(PhpMixed::Array(ssl)) = defaults.get_mut(B("ssl")) {
                    ssl.insert("capath".into(), PhpMixed::string(result));
                }
            } else {
                if let Some(PhpMixed::Array(ssl)) = defaults.get_mut(B("ssl")) {
                    ssl.insert("cafile".into(), PhpMixed::string(result));
                }
            }
        }

        let cafile = defaults
            .get(B("ssl"))
            .and_then(|v| v.as_array())
            .and_then(|a| a.get(B("cafile")))
            .and_then(|v| v.as_string())
            .map(path_from_bytes);
        if let Some(cafile) = cafile
            && (!Filesystem::is_readable(cafile) || !CaBundle::validate_ca_file(cafile, logger))
        {
            return Err(Box::new(TransportException::new(
                BString::from("The configured cafile was not valid or could not be read."),
                0,
            )));
        }

        let capath = defaults
            .get(B("ssl"))
            .and_then(|v| v.as_array())
            .and_then(|a| a.get(B("capath")))
            .and_then(|v| v.as_string())
            .map(path_from_bytes);
        if let Some(capath) = capath
            && (!is_dir(capath) || !Filesystem::is_readable(capath))
        {
            return Err(Box::new(TransportException::new(
                BString::from("The configured capath was not valid or could not be read."),
                0,
            )));
        }

        // Disable TLS compression to prevent CRIME attacks where supported.
        if let Some(PhpMixed::Array(ssl)) = defaults.get_mut(B("ssl")) {
            ssl.insert("disable_compression".into(), PhpMixed::Bool(true));
        }

        Ok(defaults)
    }

    /// `$options['http']['header']`, which the start of `init_options` has set.
    fn http_header_mut(options: &mut IndexMap<BString, PhpMixed>) -> &mut PhpMixed {
        options
            .get_mut(B("http"))
            .and_then(PhpMixed::as_array_mut)
            .and_then(|http| http.get_mut(B("header")))
            .expect("init_options sets http.header before reading it")
    }

    /// A bug in PHP prevents the headers from correctly being sent when a content-type header is
    /// present and NOT at the end of the array. This method fixes the array by moving the
    /// content-type header to the end.
    fn fix_http_header_field(header: PhpMixed) -> anyhow::Result<PhpMixed> {
        let mut header = match header {
            PhpMixed::String(s) => PhpMixed::List(
                explode("\r\n", s)
                    .into_iter()
                    .map(PhpMixed::String)
                    .collect(),
            ),
            header @ (PhpMixed::List(_) | PhpMixed::Array(_)) => header,
            other => {
                return Err(TypeError::new(bconcat!(
                    "explode(): Argument #2 ($string) must be of type string, ",
                    get_debug_type(&other),
                    " given"
                ))
                .into());
            }
        };
        // The comparator runs only when there are two elements to compare, and the file declares
        // strict_types, so stripos() rejects any element that is not a string.
        if header.values().len() > 1
            && let Some(other) = header
                .values()
                .into_iter()
                .find(|value| !matches!(value, PhpMixed::String(_)))
        {
            return Err(TypeError::new(bconcat!(
                "stripos(): Argument #1 ($haystack) must be of type string, ",
                get_debug_type(other),
                " given"
            ))
            .into());
        }
        let compare = |el: &PhpMixed, _: &PhpMixed| {
            if stripos(el.as_string().unwrap_or_default(), "content-type") == Some(0) {
                1
            } else {
                -1
            }
        };
        match &mut header {
            PhpMixed::List(list) => uasort(list, compare),
            PhpMixed::Array(map) => uasort_map(map, compare),
            _ => unreachable!("header was made an array above"),
        }
        Ok(header)
    }
}
