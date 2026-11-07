//! ref: composer/tests/Composer/Test/Repository/PlatformRepositoryTest.php sha1:4638d19f83e64fc350f85b702e6768b44b7b2d05

use bstr::{BStr, BString};
use indexmap::IndexMap;
use shirabe::package::{BasePackageHandle, Link};
use shirabe::platform::HhvmDetectorInterface;
use shirabe::repository::{
    FindPackageConstraint, PlatformRepository, RepositoryInterface, SEARCH_NAME,
};
use shirabe_php_rpc::PlatformInfo;
use shirabe_php_shim::{PhpMixed, ToPhpString, bformat};
use shirabe_semver::constraint::SimpleConstraint;

#[expect(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    reason = "the code mockall generates"
)]
mod mock_hhvm_detector {
    use super::*;

    mockall::mock! {
        pub HhvmDetector {}
        impl HhvmDetectorInterface for HhvmDetector {
            fn reset(&self);
            fn get_version(&mut self) -> Option<bstr::BString>;
        }
    }
}
use mock_hhvm_detector::MockHhvmDetector;

// The seam trait requires `Debug` (so `PlatformRepository` can derive it); mockall does
// not generate it for mocks.
impl std::fmt::Debug for MockHhvmDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MockHhvmDetector")
    }
}

/// The payload form of what a dataset's `Runtime` mock answers: the listed constants and classes
/// exist, and each extension is loaded at `extension_version` with `info` as its info() output.
fn platform_info(
    constants: Vec<(BString, Option<BString>, PhpMixed)>,
    extensions: Vec<BString>,
    extension_version: &BStr,
    info: Option<&BStr>,
    functions: &[(PhpMixed, Vec<PhpMixed>, PhpMixed)],
    class_definitions: &[ClassDef],
) -> PlatformInfo {
    let mut platform_info = PlatformInfo::default();

    for (constant_name, class, value) in constants {
        platform_info.__set_constant(&constant_name, class.as_deref().map(bstr::BStr::new), value);
    }

    for extension in &extensions {
        platform_info.__set_extension_info(extension, info.unwrap_or_default());
    }
    platform_info.__set_extensions(extensions, extension_version);

    for (callable, arguments, result) in functions {
        match (callable, arguments.as_slice()) {
            (PhpMixed::String(name), []) if name == "curl_version" => {
                platform_info.curl_version = Some(entry_string(result, BStr::new("version")));
            }
            (PhpMixed::String(name), [PhpMixed::String(address)])
                if name == "inet_pton" && address == "::" =>
            {
                platform_info.inet_pton_ipv6 = match result {
                    PhpMixed::String(address) => Some(address.to_vec()),
                    PhpMixed::Bool(false) => None,
                    other => panic!("inet_pton() does not return {other:?}"),
                };
            }
            (PhpMixed::List(spec), _) => match spec.as_slice() {
                [PhpMixed::String(class), PhpMixed::String(method)]
                    if class == "ResourceBundle" && method == "create" =>
                {
                    platform_info.resource_bundle_version =
                        Some(entry_string(result, BStr::new("Version")));
                }
                [PhpMixed::String(class), PhpMixed::String(method)]
                    if class == "IntlChar" && method == "getUnicodeVersion" =>
                {
                    platform_info.intl_char_unicode_version = match result {
                        PhpMixed::List(parts) => Some(
                            parts
                                .iter()
                                .map(|part| match part {
                                    PhpMixed::Int(n) => *n,
                                    other => panic!(
                                        "IntlChar::getUnicodeVersion() does not return {other:?}"
                                    ),
                                })
                                .collect(),
                        ),
                        other => panic!("IntlChar::getUnicodeVersion() does not return {other:?}"),
                    };
                }
                other => panic!("the platform payload does not report {other:?}"),
            },
            other => panic!("the platform payload does not report {other:?}"),
        }
    }

    for definition in class_definitions {
        match (definition.class, &definition.construct) {
            (class, Some((_arguments, result))) if class == "Imagick" => {
                platform_info.imagick_version_string =
                    Some(entry_string(result, BStr::new("versionString")))
            }
            (class, None) => platform_info.__set_class(class),
            (class, Some(_)) => panic!(
                "the platform payload does not construct {class}",
                class = class.display_lossy()
            ),
        }
    }

    platform_info
}

/// The string entry `key` of an array, or of an object modelled as its fields, that the platform
/// payload reduces a PHP call result to.
fn entry_string(value: &PhpMixed, key: &BStr) -> BString {
    match value {
        PhpMixed::Array(fields) | PhpMixed::Object(fields) => match fields.get(key) {
            Some(PhpMixed::String(entry)) => entry.clone(),
            other => panic!("{key:?} is not a string entry: {other:?}"),
        },
        other => panic!("{other:?} has no {key:?} entry"),
    }
}

#[test]
fn test_hhvm_package() {
    let mut hhvm_detector = MockHhvmDetector::new();
    hhvm_detector
        .expect_get_version()
        .times(..)
        .returning(|| Some("2.1.0".into()));

    let mut platform_repository =
        PlatformRepository::new(vec![], IndexMap::new(), None, Some(Box::new(hhvm_detector)))
            .unwrap();

    let hhvm = platform_repository
        .find_package(
            bstr::BStr::new("hhvm"),
            FindPackageConstraint::String(BString::from("*")),
        )
        .unwrap();
    assert!(hhvm.is_some(), "hhvm found");

    assert_eq!("2.1.0", hhvm.unwrap().get_pretty_version());
}

fn php_flavor_test_cases() -> Vec<(
    IndexMap<BString, PhpMixed>,
    Vec<(&'static BStr, &'static BStr)>,
    Vec<(PhpMixed, Vec<PhpMixed>, PhpMixed)>,
)> {
    let ubuntu = BStr::new("7.2.31-1+ubuntu16.04.1+deb.sury.org+1");
    let s = |v: &BStr| PhpMixed::string(v.to_php_string());

    vec![
        (
            IndexMap::from([("PHP_VERSION".into(), s(BStr::new("7.1.33")))]),
            vec![(BStr::new("php"), BStr::new("7.1.33"))],
            vec![],
        ),
        (
            IndexMap::from([
                ("PHP_VERSION".into(), s(ubuntu)),
                ("PHP_DEBUG".into(), PhpMixed::Bool(true)),
            ]),
            vec![
                (BStr::new("php"), BStr::new("7.2.31")),
                (BStr::new("php-debug"), BStr::new("7.2.31")),
            ],
            vec![],
        ),
        (
            IndexMap::from([
                ("PHP_VERSION".into(), s(ubuntu)),
                ("PHP_ZTS".into(), PhpMixed::Bool(true)),
            ]),
            vec![
                (BStr::new("php"), BStr::new("7.2.31")),
                (BStr::new("php-zts"), BStr::new("7.2.31")),
            ],
            vec![],
        ),
        (
            IndexMap::from([
                ("PHP_VERSION".into(), s(ubuntu)),
                ("PHP_INT_SIZE".into(), PhpMixed::Int(8)),
            ]),
            vec![
                (BStr::new("php"), BStr::new("7.2.31")),
                (BStr::new("php-64bit"), BStr::new("7.2.31")),
            ],
            vec![],
        ),
        (
            IndexMap::from([
                ("PHP_VERSION".into(), s(ubuntu)),
                ("AF_INET6".into(), PhpMixed::Int(30)),
            ]),
            vec![
                (BStr::new("php"), BStr::new("7.2.31")),
                (BStr::new("php-ipv6"), BStr::new("7.2.31")),
            ],
            vec![],
        ),
        (
            IndexMap::from([("PHP_VERSION".into(), s(ubuntu))]),
            vec![
                (BStr::new("php"), BStr::new("7.2.31")),
                (BStr::new("php-ipv6"), BStr::new("7.2.31")),
            ],
            vec![(
                s(BStr::new("inet_pton")),
                vec![s(BStr::new("::"))],
                s(BStr::new("")),
            )],
        ),
        (
            IndexMap::from([("PHP_VERSION".into(), s(ubuntu))]),
            vec![(BStr::new("php"), BStr::new("7.2.31"))],
            vec![(
                s(BStr::new("inet_pton")),
                vec![s(BStr::new("::"))],
                PhpMixed::Bool(false),
            )],
        ),
    ]
}

