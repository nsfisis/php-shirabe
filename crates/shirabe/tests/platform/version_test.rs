//! ref: composer/tests/Composer/Test/Platform/VersionTest.php

use shirabe::platform::version::Version;
use shirabe_semver::version_parser::VersionParser;

fn provide_openssl_versions() -> Vec<(&'static str, &'static str, bool, Option<&'static str>)> {
    vec![
        // Generated
        ("1.2.3", "1.2.3.0", false, None),
        ("1.2.3-beta3", "1.2.3.0-beta3", false, None),
        ("1.2.3-beta3-dev", "1.2.3.0-beta3-dev", false, None),
        ("1.2.3-beta3-fips", "1.2.3.0-beta3", true, None),
        ("1.2.3-beta3-fips-dev", "1.2.3.0-beta3-dev", true, None),
        ("1.2.3-dev", "1.2.3.0-dev", false, None),
        ("1.2.3-fips", "1.2.3.0", true, None),
        ("1.2.3-fips-beta3", "1.2.3.0-beta3", true, None),
        ("1.2.3-fips-beta3-dev", "1.2.3.0-beta3-dev", true, None),
        ("1.2.3-fips-dev", "1.2.3.0-dev", true, None),
        ("1.2.3-pre2", "1.2.3.0-alpha2", false, None),
        ("1.2.3-pre2-dev", "1.2.3.0-alpha2-dev", false, None),
        ("1.2.3-pre2-fips", "1.2.3.0-alpha2", true, None),
        ("1.2.3-pre2-fips-dev", "1.2.3.0-alpha2-dev", true, None),
        ("1.2.3a", "1.2.3.1", false, None),
        ("1.2.3a-beta3", "1.2.3.1-beta3", false, None),
        ("1.2.3a-beta3-dev", "1.2.3.1-beta3-dev", false, None),
        ("1.2.3a-dev", "1.2.3.1-dev", false, None),
        ("1.2.3a-dev-fips", "1.2.3.1-dev", true, None),
        ("1.2.3a-fips", "1.2.3.1", true, None),
        ("1.2.3a-fips-beta3", "1.2.3.1-beta3", true, None),
        ("1.2.3a-fips-dev", "1.2.3.1-dev", true, None),
        ("1.2.3beta3", "1.2.3.0-beta3", false, None),
        ("1.2.3beta3-dev", "1.2.3.0-beta3-dev", false, None),
        ("1.2.3zh", "1.2.3.34", false, None),
        ("1.2.3zh-dev", "1.2.3.34-dev", false, None),
        ("1.2.3zh-fips", "1.2.3.34", true, None),
        ("1.2.3zh-fips-dev", "1.2.3.34-dev", true, None),
        // Additional cases
        ("1.2.3zh-fips-rc3", "1.2.3.34-rc3", true, Some("1.2.3.34-RC3")),
        ("1.2.3zh-alpha10-fips", "1.2.3.34-alpha10", true, None),
        ("1.1.1l (Schannel)", "1.1.1.12", false, None),
        // Check that alphabetical patch levels overflow correctly
        ("1.2.3", "1.2.3.0", false, None),
        ("1.2.3a", "1.2.3.1", false, None),
        ("1.2.3z", "1.2.3.26", false, None),
        ("1.2.3za", "1.2.3.27", false, None),
        ("1.2.3zy", "1.2.3.51", false, None),
        ("1.2.3zz", "1.2.3.52", false, None),
        // 3.x
        ("3.0.0", "3.0.0", false, Some("3.0.0.0")),
        ("3.2.4-dev", "3.2.4-dev", false, Some("3.2.4.0-dev")),
    ]
}

#[test]
#[ignore = "compile_php_pattern in the php-shim cannot yet parse Version's PCRE patterns"]
fn test_parse_openssl_versions() {
    for (input, parsed_version, fips_expected, normalized_version) in provide_openssl_versions() {
        let mut is_fips = false;
        assert_eq!(
            Some(parsed_version.to_string()),
            Version::parse_openssl(input, &mut is_fips)
        );
        assert_eq!(fips_expected, is_fips);

        let normalized_version = normalized_version.unwrap_or(parsed_version);
        assert_eq!(
            normalized_version,
            VersionParser.normalize(parsed_version, None).unwrap()
        );
    }
}

#[test]
#[ignore = "compile_php_pattern in the php-shim cannot yet parse Version's PCRE patterns"]
fn test_parse_libjpeg_version() {
    let cases = [
        ("9", "9.0"),
        ("9a", "9.1"),
        ("9b", "9.2"),
        // Never seen in the wild, just for overflow correctness
        ("9za", "9.27"),
    ];

    for (input, parsed_version) in cases {
        assert_eq!(
            Some(parsed_version.to_string()),
            Version::parse_libjpeg(input)
        );
    }
}

#[test]
#[ignore = "compile_php_pattern in the php-shim cannot yet parse Version's PCRE patterns"]
fn test_parse_zoneinfo_version() {
    let cases = [
        ("2019c", "2019.3"),
        ("2020a", "2020.1"),
        // Never happened so far but fixate overflow behavior
        ("2020za", "2020.27"),
    ];

    for (input, parsed_version) in cases {
        assert_eq!(
            Some(parsed_version.to_string()),
            Version::parse_zoneinfo_version(input)
        );
    }
}
