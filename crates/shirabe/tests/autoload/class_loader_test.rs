//! ref: composer/tests/Composer/Test/Autoload/ClassLoaderTest.php

use shirabe::autoload::class_loader::ClassLoader;

// ref: ClassLoaderTest::getLoadClassTests
fn get_find_file_tests() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Namespaced\\Foo", "Namespaced/Foo.php"),
        ("Pearlike_Foo", "Pearlike/Foo.php"),
        (
            "ShinyVendor\\ShinyPackage\\SubNamespace\\Foo",
            "SubNamespace/Foo.php",
        ),
    ]
}

// Tests ClassLoader::findFile instead of ClassLoader::loadClass in Rust. ClassLoader::loadClass
// calls `include`, which needs a PHP runtime. Shirabe resolves class files with findFile on the
// Rust side and lets the PHP worker `include` them, so the test asserts which file each class
// resolves to instead.
//
// ref: ClassLoaderTest::testLoadClass
#[test]
fn test_find_file() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Autoload/Fixtures")
        .canonicalize()
        .unwrap()
        .display()
        .to_string();

    for (class, file) in get_find_file_tests() {
        let mut loader = ClassLoader::new(None);
        loader.add("Namespaced\\", vec![fixtures.clone()], false);
        loader.add("Pearlike_", vec![fixtures.clone()], false);
        loader
            .add_psr4("ShinyVendor\\ShinyPackage\\", vec![fixtures.clone()], false)
            .unwrap();
        assert_eq!(
            loader.find_file(class),
            Some(format!("{}/{}", fixtures, file)),
            "->findFile() finds '{}'",
            class
        );
    }
}

#[test]
fn test_get_prefixes_with_no_psr0_configuration() {
    let loader = ClassLoader::new(None);
    assert!(loader.get_prefixes().is_empty());
}

#[test]
#[ignore = "the round trip is `unserialize(serialize($loader))`: shirabe_php_shim::serialize takes a PhpMixed (a ClassLoader cannot be turned into one) and there is no unserialize at all, so the ClassLoader under test cannot be round-tripped"]
fn test_serializability() {
    // TODO(php-semantics): the round trip is `unserialize(serialize($loader))`. serialize() in the shim
    // takes a PhpMixed, which a ClassLoader cannot be converted into, and there is no unserialize
    // symbol to produce the second ClassLoader the assertions compare against.
    todo!()
}
