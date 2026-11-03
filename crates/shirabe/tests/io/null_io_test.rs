//! ref: composer/tests/Composer/Test/IO/NullIOTest.php

use bstr::BString;
use indexmap::IndexMap;
use shirabe::io::null_io::NullIO;
use shirabe::io::{IOInterfaceImmutable, IOInterfaceWrite};
use shirabe_php_shim::PhpMixed;

#[test]
fn test_is_interactive() {
    let io = NullIO::new();

    assert!(!io.is_interactive());
}

#[test]
fn test_has_authentication() {
    let io = NullIO::new();

    assert!(!io.has_authentication(bstr::BStr::new("foo")));
}

#[test]
fn test_ask_and_hide_answer() {
    let io = NullIO::new();

    assert_eq!(None, io.ask_and_hide_answer("foo").unwrap());
}

#[test]
fn test_get_authentications() {
    let io = NullIO::new();

    assert!(io.get_authentications().is_empty());

    let mut expected: IndexMap<bstr::BString, Option<bstr::BString>> = IndexMap::new();
    expected.insert(BString::from("username"), None);
    expected.insert(BString::from("password"), None);
    assert_eq!(expected, io.get_authentication(bstr::BStr::new("foo")));
}

#[test]
fn test_ask() {
    let io = NullIO::new();

    assert_eq!(
        PhpMixed::string(BString::from("foo")),
        io.ask("bar", PhpMixed::string(BString::from("foo")))
            .unwrap()
    );
}

#[test]
fn test_ask_confirmation() {
    let io = NullIO::new();

    assert!(!io.ask_confirmation("bar", false).unwrap());
}

#[test]
fn test_ask_and_validate() {
    let io = NullIO::new();

    assert_eq!(
        PhpMixed::string(BString::from("foo")),
        io.ask_and_validate(
            "question",
            Box::new(|_x| Ok(PhpMixed::Bool(true))),
            None,
            PhpMixed::string(BString::from("foo"))
        )
        .unwrap()
    );
}

#[test]
fn test_select() {
    let io = NullIO::new();

    assert_eq!(
        PhpMixed::string(BString::from("1")),
        io.select(
            "question",
            PhpMixed::List(vec![
                PhpMixed::string(BString::from("item1")),
                PhpMixed::string(BString::from("item2")),
            ]),
            PhpMixed::string(BString::from("1")),
            PhpMixed::Int(2),
            "foo",
            true
        )
        .unwrap()
    );
}
