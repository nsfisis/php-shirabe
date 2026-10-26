//! ref: composer/vendor/composer/ca-bundle/src/CaBundle.php

#[derive(Debug)]
pub struct CaBundle;

impl CaBundle {
    // TODO(plugin): unused for now; kept for API parity.
    // TODO(http): The original inspects the linked OpenSSL version to decide
    // whether openssl_x509_parse can be called safely. Certificate handling is
    // slated to move to reqwest, so this dummy always reports safe.
    pub fn is_openssl_parse_safe() -> bool {
        true
    }

    // The original `$logger` parameter (PSR LoggerInterface) is replaced by a
    // `()` placeholder: CaBundle is expected to be subsumed by a Rust TLS
    // library and removed, so it does not need a real logger.
    //
    // TODO(http): Dummy stand-in until HTTP handling moves to reqwest, which
    // discovers the system CA bundle itself. This probes the SSL_CERT_FILE /
    // SSL_CERT_DIR environment variables and the common distribution CA
    // locations, returning the first that exists. Unlike the original it does
    // not consult OpenSSL's default cert locations, does not validate the
    // candidate before returning it, and has no bundled cacert.pem fallback.
    pub fn get_system_ca_root_bundle_path(_logger: ()) -> std::path::PathBuf {
        if let Some(file) = Self::get_env_variable("SSL_CERT_FILE")
            && std::path::Path::new(&file).is_file()
        {
            return file.into();
        }

        let ca_file_paths = &[
            std::path::Path::new("/etc/pki/tls/certs/ca-bundle.crt"),
            std::path::Path::new("/etc/ssl/certs/ca-certificates.crt"),
            std::path::Path::new("/etc/ssl/ca-bundle.pem"),
            std::path::Path::new("/usr/local/share/certs/ca-root-nss.crt"),
            std::path::Path::new("/usr/ssl/certs/ca-bundle.crt"),
            std::path::Path::new("/opt/local/share/curl/curl-ca-bundle.crt"),
            std::path::Path::new("/usr/local/share/curl/curl-ca-bundle.crt"),
            std::path::Path::new("/usr/share/ssl/certs/ca-bundle.crt"),
            std::path::Path::new("/etc/ssl/cert.pem"),
            std::path::Path::new("/usr/local/etc/ssl/cert.pem"),
            std::path::Path::new("/usr/local/etc/openssl/cert.pem"),
            std::path::Path::new("/usr/local/etc/openssl@1.1/cert.pem"),
        ];
        for path in ca_file_paths {
            if path.is_file() {
                return path.into();
            }
        }

        if let Some(dir) = Self::get_env_variable("SSL_CERT_DIR")
            && std::path::Path::new(&dir).is_dir()
        {
            return dir.into();
        }

        let ca_dir_paths = &[
            std::path::Path::new("/etc/pki/tls/certs"),
            std::path::Path::new("/etc/ssl/certs"),
        ];
        for path in ca_dir_paths {
            if path.is_dir() {
                return path.into();
            }
        }

        std::path::PathBuf::new()
    }

    fn get_env_variable(name: &str) -> Option<std::ffi::OsString> {
        let value = shirabe_php_shim::PHP_SERVER.lock().unwrap().get(name);
        value.or_else(|| shirabe_php_shim::getenv(name))
    }

    // TODO(http): Dummy stand-in until reqwest validates certificates itself.
    // The original parses the file with OpenSSL and rejects malformed or expired
    // bundles; here we only require the file to exist and be non-empty.
    pub fn validate_ca_file(ca_file: &str, _logger: ()) -> bool {
        std::fs::read(ca_file)
            .map(|c| !c.is_empty())
            .unwrap_or(false)
    }
}
