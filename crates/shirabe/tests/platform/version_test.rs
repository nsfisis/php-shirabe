//! ref: composer/tests/Composer/Test/Platform/VersionTest.php sha1:dac663441d569b0c89b96df5fde03d8272991b0e

use bstr::BStr;
use shirabe::platform::version::Version;
use shirabe_semver::VersionParser;

fn provide_openssl_versions() -> Vec<(&'static BStr, &'static BStr, bool, Option<&'static BStr>)> {
    vec![
        // Generated
        (BStr::new("1.2.3"), BStr::new("1.2.3.0"), false, None),
        (
            BStr::new("1.2.3-beta3"),
            BStr::new("1.2.3.0-beta3"),
            false,
            None,
        ),
        (
            BStr::new("1.2.3-beta3-dev"),
            BStr::new("1.2.3.0-beta3-dev"),
            false,
            None,
        ),
        (
            BStr::new("1.2.3-beta3-fips"),
            BStr::new("1.2.3.0-beta3"),
            true,
            None,
        ),
        (
            BStr::new("1.2.3-beta3-fips-dev"),
            BStr::new("1.2.3.0-beta3-dev"),
            true,
            None,
        ),
        (
            BStr::new("1.2.3-dev"),
            BStr::new("1.2.3.0-dev"),
            false,
            None,
        ),
        (BStr::new("1.2.3-fips"), BStr::new("1.2.3.0"), true, None),
        (
            BStr::new("1.2.3-fips-beta3"),
            BStr::new("1.2.3.0-beta3"),
            true,
            None,
        ),
        (
            BStr::new("1.2.3-fips-beta3-dev"),
            BStr::new("1.2.3.0-beta3-dev"),
            true,
            None,
        ),
        (
            BStr::new("1.2.3-fips-dev"),
            BStr::new("1.2.3.0-dev"),
            true,
            None,
        ),
        (
            BStr::new("1.2.3-pre2"),
            BStr::new("1.2.3.0-alpha2"),
            false,
            None,
        ),
        (
            BStr::new("1.2.3-pre2-dev"),
            BStr::new("1.2.3.0-alpha2-dev"),
            false,
            None,
        ),
        (
            BStr::new("1.2.3-pre2-fips"),
            BStr::new("1.2.3.0-alpha2"),
            true,
            None,
        ),
        (
            BStr::new("1.2.3-pre2-fips-dev"),
            BStr::new("1.2.3.0-alpha2-dev"),
            true,
            None,
        ),
        (BStr::new("1.2.3a"), BStr::new("1.2.3.1"), false, None),
        (
            BStr::new("1.2.3a-beta3"),
            BStr::new("1.2.3.1-beta3"),
            false,
            None,
        ),
        (
            BStr::new("1.2.3a-beta3-dev"),
            BStr::new("1.2.3.1-beta3-dev"),
            false,
            None,
        ),
        (
            BStr::new("1.2.3a-dev"),
            BStr::new("1.2.3.1-dev"),
            false,
            None,
        ),
        (
            BStr::new("1.2.3a-dev-fips"),
            BStr::new("1.2.3.1-dev"),
            true,
            None,
        ),
        (BStr::new("1.2.3a-fips"), BStr::new("1.2.3.1"), true, None),
        (
            BStr::new("1.2.3a-fips-beta3"),
            BStr::new("1.2.3.1-beta3"),
            true,
            None,
        ),
        (
            BStr::new("1.2.3a-fips-dev"),
            BStr::new("1.2.3.1-dev"),
            true,
            None,
        ),
        (
            BStr::new("1.2.3beta3"),
            BStr::new("1.2.3.0-beta3"),
            false,
            None,
        ),
        (
            BStr::new("1.2.3beta3-dev"),
            BStr::new("1.2.3.0-beta3-dev"),
            false,
            None,
        ),
        (BStr::new("1.2.3zh"), BStr::new("1.2.3.34"), false, None),
        (
            BStr::new("1.2.3zh-dev"),
            BStr::new("1.2.3.34-dev"),
            false,
            None,
        ),
        (BStr::new("1.2.3zh-fips"), BStr::new("1.2.3.34"), true, None),
        (
            BStr::new("1.2.3zh-fips-dev"),
            BStr::new("1.2.3.34-dev"),
            true,
            None,
        ),
        // Additional cases
        (
            BStr::new("1.2.3zh-fips-rc3"),
            BStr::new("1.2.3.34-rc3"),
            true,
            Some(BStr::new("1.2.3.34-RC3")),
        ),
        (
            BStr::new("1.2.3zh-alpha10-fips"),
            BStr::new("1.2.3.34-alpha10"),
            true,
            None,
        ),
        (
            BStr::new("1.1.1l (Schannel)"),
            BStr::new("1.1.1.12"),
            false,
            None,
        ),
        // Check that alphabetical patch levels overflow correctly
        (BStr::new("1.2.3"), BStr::new("1.2.3.0"), false, None),
        (BStr::new("1.2.3a"), BStr::new("1.2.3.1"), false, None),
        (BStr::new("1.2.3z"), BStr::new("1.2.3.26"), false, None),
        (BStr::new("1.2.3za"), BStr::new("1.2.3.27"), false, None),
        (BStr::new("1.2.3zy"), BStr::new("1.2.3.51"), false, None),
        (BStr::new("1.2.3zz"), BStr::new("1.2.3.52"), false, None),
        // 3.x
        (
            BStr::new("3.0.0"),
            BStr::new("3.0.0"),
            false,
            Some(BStr::new("3.0.0.0")),
        ),
        (
            BStr::new("3.2.4-dev"),
            BStr::new("3.2.4-dev"),
            false,
            Some(BStr::new("3.2.4.0-dev")),
        ),
    ]
}

#[test]
fn test_parse_openssl_versions() {
    for (input, parsed_version, fips_expected, normalized_version) in provide_openssl_versions() {
        let mut is_fips = false;
        assert_eq!(
            Some(bstr::BString::from(parsed_version)),
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
            Some(bstr::BString::from(parsed_version)),
            Version::parse_libjpeg(input)
        );
    }
}

#[test]
fn test_parse_zoneinfo_version() {
    let cases = [
        ("2019c", "2019.3"),
        ("2020a", "2020.1"),
        // Never happened so far but fixate overflow behavior
        ("2020za", "2020.27"),
    ];

    for (input, parsed_version) in cases {
        assert_eq!(
            Some(bstr::BString::from(parsed_version)),
            Version::parse_zoneinfo_version(input)
        );
    }
}
