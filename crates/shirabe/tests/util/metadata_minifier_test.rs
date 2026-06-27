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
            "name".to_string() => PhpMixed::String("foo/bar".to_string()),
            "version".to_string() => PhpMixed::String("2.0.0".to_string()),
            "version_normalized".to_string() => PhpMixed::String("2.0.0.0".to_string()),
            "type".to_string() => PhpMixed::String("library".to_string()),
            "scripts".to_string() => PhpMixed::Array(indexmap! {
                "foo".to_string() => PhpMixed::List(vec![PhpMixed::String("bar".to_string())]),
            }),
            "license".to_string() => PhpMixed::List(vec![PhpMixed::String("MIT".to_string())]),
        },
        indexmap! {
            "version".to_string() => PhpMixed::String("1.2.0".to_string()),
            "version_normalized".to_string() => PhpMixed::String("1.2.0.0".to_string()),
            "license".to_string() => PhpMixed::List(vec![PhpMixed::String("GPL".to_string())]),
            "homepage".to_string() => PhpMixed::String("https://example.org".to_string()),
            "scripts".to_string() => PhpMixed::String("__unset".to_string()),
        },
        indexmap! {
            "version".to_string() => PhpMixed::String("1.0.0".to_string()),
            "version_normalized".to_string() => PhpMixed::String("1.0.0.0".to_string()),
            "homepage".to_string() => PhpMixed::String("__unset".to_string()),
        },
    ];

    let source = vec![
        dumper.dump(package1.into()),
        dumper.dump(package2.into()),
        dumper.dump(package3.into()),
    ];

    assert_eq!(source, MetadataMinifier::expand(minified));
}
