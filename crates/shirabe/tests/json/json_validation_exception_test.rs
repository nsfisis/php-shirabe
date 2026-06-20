//! ref: composer/tests/Composer/Test/Json/JsonValidationExceptionTest.php

use shirabe::json::json_validation_exception::JsonValidationException;

#[test]
fn test_get_errors() {
    for (message, errors, expected_message, expected_errors) in error_provider() {
        let object = JsonValidationException::new(message.to_string(), errors.clone());
        assert_eq!(expected_message, object.get_message());
        assert_eq!(&expected_errors, object.get_errors());
    }
}

#[test]
fn test_get_errors_when_no_errors_provided() {
    let object = JsonValidationException::new("test message".to_string(), vec![]);
    assert_eq!(&Vec::<String>::new(), object.get_errors());
}

fn error_provider() -> Vec<(&'static str, Vec<String>, &'static str, Vec<String>)> {
    vec![
        ("test message", vec![], "test message", vec![]),
        ("", vec!["foo".to_string()], "", vec!["foo".to_string()]),
    ]
}
