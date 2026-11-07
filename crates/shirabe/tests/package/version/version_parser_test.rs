//! ref: composer/tests/Composer/Test/Package/Version/VersionParserTest.php sha1:ae8b539ae640cfcdbe5f182da498ebf1516da2b9

use bstr::{BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe::package::version::VersionParser;
use shirabe_php_shim::bformat;

fn pairs(items: &[&BStr]) -> Vec<BString> {
    items.iter().map(|s| bformat!("{s}")).collect()
}

fn entry(fields: &[(&BStr, &BStr)]) -> IndexMap<BString, BString> {
    fields
        .iter()
        .map(|(k, v)| (bformat!("{k}"), bformat!("{v}")))
        .collect()
}

#[test]
#[ignore = "VersionParser::parse_name_version_pairs uses the PCRE lookaround pattern \
            {(?<=[a-z0-9_/-])\\*|\\*(?=[a-z0-9_/-])}i, which regex-crate rejects (look-around, \
            including look-ahead and look-behind, is not supported); not yet converted per \
            docs/dev/regex-porting.md"]
fn test_parse_name_version_pairs() {
    for (input, result) in provide_parse_name_version_pairs_data() {
        let version_parser = VersionParser::new();

        assert_eq!(
            result
                .iter()
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| (BString::from(k.as_bstr()), BString::from(v.as_bstr())))
                        .collect::<IndexMap<_, _>>()
                })
                .collect::<Vec<_>>(),
            version_parser
                .parse_name_version_pairs(input.into_iter().collect::<Vec<_>>())
                .unwrap()
        );
    }
}

fn provide_parse_name_version_pairs_data() -> Vec<(Vec<BString>, Vec<IndexMap<BString, BString>>)> {
    vec![
        (
            pairs(&[BStr::new("php:^7.0")]),
            vec![entry(&[
                (BStr::new("name"), BStr::new("php")),
                (BStr::new("version"), BStr::new("^7.0")),
            ])],
        ),
        (
            pairs(&[BStr::new("php"), BStr::new("^7.0")]),
            vec![entry(&[
                (BStr::new("name"), BStr::new("php")),
                (BStr::new("version"), BStr::new("^7.0")),
            ])],
        ),
        (
            pairs(&[BStr::new("php"), BStr::new("ext-apcu")]),
            vec![
                entry(&[(BStr::new("name"), BStr::new("php"))]),
                entry(&[(BStr::new("name"), BStr::new("ext-apcu"))]),
            ],
        ),
        (
            pairs(&[
                BStr::new("foo/*"),
                BStr::new("bar*"),
                BStr::new("acme/baz"),
                BStr::new("*@dev"),
            ]),
            vec![
                entry(&[(BStr::new("name"), BStr::new("foo/*"))]),
                entry(&[(BStr::new("name"), BStr::new("bar*"))]),
                entry(&[
                    (BStr::new("name"), BStr::new("acme/baz")),
                    (BStr::new("version"), BStr::new("*@dev")),
                ]),
            ],
        ),
        (
            pairs(&[BStr::new("php"), BStr::new("*")]),
            vec![entry(&[
                (BStr::new("name"), BStr::new("php")),
                (BStr::new("version"), BStr::new("*")),
            ])],
        ),
    ]
}

#[test]
fn test_is_upgrade() {
    for (from, to, expected) in provide_is_upgrade_tests() {
        assert_eq!(expected, VersionParser::is_upgrade(&from, &to).unwrap());
    }
}

fn provide_is_upgrade_tests() -> Vec<(BString, BString, bool)> {
    vec![
        (BString::from("0.9.0.0"), BString::from("1.0.0.0"), true),
        (BString::from("1.0.0.0"), BString::from("0.9.0.0"), false),
        (
            BString::from("1.0.0.0"),
            VersionParser::DEFAULT_BRANCH_ALIAS.to_owned(),
            true,
        ),
        (
            VersionParser::DEFAULT_BRANCH_ALIAS.to_owned(),
            VersionParser::DEFAULT_BRANCH_ALIAS.to_owned(),
            true,
        ),
        (
            VersionParser::DEFAULT_BRANCH_ALIAS.to_owned(),
            BString::from("1.0.0.0"),
            false,
        ),
        (BString::from("1.0.0.0"), BString::from("dev-foo"), true),
        (BString::from("dev-foo"), BString::from("dev-foo"), true),
        (BString::from("dev-foo"), BString::from("1.0.0.0"), true),
    ]
}
