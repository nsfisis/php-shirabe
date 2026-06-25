//! ref: composer/tests/Composer/Test/Util/TlsHelperTest.php

use indexmap::IndexMap;
use shirabe::util::tls_helper::TlsHelper;
use shirabe_php_shim::PhpMixed;

// Builds the `['subject' => ['commonName' => ..], 'extensions' => ['subjectAltName' => ..]]`
// certificate array used by the test, given the common name and the subjectAltName string.
fn certificate(common_name: &str, subject_alt_name: &str) -> PhpMixed {
    let mut subject = IndexMap::new();
    subject.insert(
        "commonName".to_string(),
        PhpMixed::String(common_name.to_string()),
    );
    let mut extensions = IndexMap::new();
    extensions.insert(
        "subjectAltName".to_string(),
        PhpMixed::String(subject_alt_name.to_string()),
    );
    let mut cert = IndexMap::new();
    cert.insert("subject".to_string(), PhpMixed::Array(subject));
    cert.insert("extensions".to_string(), PhpMixed::Array(extensions));
    PhpMixed::Array(cert)
}

/// ref: TlsHelperTest::dataCheckCertificateHost
fn data_check_certificate_host() -> Vec<(bool, &'static str, Vec<&'static str>)> {
    vec![
        (true, "getcomposer.org", vec!["getcomposer.org"]),
        (
            true,
            "getcomposer.org",
            vec!["getcomposer.org", "packagist.org"],
        ),
        (
            true,
            "getcomposer.org",
            vec!["packagist.org", "getcomposer.org"],
        ),
        (true, "foo.getcomposer.org", vec!["*.getcomposer.org"]),
        (false, "xyz.foo.getcomposer.org", vec!["*.getcomposer.org"]),
        (
            true,
            "foo.getcomposer.org",
            vec!["getcomposer.org", "*.getcomposer.org"],
        ),
        (
            true,
            "foo.getcomposer.org",
            vec!["foo.getcomposer.org", "foo*.getcomposer.org"],
        ),
        (
            true,
            "foo1.getcomposer.org",
            vec!["foo.getcomposer.org", "foo*.getcomposer.org"],
        ),
        (
            true,
            "foo2.getcomposer.org",
            vec!["foo.getcomposer.org", "foo*.getcomposer.org"],
        ),
        (
            false,
            "foo2.another.getcomposer.org",
            vec!["foo.getcomposer.org", "foo*.getcomposer.org"],
        ),
        (
            false,
            "test.example.net",
            vec!["**.example.net", "**.example.net"],
        ),
        (
            false,
            "test.example.net",
            vec!["t*t.example.net", "t*t.example.net"],
        ),
        (
            false,
            "xyz.example.org",
            vec!["*z.example.org", "*z.example.org"],
        ),
        (
            false,
            "foo.bar.example.com",
            vec!["foo.*.example.com", "foo.*.example.com"],
        ),
        (false, "example.com", vec!["example.*", "example.*"]),
        (true, "localhost", vec!["localhost"]),
        (false, "localhost", vec!["*"]),
        (false, "localhost", vec!["local*"]),
        (false, "example.net", vec!["*.net", "*.org", "ex*.net"]),
        (true, "example.net", vec!["*.net", "*.org", "example.net"]),
    ]
}

#[test]
fn test_check_certificate_host() {
    for (expected_result, hostname, mut cert_names) in data_check_certificate_host() {
        let expected_cn = cert_names.remove(0);
        let subject_alt_name = if cert_names.is_empty() {
            String::new()
        } else {
            format!("DNS:{}", cert_names.join(",DNS:"))
        };
        let cert = certificate(expected_cn, &subject_alt_name);

        let mut found_cn: Option<String> = None;
        let result = TlsHelper::check_certificate_host(&cert, hostname, &mut found_cn);

        if expected_result {
            assert!(result, "hostname {hostname} should match");
            assert_eq!(found_cn.as_deref(), Some(expected_cn));
        } else {
            assert!(!result, "hostname {hostname} should not match");
            assert_eq!(found_cn, None);
        }
    }
}

#[test]
fn test_get_certificate_names() {
    let cert = certificate(
        "example.net",
        "DNS: example.com, IP: 127.0.0.1, DNS: getcomposer.org, Junk: blah, DNS: composer.example.org",
    );

    let names = TlsHelper::get_certificate_names(&cert).unwrap();

    assert_eq!(names.cn, "example.net");
    assert_eq!(
        names.san,
        vec![
            "example.com".to_string(),
            "getcomposer.org".to_string(),
            "composer.example.org".to_string(),
        ]
    );
}
