//! ref: composer/tests/Composer/Test/Json/JsonValidationExceptionTest.php sha1:b2be5018acb9f0ff3103dfce8ac215e649caacf5

use bstr::{BStr, BString, ByteSlice as _};
use shirabe::json::JsonValidationException;
use shirabe_php_shim::ToPhpString;

#[test]
fn test_get_errors() {
    for (message, errors, expected_message, expected_errors) in error_provider() {
        let object = JsonValidationException::new(
            message.to_php_string(),
            errors
                .iter()
                .map(|e| bstr::BString::from(e.as_bstr()))
                .collect(),
        );
        assert_eq!(expected_message, object.get_message());
        assert_eq!(&expected_errors, object.get_errors());
    }
}

#[test]
fn test_get_errors_when_no_errors_provided() {
    let object = JsonValidationException::new(BString::from("test message"), vec![]);
    assert_eq!(&Vec::<BString>::new(), object.get_errors());
}

fn error_provider() -> Vec<(&'static BStr, Vec<BString>, &'static BStr, Vec<BString>)> {
    vec![
        (
            BStr::new("test message"),
            vec![],
            BStr::new("test message"),
            vec![],
        ),
        (
            BStr::new(""),
            vec![BString::from("foo")],
            BStr::new(""),
            vec![BString::from("foo")],
        ),
    ]
}