#[test]
fn test_php_version() {
    for (constants, packages, functions) in php_flavor_test_cases() {
        let platform_info = platform_info(
            constants
                .into_iter()
                .map(|(constant_name, value)| (constant_name, None, value))
                .collect(),
            Vec::new(),
            BStr::new(""),
            None,
            &functions,
            &[],
        );

        let mut repository =
            PlatformRepository::new(vec![], IndexMap::new(), Some(platform_info), None).unwrap();

        for (package_name, version) in packages {
            let package = repository
                .find_package(
                    bstr::BStr::new(package_name),
                    FindPackageConstraint::String(BString::from("*")),
                )
                .unwrap();
            assert!(
                package.is_some(),
                "Expected to find package \"{}\"",
                package_name.display_lossy()
            );
            assert_eq!(
                version,
                package.unwrap().get_pretty_version(),
                "Expected package \"{}\" version to be {}",
                package_name.display_lossy(),
                version.display_lossy()
            );
        }
    }
}

#[test]
fn test_inet_pton_regression() {
    // PHP: ->expects(self::once())->method('invoke')->with('inet_pton', ['::'])->willReturn(false).
    // TODO(mock): the payload reports the result of `@inet_pton('::')` instead of answering a
    // call, so there is nothing left for the once() call-count check to observe.
    let functions = [(
        PhpMixed::string(BString::from("inet_pton")),
        vec![PhpMixed::string(BString::from("::"))],
        PhpMixed::Bool(false),
    )];

    // suppressing PHP_ZTS & AF_INET6 by leaving them undefined
    let constants: IndexMap<BString, PhpMixed> = IndexMap::from([
        (
            "PHP_VERSION".into(),
            PhpMixed::string(BString::from("7.0.0")),
        ),
        ("PHP_DEBUG".into(), PhpMixed::Bool(false)),
    ]);

    let platform_info = platform_info(
        constants
            .into_iter()
            .map(|(constant_name, value)| (constant_name, None, value))
            .collect(),
        Vec::new(),
        BStr::new(""),
        None,
        &functions,
        &[],
    );

    let mut repository =
        PlatformRepository::new(vec![], IndexMap::new(), Some(platform_info), None).unwrap();
    let package = repository
        .find_package(
            bstr::BStr::new("php-ipv6"),
            FindPackageConstraint::String(BString::from("*")),
        )
        .unwrap();
    assert!(package.is_none());
}

