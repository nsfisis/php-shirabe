//! ref: composer/tests/Composer/Test/Util/MetadataMinifierTest.php

use bstr::BString;
use indexmap::{IndexMap, indexmap};
use shirabe::package::ArrayDumper;
use shirabe::package::handle::CompletePackageHandle;
use shirabe_metadata_minifier::MetadataMinifier;
use shirabe_php_shim::PhpMixed;

// MetadataMinifier::minify is not ported (intentionally absent in metadata_minifier.rs),
// so only the expand half of testMinifyExpand() is exercised here.
#[test]
fn test_minify_expand() {
    let package1 = CompletePackageHandle::new(
        BString::from("foo/bar"),
        BString::from("2.0.0.0"),
        BString::from("2.0.0"),
    );
    package1.set_scripts(
        IndexMap::from([(BString::from("foo"), vec![BString::from("bar")])])
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect::<Vec<_>>()))
            .collect::<indexmap::IndexMap<_, _>>(),
    );
    package1.set_license(vec![BString::from("MIT")]);
    let package2 = CompletePackageHandle::new(
        BString::from("foo/bar"),
        BString::from("1.2.0.0"),
        BString::from("1.2.0"),
    );
    package2.set_license(vec![BString::from("GPL")]);
    package2.set_homepage(BString::from("https://example.org"));
    let package3 = CompletePackageHandle::new(
        BString::from("foo/bar"),
        BString::from("1.0.0.0"),
        BString::from("1.0.0"),
    );
    package3.set_license(vec![BString::from("GPL")]);
    let dumper = ArrayDumper::new();

    let minified = vec![
        indexmap! {
            "name".into() => PhpMixed::string(BString::from("foo/bar")),
            "version".into() => PhpMixed::string(BString::from("2.0.0")),
            "version_normalized".into() => PhpMixed::string(BString::from("2.0.0.0")),
            "type".into() => PhpMixed::string(BString::from("library")),
            "scripts".into() => PhpMixed::Array(indexmap! {
                "foo".into() => PhpMixed::List(vec![PhpMixed::string(BString::from("bar"))]),
            }),
            "license".into() => PhpMixed::List(vec![PhpMixed::string(BString::from("MIT"))]),
        },
        indexmap! {
            "version".into() => PhpMixed::string(BString::from("1.2.0")),
            "version_normalized".into() => PhpMixed::string(BString::from("1.2.0.0")),
            "license".into() => PhpMixed::List(vec![PhpMixed::string(BString::from("GPL"))]),
            "homepage".into() => PhpMixed::string(BString::from("https://example.org")),
            "scripts".into() => PhpMixed::string(BString::from("__unset")),
        },
        indexmap! {
            "version".into() => PhpMixed::string(BString::from("1.0.0")),
            "version_normalized".into() => PhpMixed::string(BString::from("1.0.0.0")),
            "homepage".into() => PhpMixed::string(BString::from("__unset")),
        },
    ];

    let source = vec![
        dumper.dump(package1.into()).unwrap(),
        dumper.dump(package2.into()).unwrap(),
        dumper.dump(package3.into()).unwrap(),
    ];

    assert_eq!(source, MetadataMinifier::expand(minified).into_vec());
}
