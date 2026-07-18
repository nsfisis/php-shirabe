//! ref: composer/src/Composer/Util/TlsHelper.php

use shirabe_external_packages::composer::ca_bundle::ca_bundle::CaBundle;
use shirabe_external_packages::composer::pcre::Preg;
use shirabe_php_shim::{
    PhpMixed, ltrim, php_regex, preg_quote, str_replace, strtolower, substr, substr_count,
};

/// Extracted certificate names. Mirrors PHP's `array{cn: string, san: string[]}`.
#[derive(Debug, Clone)]
pub struct CertificateNames {
    pub cn: String,
    pub san: Vec<String>,
}

/// Match hostname against a certificate.
///
/// @deprecated Use composer/ca-bundle and composer/composer 2.2 if you still need PHP 5
/// compatibility, this class will be removed in Composer 3.0
#[derive(Debug)]
pub struct TlsHelper;

impl TlsHelper {
    /// Match hostname against a certificate. Sets `cn` to the common name of the
    /// certificate iff a match is found.
    pub fn check_certificate_host(
        certificate: &PhpMixed,
        hostname: &str,
        cn: &mut Option<String>,
    ) -> bool {
        let names = Self::get_certificate_names(certificate);

        let Some(names) = names else {
            return false;
        };

        let mut combined_names = names.san.clone();
        combined_names.push(names.cn.clone());
        let hostname = strtolower(hostname);

        for cert_name in &combined_names {
            let matcher = Self::cert_name_matcher(cert_name);

            if let Some(matcher) = matcher
                && matcher(&hostname)
            {
                *cn = Some(names.cn.clone());

                return true;
            }
        }

        false
    }

    /// Extract DNS names out of an X.509 certificate.
    pub fn get_certificate_names(certificate: &PhpMixed) -> Option<CertificateNames> {
        let info: Option<&PhpMixed> = if certificate.as_array().is_some() {
            Some(certificate)
        } else if CaBundle::is_openssl_parse_safe() {
            // TODO(phase-c): openssl_x509_parse on a PEM string certificate.
            todo!("openssl_x509_parse for non-array certificates")
        } else {
            None
        };

        let info = info?.as_array()?;

        let common_name = info
            .get("subject")
            .and_then(|s| s.as_array())
            .and_then(|s| s.get("commonName"))
            .and_then(|c| c.as_string());

        let common_name = strtolower(common_name?);
        let mut subject_alt_names: Vec<String> = Vec::new();

        if let Some(san) = info
            .get("extensions")
            .and_then(|e| e.as_array())
            .and_then(|e| e.get("subjectAltName"))
            .and_then(|s| s.as_string())
        {
            let split = Preg::split(php_regex!("{\\s*,\\s*}"), san);
            subject_alt_names = split
                .into_iter()
                .filter_map(|name| {
                    if name.starts_with("DNS:") {
                        Some(strtolower(&ltrim(&substr(&name, 4, None), None)))
                    } else {
                        None
                    }
                })
                .collect();
        }

        Some(CertificateNames {
            cn: common_name,
            san: subject_alt_names,
        })
    }

    /// Get the certificate pin.
    pub fn get_certificate_fingerprint(_certificate: &str) -> String {
        todo!("openssl public key extraction and sha1 fingerprint")
    }

    /// Test if it is safe to use the PHP function openssl_x509_parse().
    pub fn is_openssl_parse_safe() -> bool {
        CaBundle::is_openssl_parse_safe()
    }

    /// Convert certificate name into matching function.
    fn cert_name_matcher(cert_name: &str) -> Option<Box<dyn Fn(&str) -> bool>> {
        let wildcards = substr_count(cert_name, "*");

        if wildcards == 0 {
            // Literal match.
            let cert_name = cert_name.to_string();
            return Some(Box::new(move |hostname: &str| hostname == cert_name));
        }

        if wildcards == 1 {
            let components: Vec<&str> = cert_name.split('.').collect();

            if components.len() < 3 {
                // Must have 3+ components
                return None;
            }

            let first_component = components[0];

            // Wildcard must be the last character.
            if !first_component.ends_with('*') {
                return None;
            }

            let mut wildcard_regex = preg_quote(cert_name, None);
            wildcard_regex = str_replace("\\*", "[a-z0-9-]+", &wildcard_regex);
            let wildcard_regex = format!("{{^{}$}}", wildcard_regex);

            return Some(Box::new(move |hostname: &str| {
                Preg::is_match(&wildcard_regex, hostname)
            }));
        }

        None
    }
}
