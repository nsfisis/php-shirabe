//! ref: composer/tests/Composer/Test/Util/MetadataMinifierTest.php

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
        "foo/bar".to_string(),
        "2.0.0.0".to_string(),
        "2.0.0".to_string(),
    );
    package1.set_scripts(IndexMap::from([(
        "foo".to_string(),
        vec!["bar".to_string()],
    )]));
    package1.set_license(vec!["MIT".to_string()]);
    let package2 = CompletePackageHandle::new(
        "foo/bar".to_string(),
        "1.2.0.0".to_string(),
        "1.2.0".to_string(),
    );
    package2.set_license(vec!["GPL".to_string()]);
    package2.set_homepage("https://example.org".to_string());
    let package3 = CompletePackageHandle::new(
        "foo/bar".to_string(),
        "1.0.0.0".to_string(),
        "1.0.0".to_string(),
    );
    package3.set_license(vec!["GPL".to_string()]);
    let dumper = ArrayDumper::new();

    let minified = vec![
        indexmap! {
            "name".into() => PhpMixed::string("foo/bar".to_string()),
            "version".into() => PhpMixed::string("2.0.0".to_string()),
            "version_normalized".into() => PhpMixed::string("2.0.0.0".to_string()),
            "type".into() => PhpMixed::string("library".to_string()),
            "scripts".into() => PhpMixed::Array(indexmap! {
                "foo".into() => PhpMixed::List(vec![PhpMixed::string("bar".to_string())]),
            }),
            "license".into() => PhpMixed::List(vec![PhpMixed::string("MIT".to_string())]),
        },
        indexmap! {
            "version".into() => PhpMixed::string("1.2.0".to_string()),
            "version_normalized".into() => PhpMixed::string("1.2.0.0".to_string()),
            "license".into() => PhpMixed::List(vec![PhpMixed::string("GPL".to_string())]),
            "homepage".into() => PhpMixed::string("https://example.org".to_string()),
            "scripts".into() => PhpMixed::string("__unset".to_string()),
        },
        indexmap! {
            "version".into() => PhpMixed::string("1.0.0".to_string()),
            "version_normalized".into() => PhpMixed::string("1.0.0.0".to_string()),
            "homepage".into() => PhpMixed::string("__unset".to_string()),
        },
    ];

    let source = vec![
        dumper.dump(package1.into()),
        dumper.dump(package2.into()),
        dumper.dump(package3.into()),
    ];

    assert_eq!(source, MetadataMinifier::expand(minified).into_vec());
}
