//! ref: composer/tests/Composer/Test/Autoload/ClassLoaderTest.php

use shirabe::autoload::class_loader::ClassLoader;

#[test]
#[ignore = "depends on PHP runtime class_exists() to verify loadClass defined a class; no Rust equivalent"]
fn test_load_class() {
    // TODO(phase-d): loadClass() include()s a fixture and PHPUnit asserts via class_exists();
    // Rust has no equivalent of runtime class definition/loading.
    todo!()
}

#[test]
fn test_get_prefixes_with_no_psr0_configuration() {
    let loader = ClassLoader::new(None);
    assert!(loader.get_prefixes().is_empty());
}

#[test]
#[ignore = "depends on PHP serialize()/unserialize() round-trip of ClassLoader; no Rust equivalent"]
fn test_serializability() {
    // TODO(phase-d): serializes/unserializes the ClassLoader and compares every getter; PHP
    // serialize()/unserialize() has no Rust equivalent here.
    todo!()
}
