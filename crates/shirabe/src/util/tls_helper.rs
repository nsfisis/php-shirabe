//! ref: composer/src/Composer/Util/TlsHelper.php

use shirabe_external_packages::composer::ca_bundle::ca_bundle::CaBundle;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_php_shim::{
    base64_decode, openssl_get_publickey, openssl_pkey_get_details, openssl_x509_parse,
    preg_quote, substr_count, PhpMixed, RuntimeException,
};

/// @deprecated Use composer/ca-bundle and composer/composer 2.2 if you still need PHP 5 compatibility
pub struct TlsHelper;

impl TlsHelper {
    pub fn check_certificate_host(
        certificate: &PhpMixed,
        hostname: &str,
        cn: &mut Option<String>,
    ) -> bool {
        let names = match Self::get_certificate_names(certificate) {
            Some(n) => n,
            None => return false,
        };

        let mut combined_names = names.san.clone();
        combined_names.push(names.cn.clone());
        let hostname = hostname.to_lowercase();

        for cert_name in &combined_names {
            if let Some(matcher) = Self::cert_name_matcher(cert_name) {
                if matcher(&hostname) {
                    *cn = Some(names.cn.clone());
                    return true;
                }
            }
        }

        false
    }

    pub fn get_certificate_names(certificate: &PhpMixed) -> Option<CertificateNames> {
        let info = match certificate {
            PhpMixed::Array(arr) => arr.clone(),
            _ => {
                if CaBundle::is_openssl_parse_safe() {
                    if let PhpMixed::String(cert_str) = certificate {
                        openssl_x509_parse(cert_str, false)?
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
        };

        let common_name = info.get("subject")
            .and_then(|v| v.as_array())
            .and_then(|subj| subj.get("commonName"))
            .and_then(|cn| cn.as_string())
            .map(|s| s.to_lowercase())?;

        let mut subject_alt_names = vec![];
        if let Some(san_value) = info.get("extensions")
            .and_then(|v| v.as_array())
            .and_then(|ext| ext.get("subjectAltName"))
            .and_then(|v| v.as_string())
        {
            let parts = Preg::split(r"{\s*,\s*}", san_value).unwrap_or_default();
            for name in parts {
                if name.starts_with("DNS:") {
                    let dns = name[4..].trim_start().to_lowercase();
                    subject_alt_names.push(dns);
                }
            }
        }

        Some(CertificateNames {
            cn: common_name,
            san: subject_alt_names,
        })
    }

    /// Get the certificate pin.
    ///
    /// By Kevin McArthur of StormTide Digital Studios Inc.
    /// @KevinSMcArthur / https://github.com/StormTide
    ///
    /// See https://tools.ietf.org/html/draft-ietf-websec-key-pinning-02
    ///
    /// This method was adapted from Sslurp.
    /// https://github.com/EvanDotPro/Sslurp
    ///
    /// (c) Evan Coury <me@evancoury.com>
    ///
    /// For the full copyright and license information, please see below:
    ///
    /// Copyright (c) 2013, Evan Coury
    /// All rights reserved.
    ///
    /// Redistribution and use in source and binary forms, with or without modification,
    /// are permitted provided that the following conditions are met:
    ///
    ///     * Redistributions of source code must retain the above copyright notice,
    ///       this list of conditions and the following disclaimer.
    ///
    ///     * Redistributions in binary form must reproduce the above copyright notice,
    ///       this list of conditions and the following disclaimer in the documentation
    ///       and/or other materials provided with the distribution.
    ///
    /// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
    /// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
    /// WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
    /// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR
    /// ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
    /// (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
    /// LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON
    /// ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
    /// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
    /// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
    pub fn get_certificate_fingerprint(certificate: &str) -> anyhow::Result<String> {
        let pubkey = openssl_get_publickey(certificate).ok_or_else(|| RuntimeException {
            message: "Failed to retrieve the public key from certificate".to_string(),
            code: 0,
        })?;
        let pubkeydetails = openssl_pkey_get_details(pubkey).ok_or_else(|| RuntimeException {
            message: "Failed to retrieve public key details".to_string(),
            code: 0,
        })?;
        let pubkeypem = pubkeydetails.get("key")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_string();

        let start = "-----BEGIN PUBLIC KEY-----";
        let end = "-----END PUBLIC KEY-----";
        let start_pos = pubkeypem.find(start).unwrap_or(0) + start.len();
        let end_pos = pubkeypem.rfind(end).unwrap_or(pubkeypem.len());
        let pemtrim = &pubkeypem[start_pos..end_pos];

        let der = base64_decode(pemtrim).unwrap_or_default();

        Ok(shirabe_php_shim::hash("sha1", &String::from_utf8_lossy(&der)))
    }

    pub fn is_openssl_parse_safe() -> bool {
        CaBundle::is_openssl_parse_safe()
    }

    fn cert_name_matcher(cert_name: &str) -> Option<Box<dyn Fn(&str) -> bool>> {
        let wildcards = substr_count(cert_name, "*");

        if wildcards == 0 {
            let name = cert_name.to_string();
            return Some(Box::new(move |hostname: &str| hostname == name));
        }

        if wildcards == 1 {
            let components: Vec<&str> = cert_name.split('.').collect();

            if components.len() < 3 {
                return None;
            }

            let first_component = components[0];

            if !first_component.ends_with('*') {
                return None;
            }

            let mut wildcard_regex = preg_quote(cert_name, None);
            wildcard_regex = wildcard_regex.replace("\\*", "[a-z0-9-]+");
            let wildcard_regex = format!("{{^{}$}}", wildcard_regex);

            return Some(Box::new(move |hostname: &str| {
                Preg::is_match(&wildcard_regex, hostname).unwrap_or(false)
            }));
        }

        None
    }
}

#[derive(Debug)]
pub struct CertificateNames {
    pub cn: String,
    pub san: Vec<String>,
}
