//! ref: composer/tests/Composer/Test/IO/NullIOTest.php sha1:8c4905f3700076910fcbe9397d238050e2c063fe

use bstr::BString;
use indexmap::{IndexMap, indexmap};
use shirabe::io::{IOInterfaceImmutable, IOInterfaceWrite, NullIO};
use shirabe_symfony_console::input::InputValue;

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
        InputValue::String(BString::from("foo")),
        io.ask("bar", InputValue::String(BString::from("foo")))
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
        InputValue::String(BString::from("foo")),
        io.ask_and_validate(
            "question",
            Box::new(|_x| Ok(InputValue::Bool(true))),
            None,
            InputValue::String(BString::from("foo"))
        )
        .unwrap()
    );
}

#[test]
fn test_select() {
    let io = NullIO::new();

    assert_eq!(
        InputValue::String(BString::from("1")),
        io.select(
            "question",
            indexmap! {
                BString::from("0") => BString::from("item1"),
                BString::from("1") => BString::from("item2"),
            },
            InputValue::String(BString::from("1")),
            Some(2),
            "foo",
            true
        )
        .unwrap()
    );
}
