//! ref: composer/src/Composer/Util/StreamContextFactory.php

use indexmap::IndexMap;
use shirabe_external_packages::composer::ca_bundle::ca_bundle::CaBundle;
use shirabe_external_packages::psr::log::logger_interface::LoggerInterface;
use shirabe_php_shim::{
    HHVM_VERSION, PHP_MAJOR_VERSION, PHP_MINOR_VERSION, PHP_RELEASE_VERSION, PhpMixed,
    RuntimeException, array_replace_recursive, curl_version, extension_loaded, function_exists,
    php_uname, stream_context_create, stripos, uasort,
};

use crate::composer::Composer;
use crate::downloader::transport_exception::TransportException;
use crate::repository::platform_repository::PlatformRepository;
use crate::util::filesystem::Filesystem;
use crate::util::http::proxy_manager::ProxyManager;
use crate::util::platform::Platform;

pub struct StreamContextFactory;

impl StreamContextFactory {
    /// Creates a context supporting HTTP proxies.
    pub fn get_context(
        url: &str,
        default_options: IndexMap<String, PhpMixed>,
        default_params: IndexMap<String, PhpMixed>,
    ) -> anyhow::Result<PhpMixed, TransportException> {
        let mut options: IndexMap<String, PhpMixed> = {
            let mut http = IndexMap::new();
            // specify defaults again to try and work better with curlwrappers enabled
            http.insert("follow_location".to_string(), PhpMixed::Int(1));
            http.insert("max_redirects".to_string(), PhpMixed::Int(20));
            let mut o = IndexMap::new();
            o.insert(
                "http".to_string(),
                PhpMixed::Array(http.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
            );
            o
        };

        options = array_replace_recursive(
            options,
            Self::init_options(url, default_options.clone(), false)?,
        );
        let default_options = {
            let mut o = default_options;
            if let Some(PhpMixed::Array(ref mut http)) = o.get_mut("http") {
                http.remove("header");
            }
            o
        };
        options = array_replace_recursive(options, default_options);

        if let Some(PhpMixed::Array(ref mut http)) = options.get_mut("http") {
            if let Some(header) = http.get("header").cloned() {
                let fixed = Self::fix_http_header_field(&*header);
                http.insert(
                    "header".to_string(),
                    Box::new(PhpMixed::List(
                        fixed
                            .into_iter()
                            .map(|s| Box::new(PhpMixed::String(s)))
                            .collect(),
                    )),
                );
            }
        }

        Ok(stream_context_create(&options, Some(&default_params)))
    }

    pub fn init_options(
        url: &str,
        mut options: IndexMap<String, PhpMixed>,
        for_curl: bool,
    ) -> anyhow::Result<IndexMap<String, PhpMixed>, TransportException> {
        // Make sure the headers are in an array form
        let has_header = options
            .get("http")
            .and_then(|v| v.as_array())
            .map(|a| a.contains_key("header"))
            .unwrap_or(false);
        if !has_header {
            if let Some(PhpMixed::Array(ref mut http)) = options.get_mut("http") {
                http.insert("header".to_string(), Box::new(PhpMixed::List(vec![])));
            }
        }
        // Convert string header to array
        let header_is_string = options
            .get("http")
            .and_then(|v| v.as_array())
            .and_then(|a| a.get("header"))
            .map(|v| matches!(**v, PhpMixed::String(_)))
            .unwrap_or(false);
        if header_is_string {
            if let Some(PhpMixed::Array(ref mut http)) = options.get_mut("http") {
                if let Some(PhpMixed::String(header_str)) = http.get("header").map(|v| *v.clone()) {
                    let parts: Vec<Box<PhpMixed>> = header_str
                        .split("\r\n")
                        .map(|s| Box::new(PhpMixed::String(s.to_string())))
                        .collect();
                    http.insert("header".to_string(), Box::new(PhpMixed::List(parts)));
                }
            }
        }

        // Add stream proxy options if there is a proxy
        if !for_curl {
            let proxy_manager = ProxyManager::get_instance().lock().unwrap();
            let proxy_manager = proxy_manager.as_ref().unwrap();
            let proxy = proxy_manager.get_proxy_for_request(url)?;
            let proxy_options = proxy.get_context_options();
            if let Some(proxy_options) = proxy_options {
                let is_https_request = url.starts_with("https://");

                if proxy.is_secure() {
                    if !extension_loaded("openssl") {
                        return Err(TransportException::new(
                            "You must enable the openssl extension to use a secure proxy."
                                .to_string(),
                        ));
                    }
                    if is_https_request {
                        return Err(TransportException::new(
                            "You must enable the curl extension to make https requests through a secure proxy.".to_string(),
                        ));
                    }
                } else if is_https_request && !extension_loaded("openssl") {
                    return Err(TransportException::new(
                        "You must enable the openssl extension to make https requests through a proxy.".to_string(),
                    ));
                }

                // Header will be a Proxy-Authorization string or not set
                let proxy_http = proxy_options.get("http");
                if let Some(proxy_header) = proxy_http.and_then(|h| h.get("header")) {
                    if let Some(PhpMixed::Array(ref mut http)) = options.get_mut("http") {
                        if let Some(PhpMixed::List(ref mut headers)) =
                            http.get_mut("header").map(|v| &mut **v)
                        {
                            headers.push(Box::new(*proxy_header.clone()));
                        }
                    }
                }

                let proxy_options_flat: IndexMap<String, PhpMixed> = proxy_options
                    .iter()
                    .map(|(k, v)| {
                        let inner: IndexMap<String, Box<PhpMixed>> = v
                            .iter()
                            .filter(|(ik, _)| ik.as_str() != "header")
                            .map(|(ik, iv)| (ik.clone(), iv.clone()))
                            .collect();
                        (k.clone(), PhpMixed::Array(inner))
                    })
                    .collect();
                options = array_replace_recursive(options, proxy_options_flat);
            }
        }

        let php_version = if HHVM_VERSION.is_some() {
            format!("HHVM {}", HHVM_VERSION.unwrap())
        } else {
            format!(
                "PHP {}.{}.{}",
                PHP_MAJOR_VERSION, PHP_MINOR_VERSION, PHP_RELEASE_VERSION
            )
        };

        let http_version = if for_curl {
            let curl = curl_version().unwrap_or_default();
            let version = curl
                .get("version")
                .and_then(|v| v.as_string())
                .unwrap_or("")
                .to_string();
            format!("cURL {}", version)
        } else {
            "streams".to_string()
        };

        let has_user_agent = options
            .get("http")
            .and_then(|v| v.as_array())
            .and_then(|a| a.get("header"))
            .and_then(|v| match **v {
                PhpMixed::List(ref list) => {
                    let joined: String = list
                        .iter()
                        .filter_map(|item| item.as_string())
                        .collect::<Vec<_>>()
                        .join("");
                    Some(joined.to_lowercase().contains("user-agent"))
                }
                _ => None,
            })
            .unwrap_or(false);

        if !has_user_agent {
            let platform_php_version = PlatformRepository::get_platform_php_version();
            let user_agent = format!(
                "User-Agent: Composer/{} ({os}; {release}; {php_version}; {http_version}{platform}{ci})",
                Composer::get_version(),
                os = if function_exists("php_uname") {
                    php_uname("s")
                } else {
                    "Unknown".to_string()
                },
                release = if function_exists("php_uname") {
                    php_uname("r")
                } else {
                    "Unknown".to_string()
                },
                php_version = php_version,
                http_version = http_version,
                platform = platform_php_version
                    .as_deref()
                    .map(|v| format!("; Platform-PHP {}", v))
                    .unwrap_or_default(),
                ci = if Platform::get_env("CI").is_some() {
                    "; CI"
                } else {
                    ""
                },
            );
            if let Some(PhpMixed::Array(ref mut http)) = options.get_mut("http") {
                if let Some(PhpMixed::List(ref mut headers)) =
                    http.get_mut("header").map(|v| &mut **v)
                {
                    headers.push(Box::new(PhpMixed::String(user_agent)));
                }
            }
        }

        Ok(options)
    }

    pub fn get_tls_defaults(
        options: &IndexMap<String, PhpMixed>,
        logger: Option<&dyn LoggerInterface>,
    ) -> anyhow::Result<IndexMap<String, PhpMixed>, TransportException> {
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
        let ssl_defaults: IndexMap<String, PhpMixed> = {
            let mut ssl = IndexMap::new();
            ssl.insert("ciphers".to_string(), PhpMixed::String(ciphers));
            ssl.insert("verify_peer".to_string(), PhpMixed::Bool(true));
            ssl.insert("verify_depth".to_string(), PhpMixed::Int(7));
            ssl.insert("SNI_enabled".to_string(), PhpMixed::Bool(true));
            ssl.insert("capture_peer_cert".to_string(), PhpMixed::Bool(true));
            ssl
        };

        let mut defaults: IndexMap<String, PhpMixed> = {
            let mut d = IndexMap::new();
            d.insert(
                "ssl".to_string(),
                PhpMixed::Array(
                    ssl_defaults
                        .into_iter()
                        .map(|(k, v)| (k, Box::new(v)))
                        .collect(),
                ),
            );
            d
        };

        if let Some(ssl_options) = options.get("ssl") {
            if let Some(ssl_defaults_mixed) = defaults.get("ssl").cloned() {
                let merged = array_replace_recursive(
                    match ssl_defaults_mixed {
                        PhpMixed::Array(a) => a.into_iter().map(|(k, v)| (k, *v)).collect(),
                        _ => IndexMap::new(),
                    },
                    match ssl_options.clone() {
                        PhpMixed::Array(a) => a.into_iter().map(|(k, v)| (k, *v)).collect(),
                        _ => IndexMap::new(),
                    },
                );
                defaults.insert(
                    "ssl".to_string(),
                    PhpMixed::Array(merged.into_iter().map(|(k, v)| (k, Box::new(v))).collect()),
                );
            }
        }

        // Attempt to find a local cafile or throw an exception if none pre-set.
        // The user may go download one if this occurs.
        let ssl = defaults.get("ssl").and_then(|v| v.as_array());
        let has_cafile = ssl.as_ref().and_then(|a| a.get("cafile")).is_some();
        let has_capath = ssl.as_ref().and_then(|a| a.get("capath")).is_some();
        if !has_cafile && !has_capath {
            let result = CaBundle::get_system_ca_root_bundle_path(logger);
            if shirabe_php_shim::is_dir(&result) {
                if let Some(PhpMixed::Array(ref mut ssl)) = defaults.get_mut("ssl") {
                    ssl.insert("capath".to_string(), Box::new(PhpMixed::String(result)));
                }
            } else {
                if let Some(PhpMixed::Array(ref mut ssl)) = defaults.get_mut("ssl") {
                    ssl.insert("cafile".to_string(), Box::new(PhpMixed::String(result)));
                }
            }
        }

        let cafile = defaults
            .get("ssl")
            .and_then(|v| v.as_array())
            .and_then(|a| a.get("cafile"))
            .and_then(|v| v.as_string())
            .map(|s| s.to_string());
        if let Some(ref cafile) = cafile {
            if !Filesystem::is_readable(cafile) || !CaBundle::validate_ca_file(cafile, logger) {
                return Err(TransportException::new(
                    "The configured cafile was not valid or could not be read.".to_string(),
                ));
            }
        }

        let capath = defaults
            .get("ssl")
            .and_then(|v| v.as_array())
            .and_then(|a| a.get("capath"))
            .and_then(|v| v.as_string())
            .map(|s| s.to_string());
        if let Some(ref capath) = capath {
            if !shirabe_php_shim::is_dir(capath) || !Filesystem::is_readable(capath) {
                return Err(TransportException::new(
                    "The configured capath was not valid or could not be read.".to_string(),
                ));
            }
        }

        // Disable TLS compression to prevent CRIME attacks where supported.
        if let Some(PhpMixed::Array(ref mut ssl)) = defaults.get_mut("ssl") {
            ssl.insert(
                "disable_compression".to_string(),
                Box::new(PhpMixed::Bool(true)),
            );
        }

        Ok(defaults)
    }

    /// A bug in PHP prevents the headers from correctly being sent when a content-type header is
    /// present and NOT at the end of the array. This method fixes the array by moving the
    /// content-type header to the end.
    fn fix_http_header_field(header: &PhpMixed) -> Vec<String> {
        let mut headers: Vec<String> = match header {
            PhpMixed::String(s) => s.split("\r\n").map(|p| p.to_string()).collect(),
            PhpMixed::List(list) => list
                .iter()
                .filter_map(|v| v.as_string())
                .map(|s| s.to_string())
                .collect(),
            _ => vec![],
        };
        uasort(&mut headers, |el, _| {
            if stripos(el, "content-type") == Some(0) {
                1
            } else {
                -1
            }
        });
        headers
    }
}