enum Exp {
    Version(&'static BStr),
    Missing,
    Full(&'static BStr, Vec<&'static BStr>, Vec<&'static BStr>),
}

impl Exp {
    fn parts(&self) -> (Option<&'static BStr>, &[&'static BStr], &[&'static BStr]) {
        match self {
            Exp::Version(v) => (Some(*v), &[], &[]),
            Exp::Missing => (None, &[], &[]),
            Exp::Full(v, replaces, provides) => (Some(*v), replaces, provides),
        }
    }
}

#[derive(Clone)]
struct ClassDef {
    class: &'static BStr,
    construct: Option<(Vec<PhpMixed>, PhpMixed)>,
}

struct LibCase {
    extensions: Vec<&'static BStr>,
    info: Option<&'static BStr>,
    expectations: Vec<(&'static BStr, Exp)>,
    functions: Vec<(PhpMixed, Vec<PhpMixed>, PhpMixed)>,
    constants: Vec<(&'static BStr, Option<&'static BStr>, PhpMixed)>,
    class_definitions: Vec<ClassDef>,
}

// Port of PlatformRepositoryTest::provideLibraryTestCases (all 59 datasets). The intl and
// imagick cases reference the PHP ResourceBundleStub/ImagickStub objects (modelled as
// PhpMixed::Object holding the version they would expose); their runtime read paths are
// still `TODO(plugin)` stubs, but the data is ported faithfully.
fn library_test_cases() -> Vec<LibCase> {
    let s = |v: &BStr| PhpMixed::string(v.to_php_string());
    let curl = |v: &BStr| -> (PhpMixed, Vec<PhpMixed>, PhpMixed) {
        (
            s(BStr::new("curl_version")),
            vec![],
            PhpMixed::Array(IndexMap::from([("version".into(), s(v))])),
        )
    };

    vec![
        LibCase {
            extensions: vec![BStr::new("amqp")],
            info: Some(
                BStr::new("

amqp

Version => 1.9.4
Revision => release
Compiled => Nov 19 2019 @ 08:44:26
AMQP protocol version => 0-9-1
librabbitmq version => 0.9.0
Default max channels per connection => 256
Default max frame size => 131072
Default heartbeats interval => 0"),
            ),
            expectations: vec![
                (BStr::new("lib-amqp-protocol"), Exp::Version(BStr::new("0.9.1"))),
                (BStr::new("lib-amqp-librabbitmq"), Exp::Version(BStr::new("0.9.0"))),
            ],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("bz2")],
            info: Some(
                BStr::new("
bz2

BZip2 Support => Enabled
Stream Wrapper support => compress.bzip2://
Stream Filter support => bzip2.decompress, bzip2.compress
BZip2 Version => 1.0.5, 6-Sept-2010"),
            ),
            expectations: vec![(BStr::new("lib-bz2"), Exp::Version(BStr::new("1.0.5")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("curl")],
            info: Some(
                BStr::new("
curl

cURL support => enabled
cURL Information => 7.38.0
Age => 3
Features
AsynchDNS => Yes
CharConv => No
Debug => No
GSS-Negotiate => No
IDN => Yes
IPv6 => Yes
krb4 => No
Largefile => Yes
libz => Yes
NTLM => Yes
NTLMWB => Yes
SPNEGO => Yes
SSL => Yes
SSPI => No
TLS-SRP => Yes
HTTP2 => No
GSSAPI => Yes
Protocols => dict, file, ftp, ftps, gopher, http, https, imap, imaps, ldap, ldaps, pop3, pop3s, rtmp, rtsp, scp, sftp, smtp, smtps, telnet, tftp
Host => x86_64-pc-linux-gnu
SSL Version => OpenSSL/1.0.1t
ZLib Version => 1.2.8
libSSH Version => libssh2/1.4.3

Directive => Local Value => Master Value
curl.cainfo => no value => no value"),
            ),
            expectations: vec![
                (BStr::new("lib-curl"), Exp::Version(BStr::new("2.0.0"))),
                (BStr::new("lib-curl-openssl"), Exp::Version(BStr::new("1.0.1.20"))),
                (BStr::new("lib-curl-zlib"), Exp::Version(BStr::new("1.2.8"))),
                (BStr::new("lib-curl-libssh2"), Exp::Version(BStr::new("1.4.3"))),
            ],
            functions: vec![curl(BStr::new("2.0.0"))],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("curl")],
            info: Some(
                BStr::new("
curl

cURL support => enabled
cURL Information => 7.38.0
Age => 3
Features
AsynchDNS => Yes
CharConv => No
Debug => No
GSS-Negotiate => No
IDN => Yes
IPv6 => Yes
krb4 => No
Largefile => Yes
libz => Yes
NTLM => Yes
NTLMWB => Yes
SPNEGO => Yes
SSL => Yes
SSPI => No
TLS-SRP => Yes
HTTP2 => No
GSSAPI => Yes
Protocols => dict, file, ftp, ftps, gopher, http, https, imap, imaps, ldap, ldaps, pop3, pop3s, rtmp, rtsp, scp, sftp, smtp, smtps, telnet, tftp
Host => x86_64-pc-linux-gnu
SSL Version => OpenSSL/1.0.1t-fips
ZLib Version => 1.2.8
libSSH Version => libssh2/1.4.3

Directive => Local Value => Master Value
curl.cainfo => no value => no value"),
            ),
            expectations: vec![
                (BStr::new("lib-curl"), Exp::Version(BStr::new("2.0.0"))),
                (
                    BStr::new("lib-curl-openssl-fips"),
                    Exp::Full(BStr::new("1.0.1.20"), vec![], vec![BStr::new("lib-curl-openssl")]),
                ),
                (BStr::new("lib-curl-zlib"), Exp::Version(BStr::new("1.2.8"))),
                (BStr::new("lib-curl-libssh2"), Exp::Version(BStr::new("1.4.3"))),
            ],
            functions: vec![curl(BStr::new("2.0.0"))],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("curl")],
            info: Some(
                BStr::new("
curl

cURL support => enabled
cURL Information => 7.22.0
Age => 3
Features
AsynchDNS => No
CharConv => No
Debug => No
GSS-Negotiate => Yes
IDN => Yes
IPv6 => Yes
krb4 => No
Largefile => Yes
libz => Yes
NTLM => Yes
NTLMWB => Yes
SPNEGO => No
SSL => Yes
SSPI => No
TLS-SRP => Yes
Protocols => dict, file, ftp, ftps, gopher, http, https, imap, imaps, ldap, pop3, pop3s, rtmp, rtsp, smtp, smtps, telnet, tftp
Host => x86_64-pc-linux-gnu
SSL Version => GnuTLS/2.12.14
ZLib Version => 1.2.3.4"),
            ),
            expectations: vec![
                (BStr::new("lib-curl"), Exp::Version(BStr::new("7.22.0"))),
                (BStr::new("lib-curl-zlib"), Exp::Version(BStr::new("1.2.3.4"))),
                (
                    BStr::new("lib-curl-gnutls"),
                    Exp::Full(BStr::new("2.12.14"), vec![BStr::new("lib-curl-openssl")], vec![]),
                ),
            ],
            functions: vec![curl(BStr::new("7.22.0"))],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("curl")],
            info: Some(
                BStr::new("
curl

cURL support => enabled
cURL Information => 7.24.0
Age => 3
Features
AsynchDNS => Yes
Debug => No
GSS-Negotiate => Yes
IDN => Yes
IPv6 => Yes
Largefile => Yes
NTLM => Yes
SPNEGO => No
SSL => Yes
SSPI => No
krb4 => No
libz => Yes
CharConv => No
Protocols => dict, file, ftp, ftps, gopher, http, https, imap, imaps, ldap, ldaps, pop3, pop3s, rtsp, scp, sftp, smtp, smtps, telnet, tftp
Host => x86_64-redhat-linux-gnu
SSL Version => NSS/3.13.3.0
ZLib Version => 1.2.5
libSSH Version => libssh2/1.4.1"),
            ),
            expectations: vec![
                (BStr::new("lib-curl"), Exp::Version(BStr::new("7.24.0"))),
                (
                    BStr::new("lib-curl-nss"),
                    Exp::Full(BStr::new("3.13.3.0"), vec![BStr::new("lib-curl-openssl")], vec![]),
                ),
                (BStr::new("lib-curl-zlib"), Exp::Version(BStr::new("1.2.5"))),
                (BStr::new("lib-curl-libssh2"), Exp::Version(BStr::new("1.4.1"))),
            ],
            functions: vec![curl(BStr::new("7.24.0"))],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("curl")],
            info: Some(
                BStr::new("
curl

cURL support => enabled
cURL Information => 7.68.0
Age => 5
Features
AsynchDNS => Yes
CharConv => No
Debug => No
GSS-Negotiate => No
IDN => Yes
IPv6 => Yes
krb4 => No
Largefile => Yes
libz => Yes
NTLM => Yes
NTLMWB => Yes
SPNEGO => Yes
SSL => Yes
SSPI => No
TLS-SRP => Yes
HTTP2 => Yes
GSSAPI => Yes
KERBEROS5 => Yes
UNIX_SOCKETS => Yes
PSL => Yes
HTTPS_PROXY => Yes
MULTI_SSL => No
BROTLI => Yes
Protocols => dict, file, ftp, ftps, gopher, http, https, imap, imaps, ldap, ldaps, pop3, pop3s, rtmp, rtsp, scp, sftp, smb, smbs, smtp, smtps, telnet, tftp
Host => x86_64-pc-linux-gnu
SSL Version => OpenSSL/1.1.1g
ZLib Version => 1.2.11
libSSH Version => libssh/0.9.3/openssl/zlib"),
            ),
            expectations: vec![
                (BStr::new("lib-curl"), Exp::Version(BStr::new("7.68.0"))),
                (BStr::new("lib-curl-openssl"), Exp::Version(BStr::new("1.1.1.7"))),
                (BStr::new("lib-curl-zlib"), Exp::Version(BStr::new("1.2.11"))),
                (BStr::new("lib-curl-libssh"), Exp::Version(BStr::new("0.9.3"))),
            ],
            functions: vec![curl(BStr::new("7.68.0"))],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("curl")],
            info: Some(
                BStr::new("
curl

cURL support => enabled
cURL Information => 8.1.2
Age => 10
Features
AsynchDNS => Yes
CharConv => No
Debug => No
GSS-Negotiate => No
IDN => Yes
IPv6 => Yes
krb4 => No
Largefile => Yes
libz => Yes
NTLM => Yes
NTLMWB => Yes
SPNEGO => Yes
SSL => Yes
SSPI => No
TLS-SRP => Yes
HTTP2 => Yes
GSSAPI => Yes
KERBEROS5 => Yes
UNIX_SOCKETS => Yes
PSL => No
HTTPS_PROXY => Yes
MULTI_SSL => Yes
BROTLI => Yes
ALTSVC => Yes
HTTP3 => No
UNICODE => No
ZSTD => Yes
HSTS => Yes
GSASL => No
Protocols => dict, file, ftp, ftps, gopher, gophers, http, https, imap, imaps, ldap, ldaps, mqtt, pop3, pop3s, rtmp, rtmpe, rtmps, rtmpt, rtmpte, rtmpts, rtsp, scp, sftp, smb, smbs, smtp, smtps, telnet, tftp
Host => aarch64-apple-darwin22.4.0
SSL Version => (SecureTransport) OpenSSL/3.1.1
ZLib Version => 1.2.11
libSSH Version => libssh2/1.11.0"),
            ),
            expectations: vec![
                (BStr::new("lib-curl"), Exp::Version(BStr::new("8.1.2"))),
                (
                    BStr::new("lib-curl-securetransport"),
                    Exp::Full(BStr::new("3.1.1"), vec![BStr::new("lib-curl-openssl")], vec![]),
                ),
                (BStr::new("lib-curl-zlib"), Exp::Version(BStr::new("1.2.11"))),
                (BStr::new("lib-curl-libssh2"), Exp::Version(BStr::new("1.11.0"))),
            ],
            functions: vec![curl(BStr::new("8.1.2"))],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("curl")],
            info: Some(
                BStr::new("
curl

cURL support => enabled
cURL Information => 8.1.2
Age => 10
Features
AsynchDNS => Yes
CharConv => No
Debug => No
GSS-Negotiate => No
IDN => No
IPv6 => Yes
krb4 => No
Largefile => Yes
libz => Yes
NTLM => Yes
NTLMWB => Yes
SPNEGO => Yes
SSL => Yes
SSPI => No
TLS-SRP => No
HTTP2 => Yes
GSSAPI => Yes
KERBEROS5 => Yes
UNIX_SOCKETS => Yes
PSL => No
HTTPS_PROXY => Yes
MULTI_SSL => Yes
BROTLI => No
ALTSVC => Yes
HTTP3 => No
UNICODE => No
ZSTD => No
HSTS => Yes
GSASL => No
Protocols => dict, file, ftp, ftps, gopher, gophers, http, https, imap, imaps, ldap, ldaps, mqtt, pop3, pop3s, rtsp, smb, smbs, smtp, smtps, telnet, tftp
Host => x86_64-apple-darwin20.0
SSL Version => (SecureTransport) LibreSSL/2.8.3
ZLib Version => 1.2.11"),
            ),
            expectations: vec![
                (BStr::new("lib-curl"), Exp::Version(BStr::new("8.1.2"))),
                (
                    BStr::new("lib-curl-securetransport"),
                    Exp::Full(BStr::new("2.8.3"), vec![BStr::new("lib-curl-libressl")], vec![]),
                ),
                (BStr::new("lib-curl-zlib"), Exp::Version(BStr::new("1.2.11"))),
            ],
            functions: vec![curl(BStr::new("8.1.2"))],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("date")],
            info: Some(
                BStr::new("
date

date/time support => enabled
timelib version => 2018.03
\"Olson\" Timezone Database Version => 2020.1
Timezone Database => external
Default timezone => Europe/Berlin"),
            ),
            expectations: vec![
                (BStr::new("lib-date-timelib"), Exp::Version(BStr::new("2018.03"))),
                (BStr::new("lib-date-zoneinfo"), Exp::Version(BStr::new("2020.1"))),
            ],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("date")],
            info: Some(
                BStr::new("
date

date/time support => enabled
\"Olson\" Timezone Database Version => 2013.2
Timezone Database => internal
Default timezone => Europe/Amsterdam"),
            ),
            expectations: vec![
                (BStr::new("lib-date-zoneinfo"), Exp::Version(BStr::new("2013.2"))),
                (BStr::new("lib-date-timelib"), Exp::Missing),
            ],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("date"), BStr::new("timezonedb")],
            info: Some(
                BStr::new("
date

date/time support => enabled
\"Olson\" Timezone Database Version => 2020.1
Timezone Database => internal
Default timezone => UTC"),
            ),
            expectations: vec![(BStr::new("lib-date-zoneinfo"), Exp::Version(BStr::new("2020.1")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("date"), BStr::new("timezonedb")],
            info: Some(
                BStr::new("
date

date/time support => enabled
\"Olson\" Timezone Database Version => 2020.1
Timezone Database => external
Default timezone => UTC"),
            ),
            expectations: vec![(
                BStr::new("lib-timezonedb-zoneinfo"),
                Exp::Full(BStr::new("2020.1"), vec![BStr::new("lib-date-zoneinfo")], vec![]),
            )],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("date")],
            info: Some(
                BStr::new("


date/time support => enabled
timelib version => 2018.03
\"Olson\" Timezone Database Version => 0.system
Timezone Database => internal
Default timezone => Europe/Berlin

Directive => Local Value => Master Value
date.timezone => no value => no value
date.default_latitude => 31.7667 => 31.7667
date.default_longitude => 35.2333 => 35.2333
date.sunset_zenith => 90.583333 => 90.583333
date.sunrise_zenith => 90.583333 => 90.583333"),
            ),
            expectations: vec![
                (BStr::new("lib-date-zoneinfo"), Exp::Version(BStr::new("0"))),
                (BStr::new("lib-date-timelib"), Exp::Version(BStr::new("2018.03"))),
            ],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("fileinfo")],
            info: Some(
                BStr::new("
fileinfo

fileinfo support => enabled
libmagic => 537"),
            ),
            expectations: vec![(BStr::new("lib-fileinfo-libmagic"), Exp::Version(BStr::new("537")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("gd")],
            info: Some(
                BStr::new("
gd

GD Support => enabled
GD Version => bundled (2.1.0 compatible)
FreeType Support => enabled
FreeType Linkage => with freetype
FreeType Version => 2.10.0
GIF Read Support => enabled
GIF Create Support => enabled
JPEG Support => enabled
libJPEG Version => 9 compatible
PNG Support => enabled
libPNG Version => 1.6.34
WBMP Support => enabled
XBM Support => enabled
WebP Support => enabled

Directive => Local Value => Master Value
gd.jpeg_ignore_warning => 1 => 1"),
            ),
            expectations: vec![
                (BStr::new("lib-gd"), Exp::Version(BStr::new("1.2.3"))),
                (BStr::new("lib-gd-freetype"), Exp::Version(BStr::new("2.10.0"))),
                (BStr::new("lib-gd-libjpeg"), Exp::Version(BStr::new("9.0"))),
                (BStr::new("lib-gd-libpng"), Exp::Version(BStr::new("1.6.34"))),
            ],
            functions: vec![],
            constants: vec![(BStr::new("GD_VERSION"), None, s(BStr::new("1.2.3")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("gd")],
            info: Some(
                BStr::new("
gd

GD Support => enabled
GD Version => bundled (2.1.0 compatible)
FreeType Support => enabled
FreeType Linkage => with freetype
FreeType Version => 2.9.1
GIF Read Support => enabled
GIF Create Support => enabled
JPEG Support => enabled
libJPEG Version => 6b
PNG Support => enabled
libPNG Version => 1.6.35
WBMP Support => enabled
XBM Support => enabled
WebP Support => enabled

Directive => Local Value => Master Value
gd.jpeg_ignore_warning => 1 => 1"),
            ),
            expectations: vec![
                (BStr::new("lib-gd"), Exp::Version(BStr::new("1.2.3"))),
                (BStr::new("lib-gd-freetype"), Exp::Version(BStr::new("2.9.1"))),
                (BStr::new("lib-gd-libjpeg"), Exp::Version(BStr::new("6.2"))),
                (BStr::new("lib-gd-libpng"), Exp::Version(BStr::new("1.6.35"))),
            ],
            functions: vec![],
            constants: vec![(BStr::new("GD_VERSION"), None, s(BStr::new("1.2.3")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("gd")],
            info: Some(
                BStr::new("
gd

GD Support => enabled
GD headers Version => 2.2.5
GD library Version => 2.2.5
FreeType Support => enabled
FreeType Linkage => with freetype
FreeType Version => 2.6.3
GIF Read Support => enabled
GIF Create Support => enabled
JPEG Support => enabled
libJPEG Version => 6b
PNG Support => enabled
libPNG Version => 1.6.28
WBMP Support => enabled
XPM Support => enabled
libXpm Version => 30411
XBM Support => enabled
WebP Support => enabled

Directive => Local Value => Master Value
gd.jpeg_ignore_warning => 1 => 1"),
            ),
            expectations: vec![
                (BStr::new("lib-gd"), Exp::Version(BStr::new("2.2.5"))),
                (BStr::new("lib-gd-freetype"), Exp::Version(BStr::new("2.6.3"))),
                (BStr::new("lib-gd-libjpeg"), Exp::Version(BStr::new("6.2"))),
                (BStr::new("lib-gd-libpng"), Exp::Version(BStr::new("1.6.28"))),
                (BStr::new("lib-gd-libxpm"), Exp::Version(BStr::new("3.4.11"))),
            ],
            functions: vec![],
            constants: vec![(BStr::new("GD_VERSION"), None, s(BStr::new("2.2.5")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("iconv")],
            info: None,
            expectations: vec![(BStr::new("lib-iconv"), Exp::Version(BStr::new("1.2.4")))],
            functions: vec![],
            constants: vec![(BStr::new("ICONV_VERSION"), None, s(BStr::new("1.2.4")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("gmp")],
            info: None,
            expectations: vec![(BStr::new("lib-gmp"), Exp::Version(BStr::new("6.1.0")))],
            functions: vec![],
            constants: vec![(BStr::new("GMP_VERSION"), None, s(BStr::new("6.1.0")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("intl")],
            info: Some(
                BStr::new("
intl

Internationalization support => enabled
ICU version => 57.1
ICU Data version => 57.1
ICU TZData version => 2016b
ICU Unicode version => 8.0

Directive => Local Value => Master Value
intl.default_locale => no value => no value
intl.error_level => 0 => 0
intl.use_exceptions => 0 => 0"),
            ),
            expectations: vec![
                (BStr::new("lib-icu"), Exp::Version(BStr::new("100"))),
                (BStr::new("lib-icu-cldr"), Exp::Version(BStr::new("32.0.1"))),
                (BStr::new("lib-icu-unicode"), Exp::Version(BStr::new("7.0.0"))),
                (BStr::new("lib-icu-zoneinfo"), Exp::Version(BStr::new("2016.2"))),
            ],
            functions: vec![
                (
                    PhpMixed::List(vec![s(BStr::new("ResourceBundle")), s(BStr::new("create"))]),
                    vec![s(BStr::new("root")), s(BStr::new("ICUDATA")), PhpMixed::Bool(false)],
                    PhpMixed::Object(IndexMap::from([("Version".into(), s(BStr::new("32.0.1")))])),
                ),
                (
                    PhpMixed::List(vec![s(BStr::new("IntlChar")), s(BStr::new("getUnicodeVersion"))]),
                    vec![],
                    PhpMixed::List(vec![
                        PhpMixed::Int(7),
                        PhpMixed::Int(0),
                        PhpMixed::Int(0),
                        PhpMixed::Int(0),
                    ]),
                ),
            ],
            constants: vec![(BStr::new("INTL_ICU_VERSION"), None, s(BStr::new("100")))],
            class_definitions: vec![
                ClassDef {
                    class: BStr::new("ResourceBundle"),
                    construct: None,
                },
                ClassDef {
                    class: BStr::new("IntlChar"),
                    construct: None,
                },
            ],
        },
        LibCase {
            extensions: vec![BStr::new("intl")],
            info: Some(
                BStr::new("
intl

Internationalization support => enabled
version => 1.1.0
ICU version => 57.1
ICU Data version => 57.1"),
            ),
            expectations: vec![(BStr::new("lib-icu"), Exp::Version(BStr::new("57.1")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("imagick")],
            info: None,
            expectations: vec![(
                BStr::new("lib-imagick-imagemagick"),
                Exp::Full(BStr::new("6.2.9"), vec![BStr::new("lib-imagick")], vec![]),
            )],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![ClassDef {
                class: BStr::new("Imagick"),
                construct: Some((
                    vec![],
                    PhpMixed::Object(IndexMap::from([(
                        "versionString".into(),
                        s(BStr::new("ImageMagick 6.2.9 Q16 x86_64 2018-05-18 http://www.imagemagick.org")),
                    )])),
                )),
            }],
        },
        LibCase {
            extensions: vec![BStr::new("imagick")],
            info: None,
            expectations: vec![(
                BStr::new("lib-imagick-imagemagick"),
                Exp::Full(BStr::new("7.0.8.34"), vec![BStr::new("lib-imagick")], vec![]),
            )],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![ClassDef {
                class: BStr::new("Imagick"),
                construct: Some((
                    vec![],
                    PhpMixed::Object(IndexMap::from([(
                        "versionString".into(),
                        s(BStr::new("ImageMagick 7.0.8-34 Q16 x86_64 2019-03-23 https://imagemagick.org")),
                    )])),
                )),
            }],
        },
        LibCase {
            extensions: vec![BStr::new("ldap")],
            info: Some(
                BStr::new("
ldap

LDAP Support => enabled
RCS Version => $Id: 5f1913de8e05a346da913956f81e0c0d8991c7cb $
Total Links => 0/unlimited
API Version => 3001
Vendor Name => OpenLDAP
Vendor Version => 20450
SASL Support => Enabled

Directive => Local Value => Master Value
ldap.max_links => Unlimited => Unlimited"),
            ),
            expectations: vec![(BStr::new("lib-ldap-openldap"), Exp::Version(BStr::new("2.4.50")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("libxml")],
            info: None,
            expectations: vec![(BStr::new("lib-libxml"), Exp::Version(BStr::new("2.1.5")))],
            functions: vec![],
            constants: vec![(BStr::new("LIBXML_DOTTED_VERSION"), None, s(BStr::new("2.1.5")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("libxml"), BStr::new("dom"), BStr::new("simplexml"), BStr::new("xml"), BStr::new("xmlreader"), BStr::new("xmlwriter")],
            info: None,
            expectations: vec![(
                BStr::new("lib-libxml"),
                Exp::Full(
                    BStr::new("2.1.5"),
                    vec![],
                    vec![
                        BStr::new("lib-dom-libxml"),
                        BStr::new("lib-simplexml-libxml"),
                        BStr::new("lib-xml-libxml"),
                        BStr::new("lib-xmlreader-libxml"),
                        BStr::new("lib-xmlwriter-libxml"),
                    ],
                ),
            )],
            functions: vec![],
            constants: vec![(BStr::new("LIBXML_DOTTED_VERSION"), None, s(BStr::new("2.1.5")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("mbstring")],
            info: Some(
                BStr::new("
mbstring

Multibyte Support => enabled
Multibyte string engine => libmbfl
HTTP input encoding translation => disabled
libmbfl version => 1.3.2

mbstring extension makes use of \"streamable kanji code filter and converter\", which is distributed under the GNU Lesser General Public License version 2.1.

Multibyte (japanese) regex support => enabled
Multibyte regex (oniguruma) version => 6.1.3"),
            ),
            expectations: vec![
                (BStr::new("lib-mbstring-libmbfl"), Exp::Version(BStr::new("1.3.2"))),
                (BStr::new("lib-mbstring-oniguruma"), Exp::Version(BStr::new("7.0.0"))),
            ],
            functions: vec![],
            constants: vec![(BStr::new("MB_ONIGURUMA_VERSION"), None, s(BStr::new("7.0.0")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("mbstring")],
            info: Some(
                BStr::new("
mbstring

Multibyte Support => enabled
Multibyte string engine => libmbfl
HTTP input encoding translation => disabled
libmbfl version => 1.3.2

mbstring extension makes use of \"streamable kanji code filter and converter\", which is distributed under the GNU Lesser General Public License version 2.1.

Multibyte (japanese) regex support => enabled
Multibyte regex (oniguruma) version => 6.1.3"),
            ),
            expectations: vec![
                (BStr::new("lib-mbstring-libmbfl"), Exp::Version(BStr::new("1.3.2"))),
                (BStr::new("lib-mbstring-oniguruma"), Exp::Version(BStr::new("6.1.3"))),
            ],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("mbstring")],
            info: Some(
                BStr::new("
mbstring

Multibyte Support => enabled
Multibyte string engine => libmbfl
HTTP input encoding translation => disabled
libmbfl version => 1.3.2
oniguruma version => 6.9.4

mbstring extension makes use of \"streamable kanji code filter and converter\", which is distributed under the GNU Lesser General Public License version 2.1.

Multibyte (japanese) regex support => enabled
Multibyte regex (oniguruma) backtrack check => On"),
            ),
            expectations: vec![
                (BStr::new("lib-mbstring-libmbfl"), Exp::Version(BStr::new("1.3.2"))),
                (BStr::new("lib-mbstring-oniguruma"), Exp::Version(BStr::new("6.9.4"))),
            ],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("memcached")],
            info: Some(
                BStr::new("
memcached

memcached support => enabled
Version => 3.1.5
libmemcached version => 1.0.18
SASL support => yes
Session support => yes
igbinary support => yes
json support => yes
msgpack support => yes"),
            ),
            expectations: vec![(BStr::new("lib-memcached-libmemcached"), Exp::Version(BStr::new("1.0.18")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("openssl")],
            info: None,
            expectations: vec![(BStr::new("lib-openssl"), Exp::Version(BStr::new("1.1.1.7")))],
            functions: vec![],
            constants: vec![(BStr::new("OPENSSL_VERSION_TEXT"), None, s(BStr::new("OpenSSL 1.1.1g  21 Apr 2020")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("openssl")],
            info: None,
            expectations: vec![(BStr::new("lib-openssl"), Exp::Version(BStr::new("1.1.1.7")))],
            functions: vec![],
            constants: vec![(
                BStr::new("OPENSSL_VERSION_TEXT"),
                None,
                s(BStr::new("OpenSSL 1.1.1g-freebsd  21 Apr 2020")),
            )],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("openssl")],
            info: None,
            expectations: vec![(BStr::new("lib-openssl"), Exp::Version(BStr::new("0.9.8.33")))],
            functions: vec![],
            constants: vec![(BStr::new("OPENSSL_VERSION_TEXT"), None, s(BStr::new("OpenSSL 0.9.8zg  21 Apr 2020")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("openssl")],
            info: None,
            expectations: vec![(BStr::new("lib-openssl"), Exp::Version(BStr::new("1.1.1.7-alpha1")))],
            functions: vec![],
            constants: vec![(BStr::new("OPENSSL_VERSION_TEXT"), None, s(BStr::new("OpenSSL 1.1.1g-pre1  21 Apr 2020")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("openssl")],
            info: None,
            expectations: vec![(BStr::new("lib-openssl"), Exp::Version(BStr::new("1.1.1.7-beta2")))],
            functions: vec![],
            constants: vec![(
                BStr::new("OPENSSL_VERSION_TEXT"),
                None,
                s(BStr::new("OpenSSL 1.1.1g-beta2  21 Apr 2020")),
            )],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("openssl")],
            info: None,
            expectations: vec![(BStr::new("lib-openssl"), Exp::Version(BStr::new("1.1.1.7-alpha4")))],
            functions: vec![],
            constants: vec![(
                BStr::new("OPENSSL_VERSION_TEXT"),
                None,
                s(BStr::new("OpenSSL 1.1.1g-alpha4  21 Apr 2020")),
            )],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("openssl")],
            info: None,
            expectations: vec![(BStr::new("lib-openssl"), Exp::Version(BStr::new("1.1.1.7-rc2")))],
            functions: vec![],
            constants: vec![(BStr::new("OPENSSL_VERSION_TEXT"), None, s(BStr::new("OpenSSL 1.1.1g-rc2  21 Apr 2020")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("openssl")],
            info: None,
            expectations: vec![(
                BStr::new("lib-openssl-fips"),
                Exp::Full(BStr::new("1.1.1.7"), vec![], vec![BStr::new("lib-openssl")]),
            )],
            functions: vec![],
            constants: vec![(BStr::new("OPENSSL_VERSION_TEXT"), None, s(BStr::new("OpenSSL 1.1.1g-fips  21 Apr 2020")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("openssl")],
            info: None,
            expectations: vec![(BStr::new("lib-openssl"), Exp::Version(BStr::new("2.0.1.0")))],
            functions: vec![],
            constants: vec![(BStr::new("OPENSSL_VERSION_TEXT"), None, s(BStr::new("LibreSSL 2.0.1")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("mysqlnd")],
            info: Some(
                BStr::new("
                mysqlnd

mysqlnd => enabled
Version => mysqlnd 5.0.11-dev - 20150407 - $Id: 38fea24f2847fa7519001be390c98ae0acafe387 $
Compression => supported
core SSL => supported
extended SSL => supported
Command buffer size => 4096
Read buffer size => 32768
Read timeout => 31536000
Collecting statistics => Yes
Collecting memory statistics => Yes
Tracing => n/a
Loaded plugins => mysqlnd,debug_trace,auth_plugin_mysql_native_password,auth_plugin_mysql_clear_password,auth_plugin_sha256_password
API Extensions => pdo_mysql,mysqli"),
            ),
            expectations: vec![(BStr::new("lib-mysqlnd-mysqlnd"), Exp::Version(BStr::new("5.0.11-dev")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("pdo_mysql")],
            info: Some(
                BStr::new("
                pdo_mysql

PDO Driver for MySQL => enabled
Client API version => mysqlnd 5.0.10-dev - 20150407 - $Id: 38fea24f2847fa7519001be390c98ae0acafe387 $

Directive => Local Value => Master Value
pdo_mysql.default_socket => /tmp/mysql.sock => /tmp/mysql.sock"),
            ),
            expectations: vec![(BStr::new("lib-pdo_mysql-mysqlnd"), Exp::Version(BStr::new("5.0.10-dev")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("mongodb")],
            info: Some(
                BStr::new("
                mongodb

MongoDB support => enabled
MongoDB extension version => 1.6.1
MongoDB extension stability => stable
libbson bundled version => 1.15.2
libmongoc bundled version => 1.15.2
libmongoc SSL => enabled
libmongoc SSL library => OpenSSL
libmongoc crypto => enabled
libmongoc crypto library => libcrypto
libmongoc crypto system profile => disabled
libmongoc SASL => disabled
libmongoc ICU => enabled
libmongoc compression => enabled
libmongoc compression snappy => disabled
libmongoc compression zlib => enabled

Directive => Local Value => Master Value
mongodb.debug => no value => no value"),
            ),
            expectations: vec![
                (BStr::new("lib-mongodb-libmongoc"), Exp::Version(BStr::new("1.15.2"))),
                (BStr::new("lib-mongodb-libbson"), Exp::Version(BStr::new("1.15.2"))),
            ],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("pcre")],
            info: Some(
                BStr::new("
pcre

PCRE (Perl Compatible Regular Expressions) Support => enabled
PCRE Library Version => 10.33 2019-04-16
PCRE Unicode Version => 11.0.0
PCRE JIT Support => enabled
PCRE JIT Target => x86 64bit (little endian + unaligned)"),
            ),
            expectations: vec![
                (BStr::new("lib-pcre"), Exp::Version(BStr::new("10.33"))),
                (BStr::new("lib-pcre-unicode"), Exp::Version(BStr::new("11.0.0"))),
            ],
            functions: vec![],
            constants: vec![(BStr::new("PCRE_VERSION"), None, s(BStr::new("10.33 2019-04-16")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("pcre")],
            info: Some(
                BStr::new("
pcre

PCRE (Perl Compatible Regular Expressions) Support => enabled
PCRE Library Version => 8.38 2015-11-23

Directive => Local Value => Master Value
pcre.backtrack_limit => 1000000 => 1000000
pcre.recursion_limit => 100000 => 100000
                "),
            ),
            expectations: vec![(BStr::new("lib-pcre"), Exp::Version(BStr::new("8.38")))],
            functions: vec![],
            constants: vec![(BStr::new("PCRE_VERSION"), None, s(BStr::new("8.38 2015-11-23")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("pgsql")],
            info: Some(
                BStr::new("
pgsql

PostgreSQL Support => enabled
PostgreSQL(libpq) Version => 12.2
PostgreSQL(libpq)  => PostgreSQL 12.3 on x86_64-apple-darwin18.7.0, compiled by Apple clang version 11.0.0 (clang-1100.0.33.17), 64-bit
Multibyte character support => enabled
SSL support => enabled
Active Persistent Links => 0
Active Links => 0

Directive => Local Value => Master Value
pgsql.allow_persistent => On => On
pgsql.max_persistent => Unlimited => Unlimited
pgsql.max_links => Unlimited => Unlimited
pgsql.auto_reset_persistent => Off => Off
pgsql.ignore_notice => Off => Off
pgsql.log_notice => Off => Off"),
            ),
            expectations: vec![(BStr::new("lib-pgsql-libpq"), Exp::Version(BStr::new("12.2")))],
            functions: vec![],
            constants: vec![(BStr::new("PGSQL_LIBPQ_VERSION"), None, s(BStr::new("12.2")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("pdo_pgsql")],
            info: Some(
                BStr::new("
                pdo_pgsql

PDO Driver for PostgreSQL => enabled
PostgreSQL(libpq) Version => 12.1
Module version => 7.1.33
Revision =>  $Id: 9c5f356c77143981d2e905e276e439501fe0f419 $"),
            ),
            expectations: vec![(BStr::new("lib-pdo_pgsql-libpq"), Exp::Version(BStr::new("12.1")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("pq")],
            info: Some(
                BStr::new("pq

PQ Support => enabled
Extension Version => 2.2.0

Used Library => Compiled => Linked
libpq => 14.3 (Ubuntu 14.3-1.pgdg22.04+1) => 15.0.2
                "),
            ),
            expectations: vec![(BStr::new("lib-pq-libpq"), Exp::Version(BStr::new("15.0.2")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("rdkafka")],
            info: None,
            expectations: vec![(BStr::new("lib-rdkafka-librdkafka"), Exp::Version(BStr::new("1.9.2")))],
            functions: vec![],
            constants: vec![(BStr::new("RD_KAFKA_VERSION"), None, PhpMixed::Int(17367807))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("libsodium")],
            info: None,
            expectations: vec![(BStr::new("lib-libsodium"), Exp::Version(BStr::new("1.0.17")))],
            functions: vec![],
            constants: vec![(BStr::new("SODIUM_LIBRARY_VERSION"), None, s(BStr::new("1.0.17")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("sodium")],
            info: None,
            expectations: vec![(BStr::new("lib-libsodium"), Exp::Version(BStr::new("1.0.15")))],
            functions: vec![],
            constants: vec![(BStr::new("SODIUM_LIBRARY_VERSION"), None, s(BStr::new("1.0.15")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("pdo_sqlite")],
            info: Some(
                BStr::new("
pdo_sqlite

PDO Driver for SQLite 3.x => enabled
SQLite Library => 3.32.3
                "),
            ),
            expectations: vec![(BStr::new("lib-pdo_sqlite-sqlite"), Exp::Version(BStr::new("3.32.3")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("sqlite3")],
            info: Some(
                BStr::new("
sqlite3

SQLite3 support => enabled
SQLite3 module version => 7.1.33
SQLite Library => 3.31.0

Directive => Local Value => Master Value
sqlite3.extension_dir => no value => no value
sqlite3.defensive => 1 => 1"),
            ),
            expectations: vec![(BStr::new("lib-sqlite3-sqlite"), Exp::Version(BStr::new("3.31.0")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("ssh2")],
            info: Some(
                BStr::new("
ssh2

SSH2 support => enabled
extension version => 1.2
libssh2 version => 1.8.0
banner => SSH-2.0-libssh2_1.8.0"),
            ),
            expectations: vec![(BStr::new("lib-ssh2-libssh2"), Exp::Version(BStr::new("1.8.0")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("yaml")],
            info: Some(
                BStr::new("
                yaml

LibYAML Support => enabled
Module Version => 2.0.2
LibYAML Version => 0.2.2

Directive => Local Value => Master Value
yaml.decode_binary => 0 => 0
yaml.decode_timestamp => 0 => 0
yaml.decode_php => 0 => 0
yaml.output_canonical => 0 => 0
yaml.output_indent => 2 => 2
yaml.output_width => 80 => 80"),
            ),
            expectations: vec![(BStr::new("lib-yaml-libyaml"), Exp::Version(BStr::new("0.2.2")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("xsl")],
            info: Some(
                BStr::new("
xsl

XSL => enabled
libxslt Version => 1.1.33
libxslt compiled against libxml Version => 2.9.8
EXSLT => enabled
libexslt Version => 1.1.29"),
            ),
            expectations: vec![
                (BStr::new("lib-libxslt"), Exp::Full(BStr::new("1.1.29"), vec![BStr::new("lib-xsl")], vec![])),
                (BStr::new("lib-libxslt-libxml"), Exp::Version(BStr::new("2.9.8"))),
            ],
            functions: vec![],
            constants: vec![(BStr::new("LIBXSLT_DOTTED_VERSION"), None, s(BStr::new("1.1.29")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("zip")],
            info: None,
            expectations: vec![(BStr::new("lib-zip-libzip"), Exp::Full(BStr::new("1.5.0"), vec![BStr::new("lib-zip")], vec![]))],
            functions: vec![],
            constants: vec![(BStr::new("LIBZIP_VERSION"), Some(BStr::new("ZipArchive")), s(BStr::new("1.5.0")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("zlib")],
            info: None,
            expectations: vec![(BStr::new("lib-zlib"), Exp::Version(BStr::new("1.2.10")))],
            functions: vec![],
            constants: vec![(BStr::new("ZLIB_VERSION"), None, s(BStr::new("1.2.10")))],
            class_definitions: vec![],
        },
        LibCase {
            extensions: vec![BStr::new("zlib")],
            info: Some(
                BStr::new("
zlib

ZLib Support => enabled
Stream Wrapper => compress.zlib://
Stream Filter => zlib.inflate, zlib.deflate
Compiled Version => 1.2.8
Linked Version => 1.2.11"),
            ),
            expectations: vec![(BStr::new("lib-zlib"), Exp::Version(BStr::new("1.2.11")))],
            functions: vec![],
            constants: vec![],
            class_definitions: vec![],
        },
    ]
}

fn assert_package_links(
    context: &BStr,
    expected_links: &[&BStr],
    source_package: &BasePackageHandle,
    links: &IndexMap<BString, Link>,
) {
    assert_eq!(
        expected_links.len(),
        links.len(),
        "{}: expected package count to match",
        context.display_lossy()
    );

    for link in links.values() {
        assert_eq!(source_package.get_name(), link.get_source());
        assert!(
            expected_links.iter().any(|l| link.get_target() == *l),
            "{}: package {} not in {:?}",
            context.display_lossy(),
            link.get_target().display_lossy(),
            expected_links
        );
        let version_constraint = SimpleConstraint::new("=", source_package.get_version(), None);
        assert!(link.get_constraint().matches(&version_constraint.into()));
    }
}

#[test]
fn test_library_information() {
    let extension_version = BStr::new("100.200.300");

    for case in library_test_cases() {
        let extensions: Vec<BString> = case.extensions.iter().map(|e| e.to_php_string()).collect();

        let mut constants: Vec<(BString, Option<BString>, PhpMixed)> = case
            .constants
            .iter()
            .map(|(n, c, v)| (n.to_php_string(), c.map(|s| s.to_php_string()), v.clone()))
            .collect();
        constants.push((
            BString::from("PHP_VERSION"),
            None,
            PhpMixed::string(BString::from("7.1.0")),
        ));

        let platform_info = platform_info(
            constants,
            extensions.clone(),
            extension_version,
            case.info,
            &case.functions,
            &case.class_definitions,
        );

        let mut platform_repository =
            PlatformRepository::new(vec![], IndexMap::new(), Some(platform_info), None).unwrap();

        let libraries: Vec<bstr::BString> = platform_repository
            .search(BString::from("lib"), SEARCH_NAME, None)
            .unwrap()
            .into_iter()
            .map(|package| package.name)
            .filter(|name| name.starts_with(b"lib-"))
            .collect();
        let expected_libraries: Vec<&BStr> = case
            .expectations
            .iter()
            .filter(|(_, exp)| !matches!(exp, Exp::Missing))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            expected_libraries.len(),
            libraries.len(),
            "Expected: {:?}, got {:?}",
            expected_libraries,
            libraries
        );

        let mut all_expectations: Vec<(BString, (Option<&BStr>, &[&BStr], &[&BStr]))> = case
            .expectations
            .iter()
            .map(|(name, exp)| (name.to_php_string(), exp.parts()))
            .collect();
        for extension in &extensions {
            all_expectations.push((
                bformat!("ext-{extension}"),
                (Some(extension_version), &[], &[]),
            ));
        }

        for (package_name, (expected_version, expected_replaces, expected_provides)) in
            all_expectations
        {
            let package = platform_repository
                .find_package(
                    bstr::BStr::new(&package_name),
                    FindPackageConstraint::String(BString::from("*")),
                )
                .unwrap();
            match expected_version {
                None => assert!(
                    package.is_none(),
                    "Expected to not find package \"{}\"",
                    package_name.display_lossy()
                ),
                Some(expected_version) => {
                    assert!(
                        package.is_some(),
                        "Expected to find package \"{}\"",
                        package_name.display_lossy()
                    );
                    let package = package.unwrap();
                    assert_eq!(
                        expected_version,
                        package.get_pretty_version(),
                        "Expected version {} for {}",
                        expected_version.display_lossy(),
                        package_name.display_lossy()
                    );
                    assert_package_links(
                        BStr::new("replaces"),
                        expected_replaces,
                        &package,
                        &package.get_replaces(),
                    );
                    assert_package_links(
                        BStr::new("provides"),
                        expected_provides,
                        &package,
                        &package.get_provides(),
                    );
                }
            }
        }
    }
}

#[test]
fn test_composer_platform_version() {
    let constants: IndexMap<BString, PhpMixed> = IndexMap::from([
        (
            "PHP_VERSION".into(),
            PhpMixed::string(BString::from("7.0.0")),
        ),
        ("PHP_DEBUG".into(), PhpMixed::Bool(false)),
    ]);

    // PHP only stubs getExtensions/getConstant; PHPUnit auto-returns null/false for the
    // other methods, which is what the default payload reports.
    let platform_info = platform_info(
        constants
            .into_iter()
            .map(|(constant_name, value)| (constant_name, None, value))
            .collect(),
        Vec::new(),
        BStr::new(""),
        None,
        &[],
        &[],
    );

    let mut platform_repository =
        PlatformRepository::new(vec![], IndexMap::new(), Some(platform_info), None).unwrap();

    let package = platform_repository
        .find_package(
            bstr::BStr::new("composer"),
            FindPackageConstraint::String(bformat!("={}", shirabe::composer::get_version())),
        )
        .unwrap();
    assert!(package.is_some(), "Composer package exists");
}

#[test]
fn test_valid_platform_packages() {
    let cases: Vec<(&BStr, bool)> = vec![
        (BStr::new("php"), true),
        (BStr::new("php-debug"), true),
        (BStr::new("php-ipv6"), true),
        (BStr::new("php-64bit"), true),
        (BStr::new("php-zts"), true),
        (BStr::new("hhvm"), true),
        (BStr::new("hhvm-foo"), false),
        (BStr::new("ext-foo"), true),
        (BStr::new("ext-123"), true),
        (BStr::new("extfoo"), false),
        (BStr::new("ext"), false),
        (BStr::new("lib-foo"), true),
        (BStr::new("lib-123"), true),
        (BStr::new("libfoo"), false),
        (BStr::new("lib"), false),
        (BStr::new("composer"), true),
        (BStr::new("composer-foo"), false),
        (BStr::new("composer-plugin-api"), true),
        (BStr::new("composer-plugin"), false),
        (BStr::new("composer-runtime-api"), true),
        (BStr::new("composer-runtime"), false),
    ];

    for (package_name, expectation) in cases {
        assert_eq!(
            expectation,
            PlatformRepository::is_platform_package(package_name)
        );
    }
}
