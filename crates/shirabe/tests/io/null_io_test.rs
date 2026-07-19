//! ref: composer/tests/Composer/Test/IO/NullIOTest.php

use indexmap::IndexMap;
use shirabe::io::IOInterfaceImmutable;
use shirabe::io::null_io::NullIO;
use shirabe_php_shim::PhpMixed;

#[test]
fn test_is_interactive() {
    let io = NullIO::new();

    assert!(!io.is_interactive());
}

#[test]
fn test_has_authentication() {
    let io = NullIO::new();

    assert!(!io.has_authentication("foo"));
}

#[test]
fn test_ask_and_hide_answer() {
    let io = NullIO::new();

    assert_eq!(None, io.ask_and_hide_answer("foo".to_string()));
}

#[test]
fn test_get_authentications() {
    let io = NullIO::new();

    assert!(io.get_authentications().is_empty());

    let mut expected: IndexMap<String, Option<String>> = IndexMap::new();
    expected.insert("username".to_string(), None);
    expected.insert("password".to_string(), None);
    assert_eq!(expected, io.get_authentication("foo"));
}

#[test]
fn test_ask() {
    let io = NullIO::new();

    assert_eq!(
        PhpMixed::String("foo".to_string()),
        io.ask("bar".to_string(), PhpMixed::String("foo".to_string()))
    );
}

#[test]
fn test_ask_confirmation() {
    let io = NullIO::new();

    assert!(!io.ask_confirmation("bar".to_string(), false));
}

#[test]
fn test_ask_and_validate() {
    let io = NullIO::new();

    assert_eq!(
        PhpMixed::String("foo".to_string()),
        io.ask_and_validate(
            "question".to_string(),
            Box::new(|_x| Ok(PhpMixed::Bool(true))),
            None,
            PhpMixed::String("foo".to_string())
        )
        .unwrap()
    );
}

#[test]
fn test_select() {
    let io = NullIO::new();

    assert_eq!(
        PhpMixed::String("1".to_string()),
        io.select(
            "question".to_string(),
            PhpMixed::List(vec![
                PhpMixed::String("item1".to_string()),
                PhpMixed::String("item2".to_string()),
            ]),
            PhpMixed::String("1".to_string()),
            PhpMixed::Int(2),
            "foo".to_string(),
            true
        )
    );
}
