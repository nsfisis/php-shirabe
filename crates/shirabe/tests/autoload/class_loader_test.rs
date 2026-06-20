//! ref: composer/tests/Composer/Test/Autoload/ClassLoaderTest.php

use shirabe::autoload::class_loader::ClassLoader;

// In PHP, loadClass() includes the matching .php fixture and the assertion checks
// class_exists(). There is no equivalent runtime notion of loading and defining a
// class in Rust, so this case cannot be ported faithfully.
#[test]
#[ignore = "relies on PHP loadClass()/class_exists() runtime class loading, which has no Rust equivalent"]
fn test_load_class() {
    todo!()
}

#[test]
fn test_get_prefixes_with_no_psr0_configuration() {
    let loader = ClassLoader::new(None);
    assert!(loader.get_prefixes().is_empty());
}

// In PHP this serializes the loader and unserializes it, then compares every getter.
// PHP serialize()/unserialize() has no Rust equivalent here.
#[test]
#[ignore = "relies on PHP serialize()/unserialize() round-tripping of the ClassLoader, which is not ported"]
fn test_serializability() {
    todo!()
}
