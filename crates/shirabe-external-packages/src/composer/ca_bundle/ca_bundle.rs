//! ref: composer/vendor/composer/ca-bundle/src/CaBundle.php

#[derive(Debug)]
pub struct CaBundle;

impl CaBundle {
    // TODO(plugin): unused for now; kept for API parity.
    // TODO(phase-c): The original inspects the linked OpenSSL version to decide
    // whether openssl_x509_parse can be called safely. Certificate handling is
    // slated to move to reqwest, so this dummy always reports safe.
    pub fn is_openssl_parse_safe() -> bool {
        true
    }

    // The original `$logger` parameter (PSR LoggerInterface) is replaced by a
    // `()` placeholder: CaBundle is expected to be subsumed by a Rust TLS
    // library and removed, so it does not need a real logger.
    //
    // TODO(phase-c): Dummy stand-in until HTTP handling moves to reqwest, which
    // discovers the system CA bundle itself. This probes the SSL_CERT_FILE /
    // SSL_CERT_DIR environment variables and the common distribution CA
    // locations, returning the first that exists. Unlike the original it does
    // not consult OpenSSL's default cert locations, does not validate the
    // candidate before returning it, and has no bundled cacert.pem fallback.
    pub fn get_system_ca_root_bundle_path(_logger: ()) -> String {
        if let Ok(file) = std::env::var("SSL_CERT_FILE") {
            if std::path::Path::new(&file).is_file() {
                return file;
            }
        }

        const CA_FILE_PATHS: &[&str] = &[
            "/etc/pki/tls/certs/ca-bundle.crt",
            "/etc/ssl/certs/ca-certificates.crt",
            "/etc/ssl/ca-bundle.pem",
            "/usr/local/share/certs/ca-root-nss.crt",
            "/usr/ssl/certs/ca-bundle.crt",
            "/opt/local/share/curl/curl-ca-bundle.crt",
            "/usr/local/share/curl/curl-ca-bundle.crt",
            "/usr/share/ssl/certs/ca-bundle.crt",
            "/etc/ssl/cert.pem",
            "/usr/local/etc/ssl/cert.pem",
            "/usr/local/etc/openssl/cert.pem",
            "/usr/local/etc/openssl@1.1/cert.pem",
        ];
        for path in CA_FILE_PATHS {
            if std::path::Path::new(path).is_file() {
                return path.to_string();
            }
        }

        if let Ok(dir) = std::env::var("SSL_CERT_DIR") {
            if std::path::Path::new(&dir).is_dir() {
                return dir;
            }
        }

        const CA_DIR_PATHS: &[&str] = &["/etc/pki/tls/certs", "/etc/ssl/certs"];
        for path in CA_DIR_PATHS {
            if std::path::Path::new(path).is_dir() {
                return path.to_string();
            }
        }

        String::new()
    }

    // TODO(phase-c): Dummy stand-in until reqwest validates certificates itself.
    // The original parses the file with OpenSSL and rejects malformed or expired
    // bundles; here we only require the file to exist and be non-empty.
    pub fn validate_ca_file(ca_file: &str, _logger: ()) -> bool {
        std::fs::read(ca_file)
            .map(|c| !c.is_empty())
            .unwrap_or(false)
    }
}
