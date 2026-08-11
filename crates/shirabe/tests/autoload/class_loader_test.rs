//! ref: composer/tests/Composer/Test/Autoload/ClassLoaderTest.php

use shirabe::autoload::class_loader::ClassLoader;
use shirabe_php_shim::class_exists;

/// ref: ClassLoaderTest::getLoadClassTests
fn get_load_class_tests() -> Vec<&'static str> {
    vec![
        "Namespaced\\Foo",
        "Pearlike_Foo",
        "ShinyVendor\\ShinyPackage\\SubNamespace\\Foo",
    ]
}

#[test]
#[ignore = "shirabe_php_shim::class_exists models a fixed set of classes available in a PHP CLI environment; loadClass cannot add to it because including a PHP file does not define a class on the Rust side"]
fn test_load_class() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../composer/tests/Composer/Test/Autoload/Fixtures")
        .canonicalize()
        .unwrap()
        .display()
        .to_string();

    for class in get_load_class_tests() {
        let mut loader = ClassLoader::new(None);
        loader.add("Namespaced\\", vec![fixtures.clone()], false);
        loader.add("Pearlike_", vec![fixtures.clone()], false);
        loader
            .add_psr4("ShinyVendor\\ShinyPackage\\", vec![fixtures.clone()], false)
            .unwrap();
        loader.load_class(class);
        assert!(class_exists(class), "->loadClass() loads '{}'", class);
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
